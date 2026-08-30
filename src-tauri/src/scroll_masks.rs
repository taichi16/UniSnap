use image::RgbaImage;

/// Marks contiguous unchanged rows that are likely fixed headers or overlays.
pub fn fixed_row_mask(before: &RgbaImage, after: &RgbaImage) -> Vec<bool> {
    let width = after.width();
    let height = after.height();
    let mut candidates = vec![false; height as usize];
    for y in 0..height {
        let mut difference = 0u64;
        let mut sum = 0f64;
        let mut sum_sq = 0f64;
        let mut samples = 0u64;
        for x in (0..width).step_by(8) {
            let a = before.get_pixel(x, y);
            let b = after.get_pixel(x, y);
            difference += (a[0] as i32 - b[0] as i32).unsigned_abs() as u64
                + (a[1] as i32 - b[1] as i32).unsigned_abs() as u64
                + (a[2] as i32 - b[2] as i32).unsigned_abs() as u64;
            let luminance = (b[0] as f64 + b[1] as f64 + b[2] as f64) / 3.0;
            sum += luminance;
            sum_sq += luminance * luminance;
            samples += 1;
        }
        let mean = sum / samples.max(1) as f64;
        let variance = sum_sq / samples.max(1) as f64 - mean * mean;
        candidates[y as usize] = samples > 0
            && (difference as f64 / (samples * 3) as f64) < 4.0
            && variance > 80.0;
    }
    contiguous_mask(candidates)
}

/// Marks contiguous unchanged columns that are likely fixed sidebars.
pub fn fixed_column_mask(before: &RgbaImage, after: &RgbaImage) -> Vec<bool> {
    let width = after.width();
    let height = after.height();
    let mut candidates = vec![false; width as usize];
    for x in 0..width {
        let mut difference = 0u64;
        let mut sum = 0f64;
        let mut sum_sq = 0f64;
        let mut samples = 0u64;
        for y in (0..height).step_by(8) {
            let a = before.get_pixel(x, y);
            let b = after.get_pixel(x, y);
            difference += (a[0] as i32 - b[0] as i32).unsigned_abs() as u64
                + (a[1] as i32 - b[1] as i32).unsigned_abs() as u64
                + (a[2] as i32 - b[2] as i32).unsigned_abs() as u64;
            let luminance = (b[0] as f64 + b[1] as f64 + b[2] as f64) / 3.0;
            sum += luminance;
            sum_sq += luminance * luminance;
            samples += 1;
        }
        let mean = sum / samples.max(1) as f64;
        let variance = sum_sq / samples.max(1) as f64 - mean * mean;
        candidates[x as usize] = samples > 0
            && (difference as f64 / (samples * 3) as f64) < 4.0
            && variance > 80.0;
    }
    contiguous_mask(candidates)
}

fn contiguous_mask(candidates: Vec<bool>) -> Vec<bool> {
    // Isolated unchanged pixels are normal content. Keep only substantial
    // contiguous bands, preserving the original 16-pixel threshold.
    let mut mask = vec![false; candidates.len()];
    let mut start = 0usize;
    while start < candidates.len() {
        if !candidates[start] {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < candidates.len() && candidates[end] {
            end += 1;
        }
        if end - start >= 16 {
            mask[start..end].fill(true);
        }
        start = end;
    }
    mask
}
