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
    Ok(format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(buffer)
    ))
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
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality);
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, ImageBuffer, Rgba};

    #[test]
    fn decodes_data_url_and_bare_base64_payloads() {
        let source = b"unisnap-image";
        let encoded = STANDARD.encode(source);
        assert_eq!(
            decode_data_url(&format!("data:image/png;base64,{encoded}")).unwrap(),
            source
        );
        assert_eq!(decode_data_url(&encoded).unwrap(), source);
    }

    #[test]
    fn rejects_invalid_base64_payload() {
        let error = decode_data_url("data:image/png;base64,not valid base64").unwrap_err();
        assert!(error.contains("Base64 decode error"));
    }

    #[test]
    fn encodes_rgba_preview_as_decodable_jpeg() {
        let image = ImageBuffer::from_fn(3, 2, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let data_url = rgba_to_jpeg_data_url(&image).unwrap();
        assert!(data_url.starts_with("data:image/jpeg;base64,"));
        let bytes = decode_data_url(&data_url).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(decoded.dimensions(), (3, 2));
    }
}
