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

#[tauri::command]
pub fn identify_monitors(
    app: tauri::AppHandle,
    monitor_index: Option<usize>,
) -> Result<(), String> {
    use tauri::Manager;

    let monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to list monitors: {}", e))?;
    if monitors.is_empty() {
        return Err("No monitors found".to_string());
    }

    // Destroy any existing identify windows first
    for (label, win) in app.webview_windows() {
        if label.starts_with("identify_monitor_") {
            let _ = win.destroy();
        }
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let indices: Vec<usize> = if let Some(idx) = monitor_index {
        if idx < monitors.len() {
            vec![idx]
        } else {
            vec![0]
        }
    } else {
        (0..monitors.len()).collect()
    };

    for index in indices {
        let monitor = &monitors[index];
        let scale = monitor.scale_factor().max(1.0);
        let screen_w = monitor.size().width as f64 / scale;
        let screen_h = monitor.size().height as f64 / scale;
        let win_w = 260.0;
        let win_h = 170.0;
        let x = monitor.position().x as f64 / scale + (screen_w - win_w) / 2.0;
        let y = monitor.position().y as f64 / scale + (screen_h - win_h) / 2.0;

        let label = format!("identify_monitor_{}_{}", index, timestamp);
        let url_str = format!(
            "index.html#/identify-monitor?index={}&number={}&w={}&h={}",
            index,
            index + 1,
            monitor.size().width,
            monitor.size().height
        );
        let window_url = tauri::WebviewUrl::App(
            url_str.parse().map_err(|e| format!("URL error: {}", e))?,
        );

        tauri::WebviewWindowBuilder::new(&app, &label, window_url)
            .title(format!("Identify Monitor {}", index + 1))
            .decorations(false)
            .always_on_top(true)
            .transparent(true)
            .resizable(false)
            .focused(false)
            .accept_first_mouse(false)
            .inner_size(win_w, win_h)
            .position(x, y)
            .build()
            .map_err(|e| format!("Failed to build identify window: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub fn close_identify_monitors(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    for (label, win) in app.webview_windows() {
        if label.starts_with("identify_monitor_") {
            let _ = win.destroy();
        }
    }
    Ok(())
}

