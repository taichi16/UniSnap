use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Base64 images shared with dynamically created always-on-top windows.
pub struct PinnedImageState(pub Mutex<HashMap<String, String>>);

#[tauri::command]
pub fn pin_screenshot(
    app: AppHandle,
    state: tauri::State<'_, PinnedImageState>,
    image_base64: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let label = format!("pin_{timestamp}");
    state
        .0
        .lock()
        .map_err(|_| "無法鎖定置頂圖片狀態".to_string())?
        .insert(label.clone(), image_base64);

    let window_url = WebviewUrl::App(
        format!("index.html#/pin?label={label}")
            .parse()
            .map_err(|error| format!("置頂圖片網址無效：{error}"))?,
    );
    WebviewWindowBuilder::new(&app, &label, window_url)
        .title("Pinned Screenshot")
        .decorations(false)
        .always_on_top(true)
        .transparent(true)
        .resizable(true)
        .inner_size(width as f64, height as f64)
        .position(x as f64, y as f64)
        .build()
        .map_err(|error| format!("建立置頂圖片視窗失敗：{error}"))?;
    Ok(label)
}

#[tauri::command]
pub fn get_pinned_image(
    state: tauri::State<'_, PinnedImageState>,
    label: String,
) -> Result<String, String> {
    state
        .0
        .lock()
        .map_err(|_| "無法鎖定置頂圖片狀態".to_string())?
        .get(&label)
        .cloned()
        .ok_or_else(|| "找不到置頂圖片".to_string())
}

#[tauri::command]
pub fn unpin_screenshot(
    app: AppHandle,
    state: tauri::State<'_, PinnedImageState>,
    label: String,
) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|_| "無法鎖定置頂圖片狀態".to_string())?
        .remove(&label);
    if let Some(window) = app.get_webview_window(&label) {
        window
            .close()
            .map_err(|error| format!("關閉置頂圖片視窗失敗：{error}"))?;
    }
    Ok(())
}
