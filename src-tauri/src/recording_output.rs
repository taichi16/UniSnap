use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::AppHandle;

/// Allocates the next MP4 output path using the user's configured directory.
pub fn recording_output_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config = crate::config::load_config(app.clone()).unwrap_or_default();
    let directory = PathBuf::from(config.save_directory);
    fs::create_dir_all(&directory).map_err(|e| format!("無法建立錄影存檔資料夾：{e}"))?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    Ok(directory.join(format!("ScreenRec_{millis}.mp4")))
}
