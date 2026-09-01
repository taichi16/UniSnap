use xcap::Frame;

/// Pixel-space crop resolved from the user-facing selection rectangle.
#[derive(Clone, Copy)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Maps the rectangle drawn on the preview canvas to the exact physical
/// pixels in the captured monitor. Independent X/Y ratios avoid DPI rounding
/// errors on mixed-scale Windows desktops.
pub fn resolve_crop_from_canvas(
    frame: &Frame,
    canvas_width: u32,
    canvas_height: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<Crop, String> {
    if canvas_width == 0 || canvas_height == 0 {
        return Err("錄影選取畫布尺寸無效".to_string());
    }
    let scale_x = frame.width as f64 / canvas_width as f64;
    let scale_y = frame.height as f64 / canvas_height as f64;
    let left = ((x.max(0) as f64) * scale_x).round() as u32;
    let top = ((y.max(0) as f64) * scale_y).round() as u32;
    if left >= frame.width || top >= frame.height {
        return Err("錄影選取範圍位於螢幕外".into());
    }
    let right_logical = (x as i64 + width as i64).max(0) as f64;
    let bottom_logical = (y as i64 + height as i64).max(0) as f64;
    let right = (right_logical * scale_x).round() as u32;
    let bottom = (bottom_logical * scale_y).round() as u32;
    let mut pixel_width = right.min(frame.width).saturating_sub(left) & !1;
    let mut pixel_height = bottom.min(frame.height).saturating_sub(top) & !1;
    pixel_width = pixel_width.min(frame.width - left);
    pixel_height = pixel_height.min(frame.height - top);
    if pixel_width < 2 || pixel_height < 2 {
        return Err("錄影選取範圍太小".into());
    }
    if pixel_width > u16::MAX as u32 || pixel_height > u16::MAX as u32 {
        return Err("錄影解析度超過 MP4 支援範圍".into());
    }
    Ok(Crop {
        x: left,
        y: top,
        width: pixel_width,
        height: pixel_height,
    })
}

fn effective_crop(frame: &Frame, crop: Crop) -> Result<Crop, String> {
    if frame.width == crop.width && frame.height == crop.height {
        return Ok(Crop {
            x: 0,
            y: 0,
            width: crop.width,
            height: crop.height,
        });
    }
    if crop.x + crop.width > frame.width || crop.y + crop.height > frame.height {
        return Err(format!(
            "錄影來源尺寸由 {}x{} 改變，無法維持固定輸出 {}x{}",
            frame.width, frame.height, crop.width, crop.height
        ));
    }
    Ok(crop)
}

pub fn crop_rgba(frame: &Frame, crop: Crop) -> Result<Vec<u8>, String> {
    let expected = frame.width as usize * frame.height as usize * 4;
    if frame.raw.len() < expected {
        return Err("收到的螢幕影格資料不完整".into());
    }
    // Region capture already returns the requested rectangle. The first
    // startup frame is still a full-monitor image and uses original offsets.
    let crop = effective_crop(frame, crop)?;
    let row_bytes = crop.width as usize * 4;
    let mut out = Vec::with_capacity(row_bytes * crop.height as usize);
    for row in crop.y..crop.y + crop.height {
        let start = ((row * frame.width + crop.x) * 4) as usize;
        out.extend_from_slice(&frame.raw[start..start + row_bytes]);
    }
    Ok(out)
}

/// Crops into a reusable buffer for the real-time Windows encoder. Unlike the
/// compatibility helper above, this rejects a changing source size because a
/// Native video encoders require every frame to have identical dimensions.
pub fn crop_rgba_into(frame: &Frame, crop: Crop, output: &mut Vec<u8>) -> Result<(), String> {
    let expected = frame.width as usize * frame.height as usize * 4;
    if frame.raw.len() < expected {
        return Err("收到的螢幕影格資料不完整".into());
    }
    let crop = effective_crop(frame, crop)?;
    let row_bytes = crop.width as usize * 4;
    let required = row_bytes * crop.height as usize;
    output.clear();
    if output.capacity() < required {
        output.reserve(required - output.capacity());
    }
    for row in crop.y..crop.y + crop.height {
        let start = ((row * frame.width + crop.x) * 4) as usize;
        output.extend_from_slice(&frame.raw[start..start + row_bytes]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_frame(width: u32, height: u32) -> Frame {
        let mut raw = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                raw.extend_from_slice(&[x as u8, y as u8, 0, 255]);
            }
        }
        Frame::new(width, height, raw)
    }

    #[test]
    fn resolves_even_bounded_crop() {
        let frame = test_frame(10, 8);
        let crop = resolve_crop_from_canvas(&frame, 5, 4, 1, 1, 4, 3).unwrap();
        assert_eq!((crop.x, crop.y, crop.width, crop.height), (2, 2, 8, 6));
    }

    #[test]
    fn resolves_crop_using_actual_canvas_dimensions() {
        let frame = test_frame(2560, 1600);
        let crop = resolve_crop_from_canvas(&frame, 1707, 1067, 32, 20, 1293, 946).unwrap();
        assert_eq!(
            (crop.x, crop.y, crop.width, crop.height),
            (48, 30, 1938, 1418)
        );
    }

    #[test]
    fn reusable_crop_accepts_already_cropped_region_frame() {
        let frame = test_frame(4, 2);
        let mut output = Vec::new();
        crop_rgba_into(
            &frame,
            Crop {
                x: 100,
                y: 200,
                width: 4,
                height: 2,
            },
            &mut output,
        )
        .unwrap();
        assert_eq!(output, frame.raw);
    }

    #[test]
    fn rejects_selection_outside_frame() {
        let frame = test_frame(10, 8);
        let error = match resolve_crop_from_canvas(&frame, 10, 8, 10, 0, 2, 2) {
            Ok(_) => panic!("expected out-of-frame crop to fail"),
            Err(error) => error,
        };
        assert!(error.contains("螢幕外"));
    }

    #[test]
    fn crops_expected_rows_and_columns() {
        let frame = test_frame(4, 3);
        let bytes = crop_rgba(
            &frame,
            Crop {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        assert_eq!(
            bytes,
            vec![1, 1, 0, 255, 2, 1, 0, 255, 1, 2, 0, 255, 2, 2, 0, 255]
        );
    }

    #[test]
    fn rejects_unrelated_source_dimensions() {
        let frame = test_frame(4, 3);
        let result = crop_rgba(
            &frame,
            Crop {
                x: 10,
                y: 10,
                width: 2,
                height: 2,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn reusable_crop_rejects_source_size_changes() {
        let frame = test_frame(4, 3);
        let mut output = Vec::new();
        assert!(crop_rgba_into(
            &frame,
            Crop {
                x: 3,
                y: 2,
                width: 2,
                height: 2,
            },
            &mut output,
        )
        .is_err());
    }
}
