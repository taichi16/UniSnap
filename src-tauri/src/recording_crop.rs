use xcap::Frame;

/// Pixel-space crop resolved from the user-facing selection rectangle.
#[derive(Clone, Copy)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub fn resolve_crop(
    frame: &Frame,
    coordinate_scale: f64,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<Crop, String> {
    let px = ((x.max(0) as f64) * coordinate_scale).round() as u32;
    let py = ((y.max(0) as f64) * coordinate_scale).round() as u32;
    if px >= frame.width || py >= frame.height {
        return Err("錄影選取範圍位於螢幕外".into());
    }
    let mut pw = ((width as f64) * coordinate_scale).round() as u32;
    let mut ph = ((height as f64) * coordinate_scale).round() as u32;
    pw = pw.min(frame.width - px) & !1;
    ph = ph.min(frame.height - py) & !1;
    if pw < 2 || ph < 2 {
        return Err("錄影選取範圍太小".into());
    }
    if pw > u16::MAX as u32 || ph > u16::MAX as u32 {
        return Err("錄影解析度超過 MP4 支援範圍".into());
    }
    Ok(Crop {
        x: px,
        y: py,
        width: pw,
        height: ph,
    })
}

pub fn crop_rgba(frame: &Frame, crop: Crop) -> Result<Vec<u8>, String> {
    let expected = frame.width as usize * frame.height as usize * 4;
    if frame.raw.len() < expected {
        return Err("收到的螢幕影格資料不完整".into());
    }
    // Region capture already returns the requested rectangle. The first
    // startup frame is still a full-monitor image and uses original offsets.
    let crop = if crop.x + crop.width > frame.width || crop.y + crop.height > frame.height {
        Crop {
            x: 0,
            y: 0,
            width: frame.width,
            height: frame.height,
        }
    } else {
        crop
    };
    let row_bytes = crop.width as usize * 4;
    let mut out = Vec::with_capacity(row_bytes * crop.height as usize);
    for row in crop.y..crop.y + crop.height {
        let start = ((row * frame.width + crop.x) * 4) as usize;
        out.extend_from_slice(&frame.raw[start..start + row_bytes]);
    }
    Ok(out)
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
    fn resolves_scaled_even_bounded_crop() {
        let frame = test_frame(10, 8);
        let crop = resolve_crop(&frame, 2.0, 1, 1, 4, 3).unwrap();
        assert_eq!((crop.x, crop.y, crop.width, crop.height), (2, 2, 8, 6));
    }

    #[test]
    fn rejects_selection_outside_frame() {
        let frame = test_frame(10, 8);
        let error = match resolve_crop(&frame, 1.0, 10, 0, 2, 2) {
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
    fn falls_back_to_full_frame_for_startup_frame_mismatch() {
        let frame = test_frame(4, 3);
        let bytes = crop_rgba(
            &frame,
            Crop {
                x: 10,
                y: 10,
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        assert_eq!(bytes.len(), (4 * 3 * 4) as usize);
    }
}
