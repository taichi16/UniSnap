use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::RgbaImage;

/// Encodes an RGBA capture as the JPEG data URL used by preview windows.
pub fn rgba_to_jpeg_data_url(image: &RgbaImage) -> Result<String, String> {
    let rgb = image::DynamicImage::ImageRgba8(image.clone()).into_rgb8();
    let mut buffer = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, 95);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("Failed to encode image to JPEG: {error}"))?;
    Ok(format!("data:image/jpeg;base64,{}", STANDARD.encode(buffer)))
}

/// Decodes either a data URL or a bare Base64 payload.
pub fn decode_data_url(data: &str) -> Result<Vec<u8>, String> {
    let payload = data
        .strip_prefix("data:")
        .and_then(|value| value.split_once(',').map(|(_, payload)| payload))
        .unwrap_or(data);
    STANDARD
        .decode(payload)
        .map_err(|error| format!("Base64 decode error: {error}"))
}

/// Keeps PNG bytes as-is and only re-encodes the image for a JPEG destination.
pub fn encode_requested_image(
    source: &[u8],
    path: &str,
    app: &tauri::AppHandle,
) -> Result<Vec<u8>, String> {
    let is_jpeg = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| matches!(extension.to_ascii_lowercase().as_str(), "jpg" | "jpeg"))
        .unwrap_or(false);
    if !is_jpeg {
        return Ok(source.to_vec());
    }

    let quality = crate::config::load_config(app.clone())
        .map(|config| config.jpg_quality)
        .unwrap_or(90)
        .clamp(1, 100);
    let rgb = image::load_from_memory(source)
        .map_err(|error| format!("無法轉換 JPG：{error}"))?
        .to_rgb8();
    let mut output = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality as u8);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("JPG 編碼失敗：{error}"))?;
    Ok(output)
}
