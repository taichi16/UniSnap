use tauri::{WebviewUrl, WebviewWindowBuilder};
use xcap::Monitor;

use crate::image_data::rgba_to_jpeg_data_url;
use crate::monitor_resolution::resolve_xcap_monitor;

#[tauri::command]
pub fn trigger_screenshot(app: tauri::AppHandle, state: tauri::State<'_, crate::PinnedImageState>, mode: Option<String>, monitor_index: Option<usize>) -> Result<(), String> {
    std::thread::sleep(std::time::Duration::from_millis(40));
    let mode_str = mode.unwrap_or_else(|| "screenshot".to_string());
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let tauri_monitors = app.available_monitors().map_err(|e| format!("Failed to get Tauri monitors: {}", e))?;
    if tauri_monitors.is_empty() { return Err("No monitors found".to_string()); }
    let xcap_monitors = Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {}", e))?;
    let indices: Vec<usize> = if let Some(idx) = monitor_index { vec![idx.min(tauri_monitors.len().saturating_sub(1))] } else { (0..tauri_monitors.len()).collect() };
    for index in indices {
        let tauri_monitor = &tauri_monitors[index];
        let scale_factor = tauri_monitor.scale_factor();
        let phys_x = tauri_monitor.position().x;
        let phys_y = tauri_monitor.position().y;
        let phys_w = tauri_monitor.size().width;
        let phys_h = tauri_monitor.size().height;
        let target = resolve_xcap_monitor(tauri_monitor, &xcap_monitors, index).ok_or_else(|| format!("Could not find matching monitor for index {}", index))?;
        let image = target.capture_image().map_err(|e| format!("Failed to capture monitor: {}", e))?;
        let data_url = rgba_to_jpeg_data_url(&image)?;
        let label = format!("capture_{}_{}", index, timestamp);
        state.0.lock().unwrap().insert(label.clone(), data_url);
        let window_url = WebviewUrl::App(format!("index.html#/capture?label={label}&mode={mode_str}").parse().unwrap());
        WebviewWindowBuilder::new(&app, &label, window_url)
            .title(format!("Capture Window {}", index)).decorations(false).always_on_top(true).transparent(true).resizable(false).focused(true).accept_first_mouse(true)
            .inner_size(phys_w as f64 / scale_factor, phys_h as f64 / scale_factor)
            .position(phys_x as f64 / scale_factor, phys_y as f64 / scale_factor)
            .build().map_err(|e| format!("Failed to build capture window: {}", e))?;
    }
    Ok(())
}

