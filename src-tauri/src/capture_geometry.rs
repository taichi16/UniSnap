#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CropBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Converts a desktop-global work-area rectangle into image-local pixels.
#[allow(clippy::too_many_arguments)]
pub fn work_area_crop_bounds(
    monitor_x: i32,
    monitor_y: i32,
    work_x: i32,
    work_y: i32,
    work_width: u32,
    work_height: u32,
    image_width: u32,
    image_height: u32,
) -> Option<CropBounds> {
    let x = work_x.saturating_sub(monitor_x).max(0) as u32;
    let y = work_y.saturating_sub(monitor_y).max(0) as u32;
    if x >= image_width || y >= image_height {
        return None;
    }
    let width = work_width.min(image_width.saturating_sub(x));
    let height = work_height.min(image_height.saturating_sub(y));
    (width > 0 && height > 0).then_some(CropBounds { x, y, width, height })
}
