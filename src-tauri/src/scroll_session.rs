use image::RgbaImage;

/// Owned state for one long-screenshot session.
pub struct ScrollCaptureSession {
    frames: Vec<RgbaImage>,
    offsets: Vec<u32>,
    fixed_masks: Vec<(Vec<bool>, Vec<bool>)>,
    total_height: u32,
}

impl ScrollCaptureSession {
    pub fn new(first_frame: RgbaImage) -> Self {
        let total_height = first_frame.height();
        Self { frames: vec![first_frame], offsets: vec![0], fixed_masks: Vec::new(), total_height }
    }

    pub fn frame_count(&self) -> usize { self.frames.len() }
    pub fn total_height(&self) -> u32 { self.total_height }

    pub fn append(
        &mut self,
        frame: RgbaImage,
        offset: u32,
        fixed_mask: (Vec<bool>, Vec<bool>),
        max_height: u32,
    ) -> Result<(), String> {
        if frame.dimensions() != self.frames[0].dimensions() {
            return Err("長截圖影格尺寸在擷取期間改變".to_string());
        }
        let total_height = frame.height().checked_add(offset)
            .ok_or_else(|| "長截圖高度溢位".to_string())?;
        if total_height > max_height {
            return Err(format!("長截圖超過安全高度 {} 像素", max_height));
        }
        self.frames.push(frame);
        self.offsets.push(offset);
        self.fixed_masks.push(fixed_mask);
        self.total_height = total_height;
        Ok(())
    }

    pub fn frames(&self) -> &[RgbaImage] { &self.frames }
    pub fn offsets(&self) -> &[u32] { &self.offsets }
    pub fn fixed_masks(&self) -> &[(Vec<bool>, Vec<bool>)] { &self.fixed_masks }
}

#[cfg(test)]
mod tests {
    use super::ScrollCaptureSession;
    use image::{Rgba, RgbaImage};

    fn frame(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]))
    }

    #[test]
    fn keeps_frames_offsets_and_masks_in_one_state() {
        let mut session = ScrollCaptureSession::new(frame(4, 3));
        session.append(frame(4, 3), 2, (vec![false], vec![true]), 20).unwrap();
        assert_eq!(session.frame_count(), 2);
        assert_eq!(session.frames().last().expect("appended frame").dimensions(), (4, 3));
        assert_eq!(session.total_height(), 5);
        assert_eq!(session.offsets(), &[0, 2]);
        assert_eq!(session.fixed_masks().len(), 1);
    }

    #[test]
    fn rejects_dimension_change_and_height_overflow() {
        let mut session = ScrollCaptureSession::new(frame(4, 3));
        assert!(session.append(frame(5, 3), 2, (vec![], vec![]), 20).is_err());
        assert!(session.append(frame(4, 3), 18, (vec![], vec![]), 20).is_err());
    }
}
