use crate::platform::windows::{DEFAULT_RECORDING_SHORTCUT, DEFAULT_SCREENSHOT_SHORTCUT};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub shortcut_screenshot: String,
    pub shortcut_recording: String,
    pub save_directory: String,
    pub remember_save_directory: bool,
    pub jpg_quality: u8,
    pub auto_copy_to_clipboard: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_close_to_tray() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        // Find default pictures folder
        let default_dir = dirs::picture_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
            .join("Screenshots");

        Self {
            shortcut_screenshot: DEFAULT_SCREENSHOT_SHORTCUT.to_string(),
            shortcut_recording: DEFAULT_RECORDING_SHORTCUT.to_string(),
            save_directory: default_dir.to_string_lossy().to_string(),
            remember_save_directory: true,
            jpg_quality: 90,
            auto_copy_to_clipboard: true,
            theme: default_theme(),
            close_to_tray: default_close_to_tray(),
        }
    }
}

// Module helper to resolve standard directories
mod dirs {
    use std::path::PathBuf;
    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
    }
    pub fn picture_dir() -> Option<PathBuf> {
        home_dir().map(|h| h.join("Pictures"))
    }
}

fn get_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get app config dir: {}", e))?;

    // Ensure the folder exists
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }

    Ok(app_dir.join("config.json"))
}

#[tauri::command]
pub fn load_config(app: AppHandle) -> Result<AppConfig, String> {
    let path = get_config_path(&app)?;
    if !path.exists() {
        let default_config = AppConfig::default();
        let json = serde_json::to_string_pretty(&default_config)
            .map_err(|e| format!("Serialize error: {}", e))?;
        fs::write(&path, json).map_err(|e| format!("Write config file error: {}", e))?;
        return Ok(default_config);
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Read config file error: {}", e))?;
    let config: AppConfig =
        serde_json::from_str(&content).map_err(|e| format!("Parse config error: {}", e))?;
    Ok(config)
}

#[tauri::command]
pub fn save_config(app: AppHandle, config: AppConfig) -> Result<(), String> {
    let path = get_config_path(&app)?;
    let previous = load_config(app.clone())?;
    // Do not persist a shortcut Windows rejected because another application
    // already owns it. Restore the old bindings before returning the error.
    if let Err(error) = crate::apply_global_shortcuts(&app, &config) {
        let _ = crate::apply_global_shortcuts(&app, &previous);
        return Err(error);
    }
    let json =
        serde_json::to_string_pretty(&config).map_err(|e| format!("Serialize error: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Write config file error: {}", e))?;
    Ok(())
}
