use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

use crate::editor_image::load_editor_image;

fn create_image_editor_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::PinnedImageState>,
    data_url: String,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let label = format!("editor_{timestamp}");
    state.0.lock().unwrap().insert(label.clone(), data_url);
    let monitors = app.available_monitors().map_err(|e| format!("無法取得螢幕資訊：{e}"))?;
    let screen = monitors.first().ok_or_else(|| "找不到可用螢幕".to_string())?;
    let scale = screen.scale_factor();
    let max_w = (screen.size().width as f64 / scale * 0.86).max(640.0);
    let max_h = (screen.size().height as f64 / scale * 0.78).max(480.0);
    let fit = (max_w / width as f64).min(max_h / height as f64).min(1.0);
    let window_w = (width as f64 * fit).max(640.0);
    let window_h = (height as f64 * fit).max(480.0);
    let x = screen.position().x as f64 / scale + (screen.size().width as f64 / scale - window_w) / 2.0;
    let y = screen.position().y as f64 / scale + (screen.size().height as f64 / scale - window_h) / 2.0;
    let url = WebviewUrl::App(format!("index.html#/capture?label={label}&mode=edit").parse().unwrap());
    WebviewWindowBuilder::new(&app, &label, url)
        .title("UniSnap 編輯圖片").decorations(true).always_on_top(true).resizable(true).focused(true)
        .inner_size(window_w, window_h).position(x.max(0.0), y.max(0.0))
        .build().map_err(|e| format!("無法開啟圖片編輯視窗：{e}"))?;
    eprintln!("[capture] open_image_editor ready label={} size={}x{}", label, width, height);
    Ok(label)
}

#[tauri::command]
pub fn open_image_editor(app: tauri::AppHandle, state: tauri::State<'_, crate::PinnedImageState>, path: String) -> Result<String, String> {
    eprintln!("[capture] open_image_editor requested path={}", path);
    let image = load_editor_image(std::path::Path::new(&path))?;
    create_image_editor_window(app, state, image.data_url, image.width, image.height)
}

#[tauri::command]
pub fn open_image_editor_data(app: tauri::AppHandle, state: tauri::State<'_, crate::PinnedImageState>, data_url: String, width: u32, height: u32) -> Result<String, String> {
    if !data_url.starts_with("data:image/") || width == 0 || height == 0 {
        return Err("圖片資料格式無效".to_string());
    }
    eprintln!("[capture] open_image_editor_data size={}x{}", width, height);
    create_image_editor_window(app, state, data_url, width, height)
}

#[tauri::command]
pub fn open_image_in_main_editor(app: tauri::AppHandle) -> Result<(), String> {
    let parent = app.get_webview_window("main").ok_or_else(|| "找不到主視窗".to_string())?;
    let app_handle = app.clone();
    app.dialog().file().set_title("開啟圖片").add_filter("圖片", &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"]).set_parent(&parent).pick_file(move |selected| {
        let result = (|| -> Result<(), String> {
            let Some(file) = selected else { return Ok(()); };
            let path = file.into_path().map_err(|e| format!("無法取得圖片路徑：{e}"))?;
            eprintln!("[capture] main_editor selected path={}", path.display());
            let image = load_editor_image(&path)?;
            let label = format!("main_editor_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0));
            app_handle.state::<crate::PinnedImageState>().0.lock().unwrap().insert(label.clone(), image.data_url);
            let main = app_handle.get_webview_window("main").ok_or_else(|| "找不到主視窗".to_string())?;
            let monitor = main.current_monitor().map_err(|e| format!("無法取得目前螢幕：{e}"))?.or_else(|| app_handle.primary_monitor().ok().flatten()).ok_or_else(|| "找不到可用螢幕".to_string())?;
            let scale = monitor.scale_factor();
            let fit = ((monitor.size().width as f64 / scale * 0.86) / image.width as f64).min((monitor.size().height as f64 / scale * 0.78) / image.height as f64).min(1.0);
            main.set_size(tauri::Size::Logical(tauri::LogicalSize::new((image.width as f64 * fit).max(640.0), (image.height as f64 * fit).max(480.0)))).map_err(|e| format!("無法調整編輯視窗：{e}"))?;
            main.eval(format!("window.location.hash = '#/capture?label={label}&mode=edit-main';")).map_err(|e| format!("無法開啟圖片編輯器：{e}"))?;
            main.show().map_err(|e| format!("無法顯示編輯視窗：{e}"))?;
            main.set_focus().map_err(|e| format!("無法聚焦編輯視窗：{e}"))?;
            eprintln!("[capture] main_editor ready label={} size={}x{}", label, image.width, image.height);
            Ok(())
        })();
        if let Err(error) = result { eprintln!("[capture] main_editor failed: {error}"); let _ = app_handle.emit("main-editor-error", error); }
    });
    Ok(())
}
