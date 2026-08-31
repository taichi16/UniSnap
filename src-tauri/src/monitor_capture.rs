use image::RgbaImage;
use xcap::Monitor;

use crate::capture_types::{MonitorBasicInfo, MonitorScreenshot};
use crate::image_data::rgba_to_jpeg_data_url;
use crate::monitor_resolution::resolve_xcap_monitor;

#[tauri::command]
pub fn list_monitors(app: tauri::AppHandle) -> Result<Vec<MonitorBasicInfo>, String> {
    let monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to list monitors: {}", e))?;
    let xcap_monitors = Monitor::all().unwrap_or_default();
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let x = monitor.position().x;
            let y = monitor.position().y;
            let fallback = monitor.name().cloned().unwrap_or_else(|| "未命名顯示器".to_string());
            let name = resolve_xcap_monitor(&monitor, &xcap_monitors, index)
                .and_then(|xcap| xcap.name().ok())
                .unwrap_or(fallback);
            MonitorBasicInfo {
                name,
                x,
                y,
                width: monitor.size().width,
                height: monitor.size().height,
                scale_factor: monitor.scale_factor() as f32,
            }
        })
        .collect())
}

#[tauri::command]
pub fn capture_screens() -> Result<Vec<MonitorScreenshot>, String> {
    let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;
    monitors
        .into_iter()
        .map(|monitor| {
            let name = monitor.name().map_err(|e| format!("Failed to get name: {}", e))?;
            let x = monitor.x().map_err(|e| format!("Failed to get x: {}", e))?;
            let y = monitor.y().map_err(|e| format!("Failed to get y: {}", e))?;
            let width = monitor.width().map_err(|e| format!("Failed to get width: {}", e))?;
            let height = monitor.height().map_err(|e| format!("Failed to get height: {}", e))?;
            let scale_factor = monitor.scale_factor().map_err(|e| format!("Failed to get scale_factor: {}", e))?;
            let image: RgbaImage = monitor.capture_image().map_err(|e| format!("Failed to capture monitor {}: {}", name, e))?;
            Ok(MonitorScreenshot {
                name,
                x,
                y,
                width,
                height,
                scale_factor,
                base64_image: rgba_to_jpeg_data_url(&image)?,
            })
        })
        .collect()
}
