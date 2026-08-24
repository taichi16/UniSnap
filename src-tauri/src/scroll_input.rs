#[cfg(not(target_os = "windows"))]
use enigo::{Axis, Coordinate, Enigo, Mouse, Settings};

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra_info: usize);
}

/// Cross-platform pointer and scroll event sender used by scroll capture.
pub struct ScrollController {
    #[cfg(not(target_os = "windows"))]
    input: Enigo,
}

impl ScrollController {
    pub fn new() -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        {
            Ok(Self {})
        }
        #[cfg(not(target_os = "windows"))]
        {
            let input = Enigo::new(&Settings::default())
                .map_err(|e| format!("Failed to initialize input control: {e}"))?;
            Ok(Self { input })
        }
    }

    pub fn position_pointer(&mut self, x: i32, y: i32) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        unsafe {
            if SetCursorPos(x, y) == 0 {
                return Err("Failed to position pointer".to_string());
            }
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.input
                .move_mouse(x, y, Coordinate::Abs)
                .map_err(|e| format!("Failed to position pointer: {e}"))
        }
    }

    pub fn scroll_down(&mut self, lines: i32) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        unsafe {
            const MOUSEEVENTF_WHEEL: u32 = 0x0800;
            const WHEEL_DELTA: i32 = 120;
            let delta = -lines.saturating_mul(WHEEL_DELTA);
            mouse_event(MOUSEEVENTF_WHEEL, 0, 0, delta as u32, 0);
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.input
                .scroll(lines, Axis::Vertical)
                .map_err(|e| format!("Failed to scroll target window: {e}"))
        }
    }
}
