use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn prepare_file_drag(path: *const std::ffi::c_char, error_buffer: *mut std::ffi::c_char, error_buffer_size: usize) -> bool;
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessItem {
    pub image_data: String,
    pub path: String,
}

pub struct QuickAccessState(pub Mutex<HashMap<String, QuickAccessItem>>);

#[tauri::command]
pub fn prepare_quick_access_drag(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let c_path = std::ffi::CString::new(path).map_err(|_| "拖曳檔案路徑包含無效字元".to_string())?;
        let mut error = vec![0_i8; 512];
        let ok = unsafe { prepare_file_drag(c_path.as_ptr(), error.as_mut_ptr(), error.len()) };
        if !ok {
            let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }.to_string_lossy().into_owned();
            return Err(if message.is_empty() { "無法準備原生檔案拖曳".into() } else { message });
        }
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err("原生檔案拖曳目前僅支援 macOS".into())
    }
}

#[tauri::command]
pub fn open_quick_access(
    app: AppHandle,
    state: tauri::State<'_, QuickAccessState>,
    image_data: String,
    path: String,
) -> Result<String, String> {
    eprintln!(
        "[quick-access] request path={} image_chars={}",
        path,
        image_data.len()
    );
    if !image_data.starts_with("data:image/") {
        eprintln!("[quick-access] rejected invalid image data URL");
        return Err("快速取用圖片資料格式無效".into());
    }
    if !Path::new(&path).is_file() {
        eprintln!("[quick-access] rejected missing file path={}", path);
        return Err("快速取用檔案不存在".into());
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let label = format!("quick_access_{timestamp}");
    state
        .0
        .lock()
        .map_err(|_| "無法鎖定快速取用狀態".to_string())?
        .insert(label.clone(), QuickAccessItem { image_data, path });

    let monitor = app
        .primary_monitor()
        .map_err(|e| format!("無法取得主要螢幕：{e}"))?
        .ok_or_else(|| "找不到可用螢幕".to_string())?;
    let scale = monitor.scale_factor().max(1.0);
    let screen_w = monitor.size().width as f64 / scale;
    let screen_h = monitor.size().height as f64 / scale;
    let window_w = 360.0;
    let window_h = 220.0;
    let x = monitor.position().x as f64 / scale + screen_w - window_w - 20.0;
    let y = monitor.position().y as f64 / scale + screen_h - window_h - 60.0;
    let url = WebviewUrl::App(
        format!("index.html#/quick-access?label={label}")
            .parse()
            .map_err(|e| format!("快速取用網址無效：{e}"))?,
    );
    WebviewWindowBuilder::new(&app, &label, url)
        .title("UniSnap 快速取用")
        .decorations(true)
        .always_on_top(true)
        .resizable(false)
        .focused(true)
        .inner_size(window_w, window_h)
        .position(x.max(0.0), y.max(0.0))
        .build()
        .map_err(|e| {
            eprintln!("[quick-access] window build failed: {e}");
            format!("建立快速取用視窗失敗：{e}")
        })?;
    eprintln!(
        "[quick-access] opened label={} path={}",
        label,
        state
            .0
            .lock()
            .ok()
            .and_then(|items| items.get(&label).map(|item| item.path.clone()))
            .unwrap_or_default()
    );
    Ok(label)
}

#[tauri::command]
pub fn get_quick_access(
    state: tauri::State<'_, QuickAccessState>,
    label: String,
) -> Result<QuickAccessItem, String> {
    state
        .0
        .lock()
        .map_err(|_| "無法鎖定快速取用狀態".to_string())?
        .get(&label)
        .cloned()
        .ok_or_else(|| "找不到快速取用圖片".to_string())
}

#[tauri::command]
pub fn close_quick_access(
    app: AppHandle,
    state: tauri::State<'_, QuickAccessState>,
    label: String,
) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|_| "無法鎖定快速取用狀態".to_string())?
        .remove(&label);
    if let Some(window) = app.get_webview_window(&label) {
        window
            .close()
            .map_err(|e| format!("關閉快速取用視窗失敗：{e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_serializes_with_drag_fields() {
        let item = QuickAccessItem {
            image_data: "data:image/png;base64,AA==".into(),
            path: "/tmp/example.png".into(),
        };
        let value = serde_json::to_value(item).unwrap();
        assert_eq!(value["imageData"], "data:image/png;base64,AA==");
        assert_eq!(value["path"], "/tmp/example.png");
    }
}
