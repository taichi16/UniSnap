use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::io::Cursor;
use std::path::Path;

pub struct EditorImageData {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

/// Reads a supported local image and normalizes it to the PNG data URL used by
/// both editor entry points.
pub fn load_editor_image(path: &Path) -> Result<EditorImageData, String> {
    if !path.is_file() {
        return Err(format!("找不到圖片檔案：{}", path.display()));
    }
    let source = std::fs::read(path)
        .map_err(|error| format!("無法讀取圖片 {}：{error}", path.display()))?;
    let image = image::load_from_memory(&source).map_err(|error| format!("無法開啟圖片：{error}"))?;
    let width = image.width();
    let height = image.height();
    let mut encoded = Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| format!("無法準備圖片編輯資料：{error}"))?;
    Ok(EditorImageData {
        data_url: format!("data:image/png;base64,{}", STANDARD.encode(encoded.into_inner())),
        width,
        height,
    })
}
