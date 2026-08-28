//! Windows-specific policy that is safe to call from the platform-neutral
//! capture and configuration modules.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Convert an overlay rectangle expressed in logical points into physical
/// screen pixels. Windows mixed-DPI layouts can produce non-integral values;
/// rounding is done once at this boundary so callers do not mix units.
pub fn logical_to_physical_rect(
    monitor_x: i32,
    monitor_y: i32,
    scale_factor: f64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> PhysicalRect {
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };

    PhysicalRect {
        x: monitor_x.saturating_add(round_to_i32(x * scale)),
        y: monitor_y.saturating_add(round_to_i32(y * scale)),
        width: round_to_u32(width * scale),
        height: round_to_u32(height * scale),
    }
}

fn round_to_i32(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

fn round_to_u32(value: f64) -> u32 {
    if !value.is_finite() {
        return 1;
    }
    value.round().clamp(1.0, u32::MAX as f64) as u32
}

/// Windows mouse-wheel input is expressed in multiples of 120. Positive
/// logical lines mean downward scrolling in UniSnap, while Win32 uses a
/// negative wheel delta for that direction.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn wheel_delta_for_down_lines(lines: i32) -> i32 {
    lines.saturating_mul(-120)
}

pub const DEFAULT_SCREENSHOT_SHORTCUT: &str = "Alt+A";
pub const DEFAULT_RECORDING_SHORTCUT: &str = "Alt+R";

pub fn validate_recording_options(
    record_audio: bool,
    record_system_audio: bool,
) -> Result<(), String> {
    let _ = record_audio;
    let _ = record_system_audio;
    #[cfg(not(target_os = "windows"))]
    if record_system_audio {
        return Err("系統音訊錄製需要在 Windows WASAPI 環境執行".to_string());
    }
    Ok(())
}

pub fn validate_recording_fps(fps: u32) -> Result<(), String> {
    if (1..=60).contains(&fps) {
        Ok(())
    } else {
        Err("錄影 FPS 必須介於 1 到 60 之間".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_mixed_dpi_overlay_coordinates_once() {
        assert_eq!(
            logical_to_physical_rect(1920, -100, 1.5, 10.4, 20.4, 100.4, 80.4),
            PhysicalRect {
                x: 1936,
                y: -69,
                width: 151,
                height: 121,
            }
        );
    }

    #[test]
    fn invalid_scale_falls_back_to_one() {
        assert_eq!(
            logical_to_physical_rect(0, 0, 0.0, 1.4, 2.4, 10.4, 20.4),
            PhysicalRect {
                x: 1,
                y: 2,
                width: 10,
                height: 20,
            }
        );
    }

    #[test]
    fn clamps_non_finite_and_overflowing_geometry_to_safe_bounds() {
        assert_eq!(
            logical_to_physical_rect(
                i32::MAX,
                i32::MIN,
                f64::INFINITY,
                f64::NAN,
                f64::INFINITY,
                f64::NAN,
                f64::INFINITY,
            ),
            PhysicalRect {
                x: i32::MAX,
                y: i32::MIN,
                width: 1,
                height: 1,
            }
        );
    }

    #[test]
    fn preserves_negative_overlay_offsets_without_wrapping() {
        assert_eq!(
            logical_to_physical_rect(-1920, 0, 1.5, -200.0, 10.0, 100.0, 50.0),
            PhysicalRect {
                x: -2220,
                y: 15,
                width: 150,
                height: 75,
            }
        );
    }

    #[test]
    fn saturates_wheel_delta_at_integer_bounds() {
        assert_eq!(wheel_delta_for_down_lines(i32::MIN), i32::MAX);
        assert_eq!(wheel_delta_for_down_lines(i32::MAX), i32::MIN);
    }

    #[test]
    fn downward_wheel_delta_is_negative() {
        assert_eq!(wheel_delta_for_down_lines(6), -720);
        assert_eq!(wheel_delta_for_down_lines(-2), 240);
    }

    #[test]
    fn system_audio_requires_windows_runtime() {
        assert!(validate_recording_options(false, false).is_ok());
        assert!(validate_recording_options(true, false).is_ok());
        #[cfg(not(target_os = "windows"))]
        assert!(validate_recording_options(false, true).is_err());
    }

    #[test]
    fn recording_fps_must_be_within_supported_range() {
        assert!(validate_recording_fps(1).is_ok());
        assert!(validate_recording_fps(60).is_ok());
        assert!(validate_recording_fps(0).is_err());
        assert!(validate_recording_fps(61).is_err());
    }
}
