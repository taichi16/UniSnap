use image::RgbaImage;

fn sampled_difference(img1: &RgbaImage, img2: &RgbaImage, shift: u32) -> Option<f64> {
    let (width, height) = img1.dimensions();
    if width != img2.width() || height != img2.height() || height < 200 {
        return None;
    }
    let left = width / 8;
    let right = width.saturating_sub(width / 8);
    let top = height / 8;
    let bottom = height.saturating_sub(height / 10);
    if top + shift + 24 >= bottom || left + 24 >= right {
        return None;
    }
    let mut difference = 0u64;
    let mut samples = 0u64;
    for y in (top..bottom - shift).step_by(8) {
        for x in (left..right).step_by(10) {
            let before = img1.get_pixel(x, y + shift);
            let after = img2.get_pixel(x, y);
            difference += (before[0] as i32 - after[0] as i32).unsigned_abs() as u64
                + (before[1] as i32 - after[1] as i32).unsigned_abs() as u64
                + (before[2] as i32 - after[2] as i32).unsigned_abs() as u64;
            samples += 3;
        }
    }
    (samples > 0).then_some(difference as f64 / samples as f64)
}

pub fn frames_are_stable(img1: &RgbaImage, img2: &RgbaImage) -> bool {
    sampled_difference(img1, img2, 0).is_some_and(|difference| difference < 2.5)
}

pub fn find_scroll_shift(img1: &RgbaImage, img2: &RgbaImage) -> Option<u32> {
    let (_, height) = img1.dimensions();
    if img1.dimensions() != img2.dimensions() || height < 200 {
        return None;
    }
    let zero_difference = sampled_difference(img1, img2, 0)?;
    if zero_difference < 2.5 {
        return None;
    }
    let max_shift = (height * 3 / 4).max(8);
    let mut best = None::<(u32, f64)>;
    for shift in (4..=max_shift).step_by(4) {
        if let Some(difference) = sampled_difference(img1, img2, shift) {
            if best.is_none_or(|(_, best_difference)| difference < best_difference) {
                best = Some((shift, difference));
            }
        }
    }
    let (coarse_shift, _) = best?;
    let start = coarse_shift.saturating_sub(3).max(1);
    let end = (coarse_shift + 3).min(max_shift);
    for shift in start..=end {
        if let Some(difference) = sampled_difference(img1, img2, shift) {
            if best.is_none_or(|(_, best_difference)| difference < best_difference) {
                best = Some((shift, difference));
            }
        }
    }
    let (shift, difference) = best?;
    (difference < 28.0 && difference + 1.0 < zero_difference).then_some(shift)
}

pub fn find_scroll_shift_near(img1: &RgbaImage, img2: &RgbaImage, expected: u32) -> Option<u32> {
    let zero_difference = sampled_difference(img1, img2, 0)?;
    if zero_difference < 2.5 {
        return None;
    }
    let min_shift = expected.saturating_sub(48).max(1);
    let max_shift = expected
        .saturating_add(48)
        .min(img1.height().saturating_sub(25));
    let mut best = None::<(u32, f64)>;
    for shift in min_shift..=max_shift {
        if let Some(difference) = sampled_difference(img1, img2, shift) {
            if best.is_none_or(|(_, best_difference)| difference < best_difference) {
                best = Some((shift, difference));
            }
        }
    }
    let (shift, difference) = best?;
    (difference < 28.0 && difference + 1.0 < zero_difference).then_some(shift)
}
