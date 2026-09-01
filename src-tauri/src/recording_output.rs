use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn verify_output_directory(directory: &std::path::Path, nonce: u128) -> Result<(), String> {
    let probe = directory.join(format!(
        ".unisnap-write-test-{}-{nonce}.tmp",
        std::process::id()
    ));
    let result = OpenOptions::new().write(true).create_new(true).open(&probe);
    match result {
        Ok(file) => {
            drop(file);
            fs::remove_file(&probe)
                .map_err(|error| format!("錄影存檔資料夾可建立檔案，但無法清理測試檔：{error}"))
        }
        Err(error) => Err(format!(
            "錄影存檔資料夾無法寫入，請在設定中改選本機資料夾：{error}"
        )),
    }
}

/// Allocates the next MP4 output path using the user's configured directory.
pub fn recording_output_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config = crate::config::load_config(app.clone()).unwrap_or_default();
    let directory = PathBuf::from(config.save_directory);
    if directory.as_os_str().is_empty() {
        return Err("錄影存檔資料夾未設定，請先在設定中選擇資料夾".to_string());
    }
    fs::create_dir_all(&directory).map_err(|e| format!("無法建立錄影存檔資料夾：{e}"))?;
    let millis = timestamp_millis();
    verify_output_directory(&directory, millis)?;
    Ok(directory.join(format!("ScreenRec_{millis}.mp4")))
}

/// Uses the app-local cache for encoder output. Windows MediaTranscoder is not
/// reliable when its working file is on redirected, synchronized or network
/// folders, so only the completed MP4 is published to the configured folder.
pub fn recording_staging_path(
    app: &AppHandle,
    output_path: &std::path::Path,
) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("無法取得 UniSnap 本機暫存資料夾：{error}"))?
        .join("recordings");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("無法建立 UniSnap 本機暫存資料夾：{error}"))?;
    Ok(staging_path(&directory, output_path, std::process::id()))
}

fn staging_path(
    directory: &std::path::Path,
    output_path: &std::path::Path,
    process_id: u32,
) -> PathBuf {
    let stem = output_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("ScreenRec");
    directory.join(format!("{stem}-{process_id}.partial.mp4"))
}

#[cfg(test)]
mod tests {
    use super::{staging_path, verify_output_directory};
    use std::fs;

    #[test]
    fn staging_file_stays_inside_app_cache_directory() {
        let cache = std::path::Path::new(r"C:\Users\tester\AppData\Local\UniSnap\recordings");
        let output = std::path::Path::new(r"Z:\redirected\ScreenRec_123.mp4");
        let staging = staging_path(cache, output, 42);

        assert_eq!(staging.parent(), Some(cache));
        assert_eq!(
            staging.file_name().and_then(|name| name.to_str()),
            Some("ScreenRec_123-42.partial.mp4")
        );
    }

    #[test]
    fn output_probe_is_removed_after_success() {
        let directory = std::env::temp_dir().join(format!(
            "unisnap-output-probe-{}-{}",
            std::process::id(),
            super::timestamp_millis()
        ));
        fs::create_dir_all(&directory).unwrap();

        verify_output_directory(&directory, 123).unwrap();
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);

        fs::remove_dir(&directory).unwrap();
    }
}
