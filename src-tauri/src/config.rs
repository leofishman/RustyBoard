use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::path::BaseDirectory;
use tauri::Manager;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub shortcut: String,
    pub persist_sensitive: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            shortcut: "ctrl+shift+v".to_string(),
            persist_sensitive: false,
        }
    }
}

pub fn get_config_path(app: &AppHandle) -> PathBuf {
    app.path()
        .resolve("config.json", BaseDirectory::AppConfig)
        .unwrap_or_else(|_| PathBuf::from("config.json"))
}

pub fn load_config(app: &AppHandle) -> AppConfig {
    let path = get_config_path(app);
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(config) = serde_json::from_str(&content) {
            return config;
        }
    }
    AppConfig::default()
}

pub fn save_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}
