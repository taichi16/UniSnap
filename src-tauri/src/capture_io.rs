use xcap::Monitor;

use crate::image_data::{decode_data_url, encode_requested_image, rgba_to_jpeg_data_url};
use crate::monitor_resolution::resolve_xcap_monitor;

#[tauri::command]
pub fn capture_screen_region(app: tauri::AppHandle, monitor_index: Option<usize>, x: f64, y: f64, width: f64, height: f64) -> Result<String, String> {
    let tauri_monitors = app.available_monitors().map_err(|e| format!("Failed to get Tauri monitors: {}", e))?;
    if tauri_monitors.is_empty() { return Err("No monitors found".to_string()); }
    let idx = monitor_index.unwrap_or(0).min(tauri_monitors.len() - 1);
    let tauri_mon = &tauri_monitors[idx];
    let scale_factor = tauri_mon.scale_factor();
    let xcap_monitors = Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {}", e))?;
    let target_xcap = resolve_xcap_monitor(tauri_mon, &xcap_monitors, idx).ok_or_else(|| format!("No matching monitor at index {}", idx))?;
    let mut rgba_img = target_xcap.capture_image().map_err(|e| format!("Capture error: {}", e))?;
    let crop_x = (x * scale_factor).max(0.0) as u32;
    let crop_y = (y * scale_factor).max(0.0) as u32;
    let crop_w = (width * scale_factor).max(1.0) as u32;
    let crop_h = (height * scale_factor).max(1.0) as u32;
    let bounded_w = crop_w.min(rgba_img.width().saturating_sub(crop_x));
    let bounded_h = crop_h.min(rgba_img.height().saturating_sub(crop_y));
    if bounded_w == 0 || bounded_h == 0 { return Err("Crop region has zero dimensions".to_string()); }
    let cropped_img = image::imageops::crop(&mut rgba_img, crop_x, crop_y, bounded_w, bounded_h).to_image();
    rgba_to_jpeg_data_url(&cropped_img)
}

#[tauri::command]
pub fn save_and_copy_screenshot(app: tauri::AppHandle, base64_image: String, save_path: Option<String>, auto_copy: bool) -> Result<String, String> {
    use arboard::{Clipboard, ImageData};
    let source_bytes = decode_data_url(&base64_image)?;
    let saved_path_str = if let Some(path) = save_path {
        let bytes = encode_requested_image(&source_bytes, &path, &app)?;
        std::fs::write(&path, &bytes).map_err(|e| format!("Failed to write file: {}", e))?;
        path
    } else {
        let default_dir = if let Ok(config) = crate::config::load_config(app.clone()) {
            std::path::PathBuf::from(config.save_directory)
        } else {
            std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default().join("Pictures").join("Screenshots")
        };
        if !default_dir.exists() { let _ = std::fs::create_dir_all(&default_dir); }
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = default_dir.join(format!("Screenshot_{}.png", timestamp));
        std::fs::write(&path, &source_bytes).map_err(|e| format!("Failed to write file: {}", e))?;
        path.to_string_lossy().to_string()
    };
    if auto_copy {
        let rgba = image::load_from_memory(&source_bytes).map_err(|e| format!("Failed to parse image for clipboard: {}", e))?.to_rgba8();
        let (w, h) = rgba.dimensions();
        Clipboard::new().map_err(|e| format!("Clipboard error: {}", e))?.set_image(ImageData { width: w as usize, height: h as usize, bytes: std::borrow::Cow::Borrowed(&rgba) }).map_err(|e| format!("Failed to copy image to clipboard: {}", e))?;
    }
    Ok(saved_path_str)
}

#[derive(serde::Serialize)]
pub struct SavedFileInfo { pub path: String, pub size: u64 }

#[tauri::command]
pub fn inspect_saved_file(path: String) -> Result<SavedFileInfo, String> {
    let metadata = std::fs::metadata(&path).map_err(|e| format!("Saved file cannot be read: {}", e))?;
    if !metadata.is_file() { return Err("Saved path is not a file".to_string()); }
    if metadata.len() == 0 { return Err("Saved file is empty".to_string()); }
    Ok(SavedFileInfo { path, size: metadata.len() })
}

#[tauri::command]
pub fn copy_screenshot_to_clipboard(base64_image: String) -> Result<(), String> {
    use arboard::{Clipboard, ImageData};
    eprintln!("[clipboard] copy request base64_chars={}", base64_image.len());
    let bytes = decode_data_url(&base64_image)?;
    let rgba = image::load_from_memory(&bytes).map_err(|e| format!("Failed to parse image for clipboard: {}", e))?.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {}", e))?;
    clipboard.set_image(ImageData { width: width as usize, height: height as usize, bytes: std::borrow::Cow::Borrowed(&rgba) }).map_err(|e| format!("Failed to copy image to clipboard: {}", e))?;
    Ok(())
}
