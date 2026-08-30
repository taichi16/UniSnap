use tauri::AppHandle;
use xcap::Window;

/// Physical-pixel selection and target window resolved from overlay points.
pub struct ResolvedScrollTarget {
    pub window: Window,
    pub coordinate_scale: f64,
    pub global_x: i32,
    pub global_y: i32,
    pub width: u32,
    pub height: u32,
}

pub fn resolve_scroll_target(
    app: &AppHandle,
    monitor_index: usize,
    selection_x: f64,
    selection_y: f64,
    selection_width: f64,
    selection_height: f64,
) -> Result<ResolvedScrollTarget, String> {
    let tauri_monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to list Tauri monitors: {e}"))?;
    let tauri_monitor = tauri_monitors
        .get(monitor_index)
        .or_else(|| tauri_monitors.first())
        .ok_or_else(|| "No monitor found".to_string())?;
    let monitor_x = tauri_monitor.position().x;
    let monitor_y = tauri_monitor.position().y;
    let coordinate_scale = tauri_monitor.scale_factor();
    let monitors = xcap::Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {e}"))?;
    let monitor = monitors
        .iter()
        .find(|candidate| candidate.x().ok() == Some(monitor_x) && candidate.y().ok() == Some(monitor_y))
        .or_else(|| {
            let name = tauri_monitor.name().cloned().unwrap_or_default();
            monitors.iter().find(|candidate| candidate.name().unwrap_or_default() == name)
        })
        .or_else(|| monitors.get(monitor_index))
        .or_else(|| monitors.first())
        .ok_or_else(|| "No matching xcap monitor found".to_string())?;
    let monitor_x = monitor.x().map_err(|e| e.to_string())?;
    let monitor_y = monitor.y().map_err(|e| e.to_string())?;
    let global_x = monitor_x + (selection_x * coordinate_scale).round() as i32;
    let global_y = monitor_y + (selection_y * coordinate_scale).round() as i32;
    let width = (selection_width * coordinate_scale).round().max(1.0) as u32;
    let height = (selection_height * coordinate_scale).round().max(1.0) as u32;
    let window = xcap::Window::all()
        .map_err(|e| format!("Failed to list windows: {e}"))?
        .into_iter()
        .find(|candidate| is_capture_target(candidate, global_x, global_y))
        .ok_or_else(|| "未找到對應的應用程式視窗，請點擊有效視窗".to_string())?;
    Ok(ResolvedScrollTarget { window, coordinate_scale, global_x, global_y, width, height })
}

fn is_capture_target(window: &Window, x: i32, y: i32) -> bool {
    let title = window.title().unwrap_or_default();
    let app = window.app_name().unwrap_or_default();
    if app == "Dock" || app == "Window Server" || title.starts_with("Capture Window") || title.starts_with("tauri-app") {
        return false;
    }
    let Ok(window_x) = window.x() else { return false; };
    let Ok(window_y) = window.y() else { return false; };
    let Ok(width) = window.width() else { return false; };
    let Ok(height) = window.height() else { return false; };
    let Ok(minimized) = window.is_minimized() else { return false; };
    !minimized && width >= 200 && height >= 200
        && x >= window_x && x < window_x + width as i32
        && y >= window_y && y < window_y + height as i32
}
