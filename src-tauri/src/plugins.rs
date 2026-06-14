use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::path::BaseDirectory;
use tauri::Manager;
use std::process::{Command, Stdio};
use std::io::Write;
use crate::security::{sanitize_text, sanitize_svg, classify_sensitivity, Sensitivity};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginResponse {
    pub success: bool,
    pub result_raw_content: String,
    pub result_display_content: String,
    pub sensitivity: Sensitivity,
    pub error: Option<String>,
}

pub fn get_plugins_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .resolve("plugins", BaseDirectory::AppConfig)
        .unwrap_or_else(|_| PathBuf::from("plugins"))
}

pub fn init_plugins_dir(app: &AppHandle) -> Result<(), String> {
    let dir = get_plugins_dir(app);
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        // Create an example plugin
        let example = PluginDefinition {
            id: "uppercase".to_string(),
            name: "Uppercase Converter".to_string(),
            description: "Converts text to uppercase".to_string(),
            command: "tr".to_string(),
            args: vec!["a-z".to_string(), "A-Z".to_string()],
        };
        let example_path = dir.join("uppercase.json");
        let content = serde_json::to_string_pretty(&example).map_err(|e| e.to_string())?;
        fs::write(example_path, content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn load_plugins(app: &AppHandle) -> Result<Vec<PluginDefinition>, String> {
    let dir = get_plugins_dir(app);
    let mut plugins = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(plugin) = serde_json::from_str::<PluginDefinition>(&content) {
                        plugins.push(plugin);
                    }
                }
            }
        }
    }
    Ok(plugins)
}

pub fn execute_plugin(app: &AppHandle, plugin_id: &str, input_text: &str) -> Result<PluginResponse, String> {
    let plugins = load_plugins(app)?;
    let plugin = plugins.into_iter().find(|p| p.id == plugin_id)
        .ok_or_else(|| format!("Plugin {} not found", plugin_id))?;

    let dir = get_plugins_dir(app);

    let mut child = Command::new(&plugin.command)
        .args(&plugin.args)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn plugin command: {}", e))?;

    // Drop stdin reference to avoid deadlock if child reads until EOF
    if let Some(mut stdin) = child.stdin.take() {
        // Spawn a thread to write to stdin to prevent deadlock with large payloads
        let input = input_text.to_string();
        std::thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
    }

    let output = child.wait_with_output().map_err(|e| e.to_string())?;

    if output.status.success() {
        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();

        // Security: Sanitize output from plugin
        let trimmed = stdout_str.trim();
        let (display_content, sensitivity) = if trimmed.starts_with("<svg") && trimmed.ends_with("</svg>") {
            match sanitize_svg(&stdout_str) {
                Ok(clean) => (clean, Sensitivity::None),
                Err(_) => (sanitize_text(&stdout_str), classify_sensitivity(&stdout_str)),
            }
        } else {
            (sanitize_text(&stdout_str), classify_sensitivity(&stdout_str))
        };

        Ok(PluginResponse {
            success: true,
            result_raw_content: stdout_str,
            result_display_content: display_content,
            sensitivity,
            error: None,
        })
    } else {
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
        Ok(PluginResponse {
            success: false,
            result_raw_content: "".to_string(),
            result_display_content: "".to_string(),
            sensitivity: Sensitivity::None,
            error: Some(stderr_str),
        })
    }
}
