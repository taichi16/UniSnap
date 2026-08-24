use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::RgbaImage;
use tauri::AppHandle;

/// Encodes and saves a full-screen or work-area capture using shared settings.
pub fn save_capture_png(
    app: AppHandle,
    image: &RgbaImage,
    filename_prefix: &str,
) -> Result<String, String> {
    let config = crate::config::load_config(app.clone()).unwrap_or_default();
    let mut buffer = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut buffer), image::ImageFormat::Png)
        .map_err(|e| format!("Encode error: {e}"))?;
    let base64_image = STANDARD.encode(&buffer);
    let filename = format!(
        "{}_{}.png",
        filename_prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let path = std::path::PathBuf::from(&config.save_directory).join(filename);
    let path_str = path.to_string_lossy().into_owned();
    let _ = crate::capture::save_and_copy_screenshot(
        app,
        base64_image,
        Some(path_str.clone()),
        config.auto_copy_to_clipboard,
    );
    Ok(path_str)
}
