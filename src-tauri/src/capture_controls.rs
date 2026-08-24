use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[tauri::command]
pub fn close_capture_windows(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::PinnedImageState>,
) -> Result<(), String> {
    let mut map = state.0.lock().unwrap();
    for (label, window) in app.webview_windows() {
        if label.starts_with("capture_")
            || label.starts_with("editor_")
            || label.starts_with("recording_control_")
        {
            map.remove(&label);
            let _ = window.close();
        }
    }
    for (label, window) in &app.webview_windows() {
        if !label.starts_with("capture_")
            && !label.starts_with("recording_control_")
            && !label.starts_with("pin_")
        {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn open_recording_control(
    app: tauri::AppHandle,
    monitor_index: usize,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    fps: u32,
    record_audio: bool,
) -> Result<(), String> {
    eprintln!("[capture] open_recording_control monitor={}", monitor_index);
    let monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to list monitors for recording control: {}", e))?;
    let monitor = monitors
        .get(monitor_index)
        .or_else(|| monitors.first())
        .ok_or_else(|| "No monitor found for recording control".to_string())?;
    let scale = monitor.scale_factor();
    let position_x = monitor.position().x as f64 / scale + 24.0;
    let position_y = monitor.position().y as f64 / scale + 24.0;
    let capture_label = app
        .webview_windows()
        .into_iter()
        .find(|(label, _)| label.starts_with(&format!("capture_{monitor_index}_")))
        .map(|(label, _)| label)
        .ok_or_else(|| format!("找不到第 {} 個螢幕的框選視窗", monitor_index + 1))?;
    let control = app
        .get_webview_window(&capture_label)
        .ok_or_else(|| format!("找不到目前的框選視窗：{capture_label}"))?;
    for (label, window) in app.webview_windows() {
        if (label.starts_with("capture_") && label != capture_label)
            || label.starts_with("recording_control_")
        {
            let _ = window.close();
        }
    }
    control.set_size(tauri::LogicalSize::new(300.0, 58.0)).map_err(|e| format!("無法縮放錄影控制列：{e}"))?;
    control.set_position(tauri::LogicalPosition::new(position_x, position_y)).map_err(|e| format!("無法定位錄影控制列：{e}"))?;
    control.set_resizable(false).map_err(|e| format!("無法鎖定錄影控制列大小：{e}"))?;
    control.set_always_on_top(true).map_err(|e| format!("無法將錄影控制列置頂：{e}"))?;
    control.show().map_err(|e| format!("無法顯示錄影控制列：{e}"))?;
    control.set_focus().map_err(|e| format!("無法聚焦錄影控制列：{e}"))?;
    eprintln!("[capture] open_recording_control ready capture_label={}", capture_label);
    let _ = (x, y, width, height, fps, record_audio);
    Ok(())
}

#[tauri::command]
pub fn open_recording_start_control(
    app: tauri::AppHandle,
    monitor_index: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    fps: u32,
    record_audio: bool,
) -> Result<(), String> {
    eprintln!("[capture] open_recording_start_control monitor={} rect=({}, {}, {}, {}) fps={} audio={}", monitor_index, x, y, width, height, fps, record_audio);
    let monitor = app.available_monitors().map_err(|e| format!("無法列出錄影控制列螢幕：{e}"))?.get(monitor_index).cloned().ok_or_else(|| format!("找不到第 {} 個螢幕", monitor_index + 1))?;
    for (label, window) in app.webview_windows() {
        if label.starts_with("recording_start_control_") { let _ = window.close(); }
    }
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
    let label = format!("recording_start_control_{timestamp}");
    let scale = monitor.scale_factor();
    let url = format!("index.html#/recording-start-control?monitorIndex={monitor_index}&x={x}&y={y}&width={width}&height={height}&fps={fps}&recordAudio={record_audio}");
    let control = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.parse().unwrap()))
        .title("開始錄影").decorations(false).always_on_top(true).focused(true).resizable(false)
        .inner_size(330.0, 58.0)
        .position(monitor.position().x as f64 / scale + 24.0, monitor.position().y as f64 / scale + 58.0)
        .build().map_err(|e| format!("建立開始錄影控制列失敗：{e}"))?;
    control.set_always_on_top(true).map_err(|e| format!("設定開始錄影控制列置頂失敗：{e}"))?;
    control.show().map_err(|e| format!("顯示開始錄影控制列失敗：{e}"))?;
    control.set_focus().map_err(|e| format!("聚焦開始錄影控制列失敗：{e}"))?;
    eprintln!("[capture] recording_start_control ready label={label}");
    Ok(())
}
