use xcap::Monitor;

use crate::capture_geometry::work_area_crop_bounds;
use crate::capture_output::save_capture_png;
use crate::monitor_resolution::resolve_xcap_monitor;

#[tauri::command]
pub fn capture_full_screen(app: tauri::AppHandle, monitor_index: Option<usize>) -> Result<String, String> {
    std::thread::sleep(std::time::Duration::from_millis(400));
    let monitors = app.available_monitors().map_err(|e| format!("Failed to get Tauri monitors: {}", e))?;
    if monitors.is_empty() { return Err("No monitors found".to_string()); }
    let idx = monitor_index.unwrap_or(0).min(monitors.len() - 1);
    let tauri_monitor = &monitors[idx];
    let xcap_monitors = Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {}", e))?;
    let target = resolve_xcap_monitor(tauri_monitor, &xcap_monitors, idx).ok_or_else(|| format!("No matching monitor at index {}", idx))?;
    let image = target.capture_image().map_err(|e| format!("Capture error: {}", e))?;
    save_capture_png(app, &image, "Screenshot")
}

#[tauri::command]
pub fn capture_work_area(app: tauri::AppHandle, monitor_index: Option<usize>) -> Result<String, String> {
    std::thread::sleep(std::time::Duration::from_millis(400));
    let monitors = app.available_monitors().map_err(|e| format!("Failed to get Tauri monitors: {}", e))?;
    if monitors.is_empty() { return Err("No monitors found".to_string()); }
    let idx = monitor_index.unwrap_or(0).min(monitors.len() - 1);
    let tauri_monitor = &monitors[idx];
    let phys_x = tauri_monitor.position().x;
    let phys_y = tauri_monitor.position().y;
    let work_area = tauri_monitor.work_area();
    let xcap_monitors = Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {}", e))?;
    let target = resolve_xcap_monitor(tauri_monitor, &xcap_monitors, idx).ok_or_else(|| format!("No matching monitor at index {}", idx))?;
    let mut image = target.capture_image().map_err(|e| format!("Capture error: {}", e))?;
    let bounds = work_area_crop_bounds(phys_x, phys_y, work_area.position.x, work_area.position.y, work_area.size.width, work_area.size.height, image.width(), image.height()).ok_or_else(|| "Work area is outside the selected monitor".to_string())?;
    let cropped = image::imageops::crop(&mut image, bounds.x, bounds.y, bounds.width, bounds.height).to_image();
    save_capture_png(app, &cropped, "Screenshot_WorkArea")
}

