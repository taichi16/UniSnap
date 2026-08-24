use std::process::Command;
use std::time::Duration;

use crate::record_types::ScrollConfig;

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateMouseEvent(
        source: *mut std::ffi::c_void,
        mouse_type: u32,
        position: CGPoint,
        button: u32,
    ) -> *mut std::ffi::c_void;
    fn CGEventCreateScrollWheelEvent2(
        source: *mut std::ffi::c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> *mut std::ffi::c_void;
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
    fn CFRelease(cf: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

pub fn trigger_scroll_capture(config: ScrollConfig) -> Result<String, String> {
    let offset_x = config.monitor_offset_x.unwrap_or(0);
    let offset_y = config.monitor_offset_y.unwrap_or(0);
    let cx = (offset_x + config.x + (config.width as i32 / 2)) as f64;
    let cy = (offset_y + config.y + (config.height as i32 / 2)) as f64;
    let amount = config.scroll_amount.unwrap_or(5).max(0);

    #[cfg(target_os = "macos")]
    unsafe {
        let point = CGPoint { x: cx, y: cy };
        let mouse_down = CGEventCreateMouseEvent(std::ptr::null_mut(), 1, point, 0);
        if !mouse_down.is_null() {
            CGEventPost(0, mouse_down);
            CFRelease(mouse_down);
        }
        std::thread::sleep(Duration::from_millis(30));

        let mouse_up = CGEventCreateMouseEvent(std::ptr::null_mut(), 2, point, 0);
        if !mouse_up.is_null() {
            CGEventPost(0, mouse_up);
            CFRelease(mouse_up);
        }
        std::thread::sleep(Duration::from_millis(60));

        for _ in 0..amount {
            let event = CGEventCreateScrollWheelEvent2(std::ptr::null_mut(), 0, 1, -280, 0, 0);
            if !event.is_null() {
                CGEventPost(0, event);
                CFRelease(event);
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"System Events\" to repeat {} times\nkey code 125\ndelay 0.03\nend repeat",
            amount * 2
        );
        let _ = Command::new("osascript").arg("-e").arg(script).output();
    }

    Ok("scroll_done".to_string())
}
