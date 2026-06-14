mod security;
mod config;
mod database;
mod plugins;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use serde::{Serialize, Deserialize};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use std::hash::{Hash, Hasher};
use clipboard_master::{ClipboardHandler, CallbackResult, Master};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: String,
    pub raw_content: String,      // Intact original content (not sent to UI)
    pub display_content: String,  // Sanitized content for UI display
    pub content_type: String,     // "text" or "image"
    pub sensitivity: security::Sensitivity,
    pub timestamp: u64,
    pub thumbnail: Option<String>,
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

struct AppState {
    db_path: std::path::PathBuf,
    history: Mutex<Vec<ClipboardItem>>,
    // Stores raw content of the last item written by the app itself
    // to prevent clipboard monitor feedback loops.
    last_written: Mutex<Option<String>>,
    // True while a debounced tray-menu rebuild is already scheduled, so bursts
    // of operations (e.g. deleting several items) coalesce into one rebuild
    // instead of piling heavy work onto the GTK main thread.
    tray_update_pending: AtomicBool,
}

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Result<Vec<UIClipboardItem>, String> {
    let history = state.history.lock().map_err(|e| e.to_string())?;
    let ui_items: Vec<UIClipboardItem> = history.iter().map(|item| item.clone().into()).collect();
    Ok(ui_items)
}

fn copy_item_by_id(app: &AppHandle, id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
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

fn update_tray_menu(app: &AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = update_tray_menu_impl(&app_handle) {
            eprintln!("Error updating tray menu on main thread: {}", e);
        }
    }).map_err(|e| e.to_string())
}

// Debounced tray rebuild. Rebuilding the tray menu (decoding image thumbnails,
// calling `set_menu`) runs on the GTK main thread, which also drives the
// WebView; doing it synchronously on every operation can freeze the UI and
// starve IPC when several happen in quick succession. This coalesces a burst
// into a single rebuild shortly after the activity settles.
fn schedule_tray_update(app: &AppHandle) {
    let state = app.state::<AppState>();
    // If a rebuild is already scheduled, let it cover this change too.
    if state.tray_update_pending.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let state = app.state::<AppState>();
        state.tray_update_pending.store(false, Ordering::SeqCst);
        let _ = update_tray_menu(&app);
    });
}

fn update_tray_menu_impl(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let history_items: Vec<ClipboardItem> = {
        let history = state.history.lock().map_err(|e| e.to_string())?;
        history.iter().take(15).cloned().collect()
    };
    
    // Build menu
    let mut menu_builder = tauri::menu::MenuBuilder::new(app);
    
    // Add history items
    if history_items.is_empty() {
        let empty_i = tauri::menu::MenuItem::with_id(app, "empty_placeholder", "(No clips yet)", false, None::<&str>).map_err(|e| e.to_string())?;
        menu_builder = menu_builder.item(&empty_i);
    } else {
        for item in history_items.iter() { // Show top 15 items in tray menu
            let mut label;
            let mut item_icon = None;

            if item.content_type == "image" {
                let base64_str = if item.display_content.starts_with("data:image/png;base64,") {
                    &item.display_content["data:image/png;base64,".len()..]
                } else {
                    &item.raw_content
                };

                let size_kb = if let Ok(bytes) = BASE64.decode(base64_str) {
                    bytes.len() as f64 / 1024.0
                } else {
                    0.0
                };
                label = format!("[Image - {:.1} KB]", size_kb);

                // Use pre-generated thumbnail if available
                if let Some(ref thumb_b64) = item.thumbnail {
                    if let Ok(rgba_data) = BASE64.decode(thumb_b64) {
                        if rgba_data.len() == 18 * 18 * 4 {
                            item_icon = Some(tauri::image::Image::new_owned(rgba_data, 18, 18));
                        }
                    }
                }

                // Fallback for older entries
                if item_icon.is_none() {
                    if let Ok(bytes) = BASE64.decode(base64_str) {
                        if let Ok(img) = image::load_from_memory(&bytes) {
                            let thumbnail = img.resize_exact(18, 18, image::imageops::FilterType::Lanczos3);
                            let rgba_data = thumbnail.into_rgba8().into_raw();
                            item_icon = Some(tauri::image::Image::new_owned(rgba_data, 18, 18));
                        }
                    }
                }
            } else {
                label = item.display_content.clone();
                // Replace newlines with spaces for clean display in menu
                label = label.replace('\n', " ").replace('\r', "");
                // Truncate label to 50 chars for clean display
                if label.chars().count() > 50 {
                    label = label.chars().take(47).collect::<String>() + "...";
                }
                if label.trim().is_empty() {
                    label = "[Empty Content]".to_string();
                }
            }

            // Mask secrets / credentials
            if item.sensitivity == security::Sensitivity::Secret {
                label = "•••••••• [Secret]".to_string();
                item_icon = None;
            } else if item.sensitivity == security::Sensitivity::Credential {
                label = "•••••••• [Credential]".to_string();
                item_icon = None;
            }

            // Create custom menu item with item id
            if let Some(icon_img) = item_icon {
                let clip_i = tauri::menu::IconMenuItem::with_id(
                    app,
                    &item.id,
                    &label,
                    true,
                    Some(icon_img),
                    None::<&str>,
                ).map_err(|e| e.to_string())?;
                menu_builder = menu_builder.item(&clip_i);
            } else {
                let clip_i = tauri::menu::MenuItem::with_id(
                    app,
                    &item.id,
                    &label,
                    true,
                    None::<&str>,
                ).map_err(|e| e.to_string())?;
                menu_builder = menu_builder.item(&clip_i);
            }
        }
    }
    
    // Add separator
    let separator = tauri::menu::PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    menu_builder = menu_builder.item(&separator);
    
    // Add static items
    let show_i = tauri::menu::MenuItem::with_id(app, "show", "Show RustyBoard", true, None::<&str>).map_err(|e| e.to_string())?;
    let quit_i = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).map_err(|e| e.to_string())?;
    
    menu_builder = menu_builder.item(&show_i).item(&quit_i);
    
    let menu = menu_builder.build().map_err(|e| e.to_string())?;
    
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_menu(Some(menu));
    }
    
    Ok(())
}

#[tauri::command]
fn copy_to_clipboard(app: AppHandle, id: String) -> Result<(), String> {
    copy_item_by_id(&app, &id)
}

#[tauri::command]
async fn delete_clipboard_item(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    
    // 1. Remove from in-memory history immediately
    {
        let mut history = state.history.lock().map_err(|e| e.to_string())?;
        history.retain(|item| item.id != id);
    }

    // 2. Update tray menu (debounced so rapid deletes don't freeze the main thread)
    schedule_tray_update(&app);

    // 3. Spawn database removal asynchronously (non-blocking)
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let _ = database::delete_item(&state.db_path, &id);
    });
    
    Ok(())
}

#[tauri::command]
async fn clear_all_history(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    
    // 1. Clear in-memory history immediately
    {
        let mut history = state.history.lock().map_err(|e| e.to_string())?;
        history.clear();
    }

    // 2. Update tray menu (debounced)
    schedule_tray_update(&app);

    // 3. Spawn database clear asynchronously (non-blocking)
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let _ = database::clear_all(&state.db_path);
    });
    
    Ok(())
}

#[tauri::command]
fn get_shortcut(app: AppHandle) -> String {
    let config = config::load_config(&app);
    config.shortcut
}

#[tauri::command]
fn set_shortcut(app: AppHandle, shortcut_str: String) -> Result<(), String> {
    use std::str::FromStr;

    let new_shortcut = Shortcut::from_str(&shortcut_str)
        .map_err(|_| "Invalid shortcut format".to_string())?;

    let mut config = config::load_config(&app);
    let old_shortcut_str = config.shortcut.clone();

    // Register new shortcut
    app.global_shortcut()
        .register(new_shortcut)
        .map_err(|e| format!("Failed to register new shortcut: {}", e))?;

    // Unregister old shortcut (if it changed)
    if old_shortcut_str != shortcut_str {
        if let Ok(old_shortcut) = Shortcut::from_str(&old_shortcut_str) {
            let _ = app.global_shortcut().unregister(old_shortcut);
        }
    }

    // Save configuration
    config.shortcut = shortcut_str;
    config::save_config(&app, &config)?;

    Ok(())
}

#[tauri::command]
fn get_plugins(app: AppHandle) -> Result<Vec<plugins::PluginDefinition>, String> {
    plugins::load_plugins(&app)
}

#[tauri::command]
fn run_plugin(app: AppHandle, plugin_id: String, item_id: String) -> Result<UIClipboardItem, String> {
    let state = app.state::<AppState>();

    let (raw_content, content_type) = {
        let history = state.history.lock().map_err(|e| e.to_string())?;
        let item = history.iter().find(|x| x.id == item_id)
            .ok_or_else(|| "Item not found".to_string())?;
        (item.raw_content.clone(), item.content_type.clone())
    };

    if content_type != "text" {
        return Err("Plugins only support text content currently.".to_string());
    }

    let response = plugins::execute_plugin(&app, &plugin_id, &raw_content)?;

    if !response.success {
        return Err(response.error.unwrap_or_else(|| "Unknown plugin error".to_string()));
    }

    let new_item = ClipboardItem {
        id: uuid::Uuid::new_v4().to_string(),
        raw_content: response.result_raw_content,
        display_content: response.result_display_content,
        content_type: "text".to_string(),
        sensitivity: response.sensitivity,
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
        thumbnail: None,
    };

    // Save item in memory.
    {
        let mut history = state.history.lock().unwrap();
        history.insert(0, new_item.clone());
        if history.len() > 100 {
            history.pop();
        }
    }

    // Emit to the UI first so the plugin result shows up instantly.
    let new_item_id = new_item.id.clone();
    let ui_item: UIClipboardItem = new_item.clone().into();
    let _ = app.emit("clipboard-changed", ui_item.clone());
    schedule_tray_update(&app);

    // Copy the output of the plugin to the system clipboard.
    let _ = copy_item_by_id(&app, &new_item_id);

    // Persist in the background so disk I/O never delays the UI.
    let app_handle = app.clone();
    let db_path = state.db_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let config = config::load_config(&app_handle);
        let _ = database::save_item(&db_path, &new_item, config.persist_level);
        let _ = database::run_cleanup(&db_path, config.persist_level);
    });

    Ok(ui_item)
}

#[tauri::command]
fn get_persist_level(app: AppHandle) -> String {
    let config = config::load_config(&app);
    format!("{:?}", config.persist_level)
}

#[tauri::command]
fn set_persist_level(app: AppHandle, value: String) -> Result<(), String> {
    let mut config = config::load_config(&app);
    let level = match value.as_str() {
        "Sensitive" => config::PersistLevel::Sensitive,
        "All" => config::PersistLevel::All,
        _ => config::PersistLevel::None,
    };
    config.persist_level = level;
    config::save_config(&app, &config)?;
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

fn generate_image_thumbnail(image_data: &arboard::ImageData<'_>) -> Option<String> {
    use image::{ImageBuffer, Rgba};
    let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(
        image_data.width as u32,
        image_data.height as u32,
        image_data.bytes.to_vec(),
    )?;
    
    let resized = image::imageops::resize(&buffer, 18, 18, image::imageops::FilterType::Lanczos3);
    let raw_bytes = resized.into_raw();
    Some(BASE64.encode(raw_bytes))
}

struct ClipboardMonitor {
    app_handle: AppHandle,
    last_text: Mutex<String>,
    last_image_hash: Mutex<Option<u64>>,
}

impl ClipboardMonitor {
    fn process_text(&mut self, state: &State<'_, AppState>, text: String) {
        let trimmed = text.trim();
        let mut last_t = self.last_text.lock().unwrap();
        if !trimmed.is_empty() && text != *last_t {
            let is_own_write = {
                let last_w = state.last_written.lock().unwrap();
                last_w.as_ref() == Some(&text)
            };

            *last_t = text.clone();
            let mut last_img = self.last_image_hash.lock().unwrap();
            *last_img = None; // Reset image comparator

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
                    thumbnail: None,
                };

                self.save_and_emit(state, new_item);
            }
        }
    }

    fn process_image(&mut self, state: &State<'_, AppState>, image_data: arboard::ImageData) {
        let bytes = &*image_data.bytes;
        let image_hash = calculate_hash(&bytes);

        let mut last_img = self.last_image_hash.lock().unwrap();
        if Some(image_hash) != *last_img {
            let mut last_t = self.last_text.lock().unwrap();
            *last_t = String::new(); // Reset text comparator
            *last_img = Some(image_hash);

            if let Ok(png_base64) = encode_image_to_png(&image_data) {
                let is_own_write = {
                    let last_w = state.last_written.lock().unwrap();
                    last_w.as_ref() == Some(&png_base64)
                };

                if is_own_write {
                    let mut last_w = state.last_written.lock().unwrap();
                    *last_w = None;
                } else {
                    let thumbnail = generate_image_thumbnail(&image_data);
                    let new_item = ClipboardItem {
                        id: uuid::Uuid::new_v4().to_string(),
                        raw_content: png_base64.clone(),
                        display_content: format!("data:image/png;base64,{}", png_base64),
                        content_type: "image".to_string(),
                        sensitivity: security::Sensitivity::None,
                        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                        thumbnail,
                    };

                    self.save_and_emit(state, new_item);
                }
            }
        }
    }

    fn save_and_emit(&self, state: &State<'_, AppState>, new_item: ClipboardItem) {
        // 1. Update in-memory history.
        {
            let mut history = state.history.lock().unwrap();
            history.insert(0, new_item.clone());
            if history.len() > 100 {
                history.pop();
            }
        }

        // 2. Emit to the UI *first* so the new clip appears at the top instantly,
        // before any disk I/O.
        let ui_item: UIClipboardItem = new_item.clone().into();
        let _ = self.app_handle.emit("clipboard-changed", ui_item);

        // 3. Rebuild the tray menu (debounced, off the main-thread hot path).
        schedule_tray_update(&self.app_handle);

        // 4. Persist to SQLite and run cleanup in the background so disk work
        // never delays the UI update.
        let app_handle = self.app_handle.clone();
        let db_path = state.db_path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let config = config::load_config(&app_handle);
            let _ = database::save_item(&db_path, &new_item, config.persist_level);
            let _ = database::run_cleanup(&db_path, config.persist_level);
        });
    }
}

impl ClipboardHandler for ClipboardMonitor {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(_) => return CallbackResult::Next,
        };

        // 1. Try to read text
        if let Ok(text) = clipboard.get_text() {
            let app_handle = self.app_handle.clone();
            let state_ref = app_handle.state::<AppState>();
            self.process_text(&state_ref, text);
        }
        // 2. Try to read image
        else if let Ok(image_data) = clipboard.get_image() {
            let app_handle = self.app_handle.clone();
            let state_ref = app_handle.state::<AppState>();
            self.process_image(&state_ref, image_data);
        }

        CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, _error: std::io::Error) -> CallbackResult {
        CallbackResult::Next
    }
}

  fn start_clipboard_monitor(app_handle: AppHandle) {
      thread::spawn(move || {
          let monitor = ClipboardMonitor {
              app_handle,
              last_text: Mutex::new(String::new()),
              last_image_hash: Mutex::new(None),
          };
          if let Ok(mut master) = Master::new(monitor) {
              let _ = master.run();
          }
      });
  }

  #[cfg_attr(mobile, tauri::mobile_entry_point)]
  pub fn run() {
      tauri::Builder::default()
          .plugin(tauri_plugin_opener::init())
          .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
              if let Some(window) = app.get_webview_window("main") {
                  let _ = window.show();
                  let _ = window.set_focus();
                  let _ = window.emit("window-shown", ());
              }
          }))
          .setup(|app| {
              let handle = app.handle().clone();

              // 1. Resolve DB Path and Initialize DB
              let db_path = handle.path()
                  .resolve("history.db", tauri::path::BaseDirectory::AppData)
                  .unwrap_or_else(|_| std::path::PathBuf::from("history.db"));
              
              let _ = database::init_db(&db_path);
              let _ = database::run_cleanup(&db_path, config::load_config(&handle).persist_level);

              // 2. Initialize Plugin Directory
              let _ = plugins::init_plugins_dir(&handle);

              // 3. Load History from SQLite
              let loaded_history = database::load_history(&db_path).unwrap_or_default();

              let app_state = AppState {
                  db_path,
              history: Mutex::new(loaded_history),
                  last_written: Mutex::new(None),
                  tray_update_pending: AtomicBool::new(false),
              };
              app.manage(app_state);

              // 4. Start Clipboard Monitor
              start_clipboard_monitor(handle.clone());

              // 4b. Start Periodic Cleanup Loop (every 60 seconds)
              let periodic_handle = handle.clone();
              tauri::async_runtime::spawn(async move {
                  let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
                  loop {
                      interval.tick().await;
                      if let Some(state) = periodic_handle.try_state::<AppState>() {
                          let config = config::load_config(&periodic_handle);
                          
                          // In Unrestricted mode, do not run cleanup at all
                          if config.persist_level == config::PersistLevel::All {
                              continue;
                          }

                          let now = std::time::SystemTime::now()
                              .duration_since(std::time::UNIX_EPOCH)
                              .map(|d| d.as_secs())
                              .unwrap_or(0);
                          let cutoff = now - 7200; // 2 hours in seconds
                          
                          let mut changed = false;
                          if let Ok(mut history) = state.history.lock() {
                              let before_len = history.len();
                              history.retain(|item| {
                                  match config.persist_level {
                                      config::PersistLevel::None => {
                                          // Paranoid mode: delete Personal, Credential, Secret
                                          !(item.sensitivity != security::Sensitivity::None && item.timestamp < cutoff)
                                      }
                                      config::PersistLevel::Sensitive => {
                                          // Balanced mode: delete Credential, Secret
                                          !(matches!(item.sensitivity, security::Sensitivity::Credential | security::Sensitivity::Secret) && item.timestamp < cutoff)
                                      }
                                      config::PersistLevel::All => true,
                                  }
                              });
                              if history.len() != before_len {
                                  changed = true;
                              }
                          }
                          
                          // Unrestricted mode already `continue`d above, so this
                          // only runs for None/Balanced.
                          let _ = database::run_cleanup(&state.db_path, config.persist_level);
                          
                          if changed {
                              let _ = update_tray_menu(&periodic_handle);
                              if let Ok(history) = state.history.lock() {
                                  let ui_history: Vec<UIClipboardItem> = history.iter().map(|item| item.clone().into()).collect();
                                  let _ = periodic_handle.emit("history-synced", ui_history);
                              }
                          }
                      }
                  }
              });

              // 5. Initialize Global Shortcut Plugin with window toggle handler
              let global_shortcut_plugin = tauri_plugin_global_shortcut::Builder::new()
                  .with_handler(move |app, shortcut, event| {
                      if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                          let config = config::load_config(app);
                          use std::str::FromStr;
                          if let Ok(configured_shortcut) = Shortcut::from_str(&config.shortcut) {
                              if shortcut == &configured_shortcut {
                                  if let Some(window) = app.get_webview_window("main") {
                                      if let Ok(visible) = window.is_visible() {
                                          if visible {
                                              let _ = window.hide();
                                          } else {
                                              let _ = window.show();
                                              let _ = window.set_focus();
                                              let _ = window.emit("window-shown", ());
                                          }
                                      }
                                  }
                              }
                          }
                      }
                  })
                  .build();

              app.handle().plugin(global_shortcut_plugin)?;

              // Register initial shortcut from config
              let config = config::load_config(&handle);
              use std::str::FromStr;
              if let Ok(initial_shortcut) = Shortcut::from_str(&config.shortcut) {
                  let _ = handle.global_shortcut().register(initial_shortcut);
              }

              // Build Tray Icon with custom ID "main" and register events
              let _tray = tauri::tray::TrayIconBuilder::with_id("main")
                  .icon(app.default_window_icon().unwrap().clone())
                  .on_menu_event(|app: &tauri::AppHandle, event: tauri::menu::MenuEvent| {
                      let id = event.id().as_ref();
                      match id {
                          "quit" => {
                              app.exit(0);
                          }
                          "show" => {
                              if let Some(window) = app.get_webview_window("main") {
                                  let _ = window.show();
                                  let _ = window.set_focus();
                                  let _ = window.emit("window-shown", ());
                              }
                          }
                          "empty_placeholder" => {}
                          clip_id => {
                              if let Err(e) = copy_item_by_id(app, clip_id) {
                                  eprintln!("Error copying item from tray menu: {}", e);
                              }
                          }
                      }
                  })
                  .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event: tauri::tray::TrayIconEvent| {
                      if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                          let app = tray.app_handle();
                          if let Some(window) = app.get_webview_window("main") {
                              if let Ok(visible) = window.is_visible() {
                                  if visible {
                                      let _ = window.hide();
                                  } else {
                                      let _ = window.show();
                                      let _ = window.set_focus();
                                      let _ = window.emit("window-shown", ());
                                  }
                              }
                          }
                      }
                  })
                  .build(app)?;

              // Populate initial tray menu items
              let _ = update_tray_menu(app.handle());

              Ok(())
          })
          .on_window_event(|window, event| {
              if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                  let _ = window.hide();
                  api.prevent_close();
              }
          })
          .invoke_handler(tauri::generate_handler![
              get_history,
              copy_to_clipboard,
              delete_clipboard_item,
              clear_all_history,
              get_shortcut,
              set_shortcut,
              get_persist_level,
              set_persist_level,
              get_plugins,
              run_plugin
          ])
          .run(tauri::generate_context!())
          .expect("error while running tauri application");
  }
