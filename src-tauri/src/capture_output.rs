use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::RgbaImage;
use tauri::AppHandle;

/// Encodes an in-memory image for editor/long-capture handoff without writing
/// it to disk. Keeping this beside the normal capture output avoids duplicate
/// PNG serialization paths.
pub fn encode_png_data_url(image: &RgbaImage) -> Result<String, String> {
    let mut buffer = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut buffer),
            image::ImageFormat::Png,
        )
        .map_err(|error| format!("Encode error: {error}"))?;
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(buffer)))
}

/// Encodes and saves a full-screen or work-area capture using shared settings.
pub fn save_capture_png(
    app: AppHandle,
    image: &RgbaImage,
    filename_prefix: &str,
) -> Result<String, String> {
    let config = crate::config::load_config(app.clone()).unwrap_or_default();
    let base64_image = encode_png_data_url(image)?
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| "PNG data URL 格式錯誤".to_string())?
        .to_string();
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
    let _ = crate::capture_io::save_and_copy_screenshot(
        app,
        base64_image,
        Some(path_str.clone()),
        config.auto_copy_to_clipboard,
    );
    Ok(path_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba};

    #[test]
    fn encodes_png_data_url_with_original_dimensions() {
        let image = RgbaImage::from_pixel(4, 3, Rgba([12, 34, 56, 255]));
        let data_url = encode_png_data_url(&image).unwrap();
        assert!(data_url.starts_with("data:image/png;base64,"));
        let bytes = STANDARD
            .decode(data_url.split_once(',').unwrap().1)
            .unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().dimensions(), (4, 3));
    }
}
