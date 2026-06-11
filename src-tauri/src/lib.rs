mod security;

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use serde::{Serialize, Deserialize};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use std::hash::{Hash, Hasher};

#[derive(Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    id: String,
    raw_content: String,      // Intact original content (not sent to UI)
    display_content: String,  // Sanitized content for UI display
    content_type: String,     // "text" or "image"
    sensitivity: security::Sensitivity,
    timestamp: u64,
}

// Representing the ClipboardItem structure sent to the UI
#[derive(Clone, Serialize, Deserialize)]
pub struct UIClipboardItem {
    id: String,
    display_content: String,
    content_type: String,
    sensitivity: security::Sensitivity,
    timestamp: u64,
}

impl From<ClipboardItem> for UIClipboardItem {
    fn from(item: ClipboardItem) -> Self {
        UIClipboardItem {
            id: item.id,
            display_content: item.display_content,
            content_type: item.content_type,
            sensitivity: item.sensitivity,
            timestamp: item.timestamp,
        }
    }
}

#[derive(Default)]
struct AppState {
    history: Mutex<Vec<ClipboardItem>>,
    // Stores raw content of the last item written by the app itself
    // to prevent clipboard monitor feedback loops.
    last_written: Mutex<Option<String>>,
}

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Result<Vec<UIClipboardItem>, String> {
    let history = state.history.lock().map_err(|e| e.to_string())?;
    let ui_items: Vec<UIClipboardItem> = history.iter().map(|item| item.clone().into()).collect();
    Ok(ui_items)
}

#[tauri::command]
fn copy_to_clipboard(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let history = state.history.lock().map_err(|e| e.to_string())?;
    let item = history.iter().find(|x| x.id == id)
        .ok_or_else(|| "Item not found".to_string())?;

    // Update last_written flag before copy operation
    {
        let mut last_w = state.last_written.lock().map_err(|e| e.to_string())?;
        *last_w = Some(item.raw_content.clone());
    }

    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;

    if item.content_type == "image" {
        // Decode base64 to image bytes
        let base64_data = if item.raw_content.contains(',') {
            item.raw_content.split(',').nth(1).unwrap_or(&item.raw_content)
        } else {
            &item.raw_content
        };
        let bytes = BASE64.decode(base64_data).map_err(|e| e.to_string())?;
        
        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();

        let img_data = arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: std::borrow::Cow::Owned(rgba.into_raw()),
        };
        clipboard.set_image(img_data).map_err(|e| e.to_string())?;
    } else {
        clipboard.set_text(item.raw_content.clone()).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

fn encode_image_to_png(image_data: &arboard::ImageData<'_>) -> Result<String, String> {
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;

    let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(
        image_data.width as u32,
        image_data.height as u32,
        image_data.bytes.to_vec(),
    ).ok_or("Invalid image dimensions")?;

    let mut png_bytes = Vec::new();
    buffer.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    Ok(BASE64.encode(png_bytes))
}

fn start_clipboard_monitor(app_handle: AppHandle) {
    thread::spawn(move || {
        let mut last_text = String::new();
        let mut last_image_hash: Option<u64> = None;

        loop {
            thread::sleep(Duration::from_millis(500));

            let state = app_handle.state::<AppState>();
            let mut clipboard = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(_) => continue,
            };

            // 1. Try to read text
            if let Ok(text) = clipboard.get_text() {
                let trimmed = text.trim();
                if !trimmed.is_empty() && text != last_text {
                    let is_own_write = {
                        let last_w = state.last_written.lock().unwrap();
                        last_w.as_ref() == Some(&text)
                    };

                    // Reset comparator for text
                    last_text = text.clone();
                    last_image_hash = None; // Reset image comparator

                    if is_own_write {
                        // Clean loop flag
                        let mut last_w = state.last_written.lock().unwrap();
                        *last_w = None;
                    } else {
                        // Process text
                        let (display_content, sensitivity) = if trimmed.starts_with("<svg") && trimmed.ends_with("</svg>") {
                            match security::sanitize_svg(&text) {
                                Ok(clean) => (clean, security::Sensitivity::None),
                                Err(_) => (security::sanitize_text(&text), security::classify_sensitivity(&text)),
                            }
                        } else {
                            (security::sanitize_text(&text), security::classify_sensitivity(&text))
                        };

                        let new_item = ClipboardItem {
                            id: uuid::Uuid::new_v4().to_string(),
                            raw_content: text.clone(),
                            display_content,
                            content_type: "text".to_string(),
                            sensitivity,
                            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                        };

                        {
                            let mut history = state.history.lock().unwrap();
                            history.insert(0, new_item.clone());
                            if history.len() > 50 {
                                history.pop();
                            }
                        }

                        let ui_item: UIClipboardItem = new_item.into();
                        let _ = app_handle.emit("clipboard-changed", ui_item);
                    }
                }
            }
            // 2. Try to read image
            else if let Ok(image_data) = clipboard.get_image() {
                let bytes = &*image_data.bytes;
                let image_hash = calculate_hash(&bytes);

                if Some(image_hash) != last_image_hash {
                    // Reset text comparator since we now have an image
                    last_text = String::new();
                    last_image_hash = Some(image_hash);

                    let png_base64 = match encode_image_to_png(&image_data) {
                        Ok(base64_str) => base64_str,
                        Err(_) => continue,
                    };

                    let is_own_write = {
                        let last_w = state.last_written.lock().unwrap();
                        last_w.as_ref() == Some(&png_base64)
                    };

                    if is_own_write {
                        let mut last_w = state.last_written.lock().unwrap();
                        *last_w = None;
                    } else {
                        let new_item = ClipboardItem {
                            id: uuid::Uuid::new_v4().to_string(),
                            raw_content: png_base64.clone(),
                            display_content: format!("data:image/png;base64,{}", png_base64),
                            content_type: "image".to_string(),
                            sensitivity: security::Sensitivity::None,
                            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                        };

                        {
                            let mut history = state.history.lock().unwrap();
                            history.insert(0, new_item.clone());
                            if history.len() > 50 {
                                history.pop();
                            }
                        }

                        let ui_item: UIClipboardItem = new_item.into();
                        let _ = app_handle.emit("clipboard-changed", ui_item);
                    }
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            start_clipboard_monitor(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_history, copy_to_clipboard])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
