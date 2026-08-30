use image::RgbaImage;

use crate::scroll_target::ScrollCaptureStrategy;

pub fn strategy_label(strategy: ScrollCaptureStrategy) -> &'static str {
    match strategy {
        ScrollCaptureStrategy::BrowserPage => "browser-page",
        ScrollCaptureStrategy::DocumentApp => "document-app",
        ScrollCaptureStrategy::DesktopStitch => "desktop-stitch",
    }
}

pub fn crop_selection(
    frame: &RgbaImage,
    crop_x: u32,
    crop_y: u32,
    width: u32,
    height: u32,
) -> Result<RgbaImage, String> {
    if crop_x >= frame.width()
        || crop_y >= frame.height()
        || crop_x.saturating_add(width) > frame.width()
        || crop_y.saturating_add(height) > frame.height()
    {
        return Err(format!(
            "選取範圍超出目標視窗：selection=({},{} {}x{}) window-frame={}x{}",
            crop_x, crop_y, width, height, frame.width(), frame.height()
        ));
    }
    Ok(image::imageops::crop_imm(frame, crop_x, crop_y, width, height).to_image())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn labels_each_capture_strategy() {
        assert_eq!(strategy_label(ScrollCaptureStrategy::BrowserPage), "browser-page");
        assert_eq!(strategy_label(ScrollCaptureStrategy::DocumentApp), "document-app");
        assert_eq!(strategy_label(ScrollCaptureStrategy::DesktopStitch), "desktop-stitch");
    }

    #[test]
    fn crops_valid_selection_and_rejects_out_of_bounds() {
        let frame = RgbaImage::from_pixel(4, 3, Rgba([1, 2, 3, 255]));
        assert_eq!(crop_selection(&frame, 1, 1, 2, 2).unwrap().dimensions(), (2, 2));
        assert!(crop_selection(&frame, 3, 1, 2, 2).is_err());
    }
}
