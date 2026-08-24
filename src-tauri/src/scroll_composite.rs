use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{Rgba, RgbaImage};

/// Composes sampled scroll frames in document coordinates.
///
/// Later-frame fixed overlays are excluded before the first-write-wins merge,
/// allowing overlapping content to back-fill covered pixels.
pub fn compose_scroll_frames(
    frames: &[RgbaImage],
    frame_offsets: &[u32],
    fixed_masks: &[(Vec<bool>, Vec<bool>)],
    total_height: u32,
) -> Result<String, String> {
    let first = frames.first().ok_or("長截圖沒有可拼接影格")?;
    if frames.len() != frame_offsets.len() {
        return Err("長截圖影格與位移資料不一致".into());
    }
    let frame_width = first.width();
    let frame_height = first.height();
    let mut composite = RgbaImage::from_pixel(frame_width, total_height, Rgba([255, 255, 255, 255]));
    let mut filled = vec![false; (frame_width * total_height) as usize];
    let mut blocked_rows = vec![vec![false; frame_height as usize]; frames.len()];
    let mut blocked_columns = vec![vec![false; frame_width as usize]; frames.len()];
    for (transition, (row_mask, column_mask)) in fixed_masks.iter().enumerate() {
        if transition + 1 >= frames.len() {
            return Err("長截圖固定區域資料不一致".into());
        }
        for (row, is_fixed) in row_mask.iter().enumerate() {
            if *is_fixed && row < frame_height as usize {
                blocked_rows[transition][row] = true;
                blocked_rows[transition + 1][row] = true;
            }
        }
        for (column, is_fixed) in column_mask.iter().enumerate() {
            if *is_fixed && column < frame_width as usize {
                blocked_columns[transition][column] = true;
                blocked_columns[transition + 1][column] = true;
            }
        }
    }
    for (frame_index, frame) in frames.iter().enumerate() {
        if frame.dimensions() != (frame_width, frame_height) {
            return Err("長截圖影格尺寸不一致".into());
        }
        let offset = frame_offsets[frame_index];
        for y in 0..frame_height {
            if blocked_rows[frame_index][y as usize] {
                continue;
            }
            let destination_y = offset + y;
            if destination_y >= total_height {
                continue;
            }
            for x in 0..frame_width {
                if blocked_columns[frame_index][x as usize] {
                    continue;
                }
                let index = (destination_y * frame_width + x) as usize;
                if !filled[index] {
                    composite.put_pixel(x, destination_y, *frame.get_pixel(x, y));
                    filled[index] = true;
                }
            }
        }
    }
    eprintln!("[scroll] composite frames={} output={}x{}", frames.len(), frame_width, total_height);
    let mut buffer = Vec::new();
    composite
        .write_to(&mut std::io::Cursor::new(&mut buffer), image::ImageFormat::Png)
        .map_err(|e| format!("Encode error: {e}"))?;
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(&buffer)))
}
