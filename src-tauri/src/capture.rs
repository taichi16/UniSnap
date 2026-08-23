use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::io::Cursor;
#[cfg(not(target_os = "windows"))]
use enigo::{Axis, Coordinate, Enigo, Mouse, Settings};
use image::RgbaImage;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use xcap::Monitor;
use crate::capture_types::{MonitorBasicInfo, MonitorScreenshot};
use crate::image_data::{decode_data_url, encode_requested_image, rgba_to_jpeg_data_url};

/// Lightweight monitor listing using Tauri's own API.
/// Does NOT require screen recording permission on macOS.
#[tauri::command]
pub fn list_monitors(app: tauri::AppHandle) -> Result<Vec<MonitorBasicInfo>, String> {
    let monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to list monitors: {}", e))?;
    let xcap_monitors = Monitor::all().unwrap_or_default();
    let infos = monitors
        .into_iter()
        .map(|m| {
            let x = m.position().x;
            let y = m.position().y;
            let fallback = m.name().cloned().unwrap_or_else(|| "未命名顯示器".to_string());
            let name = xcap_monitors.iter().find_map(|xm| {
                let same_position = xm.x().ok() == Some(x) && xm.y().ok() == Some(y);
                same_position.then(|| xm.name().ok()).flatten()
            }).unwrap_or(fallback);
            MonitorBasicInfo {
                name,
                x,
                y,
                width: m.size().width,
                height: m.size().height,
                scale_factor: m.scale_factor() as f32,
            }
        })
        .collect();
    Ok(infos)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CropBounds {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// Converts a desktop-global work-area rectangle into image-local pixels for
/// one monitor. Tauri positions and sizes are already physical pixels, as is
/// the image returned by xcap, so no DPI factor belongs in this conversion.
fn work_area_crop_bounds(
    monitor_x: i32,
    monitor_y: i32,
    work_x: i32,
    work_y: i32,
    work_width: u32,
    work_height: u32,
    image_width: u32,
    image_height: u32,
) -> Option<CropBounds> {
    let x = work_x.saturating_sub(monitor_x).max(0) as u32;
    let y = work_y.saturating_sub(monitor_y).max(0) as u32;
    if x >= image_width || y >= image_height {
        return None;
    }
    let width = work_width.min(image_width.saturating_sub(x));
    let height = work_height.min(image_height.saturating_sub(y));
    (width > 0 && height > 0).then_some(CropBounds {
        x,
        y,
        width,
        height,
    })
}

#[tauri::command]
pub fn capture_screens() -> Result<Vec<MonitorScreenshot>, String> {
    let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;

    let mut infos = Vec::new();
    for monitor in monitors {
        let name = monitor
            .name()
            .map_err(|e| format!("Failed to get name: {}", e))?;
        let x = monitor.x().map_err(|e| format!("Failed to get x: {}", e))?;
        let y = monitor.y().map_err(|e| format!("Failed to get y: {}", e))?;
        let width = monitor
            .width()
            .map_err(|e| format!("Failed to get width: {}", e))?;
        let height = monitor
            .height()
            .map_err(|e| format!("Failed to get height: {}", e))?;
        let scale_factor = monitor
            .scale_factor()
            .map_err(|e| format!("Failed to get scale_factor: {}", e))?;

        let image: RgbaImage = monitor
            .capture_image()
            .map_err(|e| format!("Failed to capture monitor {}: {}", name, e))?;

        let data_url = rgba_to_jpeg_data_url(&image)?;

        infos.push(MonitorScreenshot {
            name,
            x,
            y,
            width,
            height,
            scale_factor,
            base64_image: data_url,
        });
    }

    Ok(infos)
}

#[tauri::command]
pub fn trigger_screenshot(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::PinnedImageState>,
    mode: Option<String>,
    monitor_index: Option<usize>,
) -> Result<(), String> {
    // Ultra-low latency: 40ms is plenty for the window hide animation on macOS
    std::thread::sleep(std::time::Duration::from_millis(40));

    let mode_str = mode.unwrap_or_else(|| "screenshot".to_string());
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let tauri_monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to get Tauri monitors: {}", e))?;

    if tauri_monitors.is_empty() {
        return Err("No monitors found".to_string());
    }

    let xcap_monitors =
        Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {}", e))?;

    // Decide which indices to process
    let indices: Vec<usize> = if let Some(idx) = monitor_index {
        vec![idx.min(tauri_monitors.len().saturating_sub(1))]
    } else {
        (0..tauri_monitors.len()).collect()
    };

    for index in indices {
        let tauri_mon = &tauri_monitors[index];
        let scale_factor = tauri_mon.scale_factor();

        let phys_x = tauri_mon.position().x;
        let phys_y = tauri_mon.position().y;
        let phys_w = tauri_mon.size().width;
        let phys_h = tauri_mon.size().height;

        // Match xcap monitor by exact physical (x, y) position
        let matched_xcap = xcap_monitors
            .iter()
            .find(|xm| {
                let xm_x = xm.x().unwrap_or(i32::MIN);
                let xm_y = xm.y().unwrap_or(i32::MIN);
                xm_x == phys_x && xm_y == phys_y
            })
            .or_else(|| {
                // Fallback: match by name
                let tname = tauri_mon.name().cloned().unwrap_or_default();
                xcap_monitors
                    .iter()
                    .find(|xm| xm.name().unwrap_or_default() == tname)
            })
            .or_else(|| xcap_monitors.get(index));

        let target_xcap = matched_xcap
            .ok_or_else(|| format!("Could not find matching monitor for index {}", index))?;

        let image = target_xcap
            .capture_image()
            .map_err(|e| format!("Failed to capture monitor: {}", e))?;

        // Ultra-fast JPEG encode (15ms vs 1200ms PNG)
        let data_url = rgba_to_jpeg_data_url(&image)?;

        let label = format!("capture_{}_{}", index, timestamp);
        {
            let mut map = state.0.lock().unwrap();
            map.insert(label.clone(), data_url);
        }

        let window_url = tauri::WebviewUrl::App(
            format!("index.html#/capture?label={}&mode={}", label, mode_str)
                .parse()
                .unwrap(),
        );

        let logical_x = phys_x as f64 / scale_factor;
        let logical_y = phys_y as f64 / scale_factor;
        let logical_w = phys_w as f64 / scale_factor;
        let logical_h = phys_h as f64 / scale_factor;

        tauri::WebviewWindowBuilder::new(&app, &label, window_url)
            .title(format!("Capture Window {}", index))
            .decorations(false)
            .always_on_top(true)
            .transparent(true)
            .resizable(false)
            .focused(true)
            .accept_first_mouse(true)
            .inner_size(logical_w, logical_h)
            .position(logical_x, logical_y)
            .build()
            .map_err(|e| format!("Failed to build capture window: {}", e))?;
    }

    Ok(())
}

fn create_image_editor_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::PinnedImageState>,
    data_url: String,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let label = format!("editor_{}", timestamp);
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
    let url = tauri::WebviewUrl::App(format!("index.html#/capture?label={label}&mode=edit").parse().unwrap());
    tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title("UniSnap 編輯圖片")
        .decorations(true)
        .always_on_top(true)
        .resizable(true)
        .focused(true)
        .inner_size(window_w, window_h)
        .position(x.max(0.0), y.max(0.0))
        .build()
        .map_err(|e| format!("無法開啟圖片編輯視窗：{e}"))?;
    eprintln!("[capture] open_image_editor ready label={} size={}x{}", label, width, height);
    Ok(label)
}

/// Opens an existing local image in the same editor used for new screenshots.
/// Kept for callers that already have a local path.
#[tauri::command]
pub fn open_image_editor(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::PinnedImageState>,
    path: String,
) -> Result<String, String> {
    eprintln!("[capture] open_image_editor requested path={}", path);
    let file_path = std::path::Path::new(&path);
    if !file_path.is_file() {
        return Err(format!("找不到圖片檔案：{}", file_path.display()));
    }
    let source = std::fs::read(file_path).map_err(|e| format!("無法讀取圖片 {}：{e}", file_path.display()))?;
    let image = image::load_from_memory(&source).map_err(|e| format!("無法開啟圖片：{e}"))?;
    let width = image.width();
    let height = image.height();
    let mut encoded = Cursor::new(Vec::new());
    image.write_to(&mut encoded, image::ImageFormat::Png).map_err(|e| format!("無法準備圖片編輯資料：{e}"))?;
    create_image_editor_window(app, state, format!("data:image/png;base64,{}", STANDARD.encode(encoded.into_inner())), width, height)
}

/// Opens image data supplied by the native file input, avoiding path and
/// permission differences between macOS file providers.
#[tauri::command]
pub fn open_image_editor_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::PinnedImageState>,
    data_url: String,
    width: u32,
    height: u32,
) -> Result<String, String> {
    if !data_url.starts_with("data:image/") || width == 0 || height == 0 {
        return Err("圖片資料格式無效".to_string());
    }
    eprintln!("[capture] open_image_editor_data size={}x{}", width, height);
    create_image_editor_window(app, state, data_url, width, height)
}

/// Native macOS file chooser followed by editing in the existing main window.
/// This deliberately avoids browser file inputs and a second editor window.
#[tauri::command]
pub fn open_image_in_main_editor(
    app: tauri::AppHandle,
) -> Result<(), String> {
    let parent = app.get_webview_window("main").ok_or_else(|| "找不到主視窗".to_string())?;
    let app_handle = app.clone();
    app.dialog().file()
        .set_title("開啟圖片")
        .add_filter("圖片", &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"])
        .set_parent(&parent)
        .pick_file(move |selected| {
            let result = (|| -> Result<(), String> {
                let Some(file) = selected else { return Ok(()); };
                let path = file.into_path().map_err(|e| format!("無法取得圖片路徑：{e}"))?;
                eprintln!("[capture] main_editor selected path={}", path.display());
                let source = std::fs::read(&path).map_err(|e| format!("無法讀取圖片：{e}"))?;
                let image = image::load_from_memory(&source).map_err(|e| format!("無法開啟圖片：{e}"))?;
                let width = image.width();
                let height = image.height();
                let mut encoded = Cursor::new(Vec::new());
                image.write_to(&mut encoded, image::ImageFormat::Png).map_err(|e| format!("無法準備圖片編輯資料：{e}"))?;
                let label = format!("main_editor_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0));
                app_handle.state::<crate::PinnedImageState>().0.lock().unwrap().insert(label.clone(), format!("data:image/png;base64,{}", STANDARD.encode(encoded.into_inner())));
                let main = app_handle.get_webview_window("main").ok_or_else(|| "找不到主視窗".to_string())?;
                let monitor = main.current_monitor().map_err(|e| format!("無法取得目前螢幕：{e}"))?.or_else(|| app_handle.primary_monitor().ok().flatten()).ok_or_else(|| "找不到可用螢幕".to_string())?;
                let scale = monitor.scale_factor();
                let fit = ((monitor.size().width as f64 / scale * 0.86) / width as f64).min((monitor.size().height as f64 / scale * 0.78) / height as f64).min(1.0);
                main.set_size(tauri::Size::Logical(tauri::LogicalSize::new((width as f64 * fit).max(640.0), (height as f64 * fit).max(480.0)))).map_err(|e| format!("無法調整編輯視窗：{e}"))?;
                main.eval(&format!("window.location.hash = '#/capture?label={label}&mode=edit-main';")).map_err(|e| format!("無法開啟圖片編輯器：{e}"))?;
                main.show().map_err(|e| format!("無法顯示編輯視窗：{e}"))?;
                main.set_focus().map_err(|e| format!("無法聚焦編輯視窗：{e}"))?;
                eprintln!("[capture] main_editor ready label={} size={}x{}", label, width, height);
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("[capture] main_editor failed: {error}");
                let _ = app_handle.emit("main-editor-error", error);
            }
        });
    Ok(())
}

#[tauri::command]
pub fn close_capture_windows(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::PinnedImageState>,
) -> Result<(), String> {
    let mut map = state.0.lock().unwrap();
    let windows = app.webview_windows();
    for (label, window) in windows {
        if label.starts_with("capture_") || label.starts_with("editor_") || label.starts_with("recording_control_") {
            map.remove(&label);
            let _ = window.close();
        }
    }
    // Show all non-capture and non-pin windows again
    let windows = app.webview_windows();
    for (label, window) in &windows {
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

    control
        .set_size(tauri::LogicalSize::new(300.0, 58.0))
        .map_err(|e| format!("無法縮放錄影控制列：{e}"))?;
    control
        .set_position(tauri::LogicalPosition::new(position_x, position_y))
        .map_err(|e| format!("無法定位錄影控制列：{e}"))?;
    control
        .set_resizable(false)
        .map_err(|e| format!("無法鎖定錄影控制列大小：{e}"))?;
    control
        .set_always_on_top(true)
        .map_err(|e| format!("無法將錄影控制列置頂：{e}"))?;
    control
        .show()
        .map_err(|e| format!("無法顯示錄影控制列：{e}"))?;
    control
        .set_focus()
        .map_err(|e| format!("無法聚焦錄影控制列：{e}"))?;
    eprintln!(
        "[capture] open_recording_control ready capture_label={}",
        capture_label
    );

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
    let monitor = app
        .available_monitors()
        .map_err(|e| format!("無法列出錄影控制列螢幕：{e}"))?
        .get(monitor_index)
        .cloned()
        .ok_or_else(|| format!("找不到第 {} 個螢幕", monitor_index + 1))?;
    for (label, window) in app.webview_windows() {
        if label.starts_with("recording_start_control_") {
            let _ = window.close();
        }
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let label = format!("recording_start_control_{timestamp}");
    let scale = monitor.scale_factor();
    let url = format!(
        "index.html#/recording-start-control?monitorIndex={monitor_index}&x={x}&y={y}&width={width}&height={height}&fps={fps}&recordAudio={record_audio}"
    );
    let control = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.parse().unwrap()))
        .title("開始錄影")
        .decorations(false)
        .always_on_top(true)
        .focused(true)
        .resizable(false)
        .inner_size(330.0, 58.0)
        .position(
            monitor.position().x as f64 / scale + 24.0,
            monitor.position().y as f64 / scale + 58.0,
        )
        .build()
        .map_err(|e| format!("建立開始錄影控制列失敗：{e}"))?;
    control
        .set_always_on_top(true)
        .map_err(|e| format!("設定開始錄影控制列置頂失敗：{e}"))?;
    control
        .show()
        .map_err(|e| format!("顯示開始錄影控制列失敗：{e}"))?;
    control
        .set_focus()
        .map_err(|e| format!("聚焦開始錄影控制列失敗：{e}"))?;
    eprintln!("[capture] recording_start_control ready label={label}");
    Ok(())
}

#[tauri::command]
pub fn capture_screen_region(
    app: tauri::AppHandle,
    monitor_index: Option<usize>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<String, String> {
    let tauri_monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to get Tauri monitors: {}", e))?;
    if tauri_monitors.is_empty() {
        return Err("No monitors found".to_string());
    }
    let idx = monitor_index.unwrap_or(0).min(tauri_monitors.len() - 1);
    let tauri_mon = &tauri_monitors[idx];
    let phys_x = tauri_mon.position().x;
    let phys_y = tauri_mon.position().y;
    let scale_factor = tauri_mon.scale_factor();

    let xcap_monitors =
        Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {}", e))?;
    let matched_xcap = xcap_monitors
        .iter()
        .find(|xm| {
            let xm_x = xm.x().unwrap_or(i32::MIN);
            let xm_y = xm.y().unwrap_or(i32::MIN);
            xm_x == phys_x && xm_y == phys_y
        })
        .or_else(|| {
            let tname = tauri_mon.name().cloned().unwrap_or_default();
            xcap_monitors
                .iter()
                .find(|xm| xm.name().unwrap_or_default() == tname)
        })
        .or_else(|| xcap_monitors.get(idx));

    let target_xcap =
        matched_xcap.ok_or_else(|| format!("No matching monitor at index {}", idx))?;
    let full_img = target_xcap
        .capture_image()
        .map_err(|e| format!("Capture error: {}", e))?;
    let mut rgba_img = full_img;

    let crop_x = (x * scale_factor).max(0.0) as u32;
    let crop_y = (y * scale_factor).max(0.0) as u32;
    let crop_w = (width * scale_factor).max(1.0) as u32;
    let crop_h = (height * scale_factor).max(1.0) as u32;

    let bounded_w = crop_w.min(rgba_img.width().saturating_sub(crop_x));
    let bounded_h = crop_h.min(rgba_img.height().saturating_sub(crop_y));

    if bounded_w == 0 || bounded_h == 0 {
        return Err("Crop region has zero dimensions".to_string());
    }

    let cropped_img =
        image::imageops::crop(&mut rgba_img, crop_x, crop_y, bounded_w, bounded_h).to_image();
    let data_url = rgba_to_jpeg_data_url(&cropped_img)?;
    Ok(data_url)
}

#[tauri::command]
pub fn save_and_copy_screenshot(
    app: tauri::AppHandle,
    base64_image: String,
    save_path: Option<String>,
    auto_copy: bool,
) -> Result<String, String> {
    use arboard::{Clipboard, ImageData};

    let source_bytes = decode_data_url(&base64_image)?;

    // Save to file if path is provided
    let saved_path_str = if let Some(path) = save_path {
        let bytes = encode_requested_image(&source_bytes, &path, &app)?;
        std::fs::write(&path, &bytes).map_err(|e| format!("Failed to write file: {}", e))?;
        path
    } else {
        // Resolve default folder from config
        let default_dir = if let Ok(loaded_config) = crate::config::load_config(app.clone()) {
            std::path::PathBuf::from(loaded_config.save_directory)
        } else {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_default()
                .join("Pictures")
                .join("Screenshots")
        };

        if !default_dir.exists() {
            let _ = std::fs::create_dir_all(&default_dir);
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = format!("Screenshot_{}.png", timestamp);
        let path = default_dir.join(filename);
        std::fs::write(&path, &source_bytes).map_err(|e| format!("Failed to write file: {}", e))?;
        path.to_string_lossy().to_string()
    };

    // Copy to clipboard
    if auto_copy {
        let img = image::load_from_memory(&source_bytes)
            .map_err(|e| format!("Failed to parse image for clipboard: {}", e))?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut ctx = Clipboard::new().map_err(|e| format!("Clipboard error: {}", e))?;
        ctx.set_image(ImageData {
            width: w as usize,
            height: h as usize,
            bytes: std::borrow::Cow::Borrowed(&rgba),
        })
        .map_err(|e| format!("Failed to copy image to clipboard: {}", e))?;
    }

    Ok(saved_path_str)
}

#[derive(serde::Serialize)]
pub struct SavedFileInfo {
    pub path: String,
    pub size: u64,
}

#[tauri::command]
pub fn inspect_saved_file(path: String) -> Result<SavedFileInfo, String> {
    let metadata =
        std::fs::metadata(&path).map_err(|e| format!("Saved file cannot be read: {}", e))?;
    if !metadata.is_file() {
        return Err("Saved path is not a file".to_string());
    }
    if metadata.len() == 0 {
        return Err("Saved file is empty".to_string());
    }
    Ok(SavedFileInfo {
        path,
        size: metadata.len(),
    })
}

/// Copy an image to the clipboard without creating a file.
///
/// Keeping this separate from `save_and_copy_screenshot` prevents copy-only
/// actions (including the automatic copy after a long capture) from silently
/// writing an additional screenshot to disk.
#[tauri::command]
pub fn copy_screenshot_to_clipboard(base64_image: String) -> Result<(), String> {
    use arboard::{Clipboard, ImageData};

    eprintln!(
        "[clipboard] copy request base64_chars={}",
        base64_image.len()
    );
    let bytes = decode_data_url(&base64_image)?;
    eprintln!("[clipboard] data-url decoded bytes={}", bytes.len());
    let rgba = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to parse image for clipboard: {}", e))?
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    eprintln!("[clipboard] image decoded width={} height={}", width, height);
    let mut clipboard = Clipboard::new().map_err(|e| {
        eprintln!("[clipboard] Clipboard::new failed: {}", e);
        format!("Clipboard error: {}", e)
    })?;
    eprintln!("[clipboard] Clipboard::new ok; writing image");
    clipboard
        .set_image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: std::borrow::Cow::Borrowed(&rgba),
        })
        .map_err(|e| {
            eprintln!("[clipboard] set_image failed: {}", e);
            format!("Failed to copy image to clipboard: {}", e)
        })?;
    eprintln!("[clipboard] set_image ok");
    Ok(())
}

use std::sync::atomic::{AtomicBool, Ordering};

static SCROLL_CANCELLED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn cancel_scroll_capture() {
    SCROLL_CANCELLED.store(true, Ordering::SeqCst);
}

const MAX_SCROLL_STEPS: usize = 120;
const MAX_STITCHED_HEIGHT: u32 = 60_000;

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra_info: usize);
}

struct ScrollController {
    #[cfg(not(target_os = "windows"))]
    input: Enigo,
}

impl ScrollController {
    fn new() -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        {
            Ok(Self {})
        }
        #[cfg(not(target_os = "windows"))]
        {
            let input = Enigo::new(&Settings::default())
                .map_err(|e| format!("Failed to initialize input control: {}", e))?;
            Ok(Self { input })
        }
    }

    fn position_pointer(&mut self, x: i32, y: i32) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        unsafe {
            if SetCursorPos(x, y) == 0 {
                return Err("Failed to position pointer".to_string());
            }
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.input
                .move_mouse(x, y, Coordinate::Abs)
                .map_err(|e| format!("Failed to position pointer: {}", e))
        }
    }

    fn scroll_down(&mut self, lines: i32) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        unsafe {
            const MOUSEEVENTF_WHEEL: u32 = 0x0800;
            const WHEEL_DELTA: i32 = 120;
            let delta = -lines.saturating_mul(WHEEL_DELTA);
            mouse_event(MOUSEEVENTF_WHEEL, 0, 0, delta as u32, 0);
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.input
                .scroll(lines, Axis::Vertical)
                .map_err(|e| format!("Failed to scroll target window: {}", e))
        }
    }
}

fn sampled_difference(img1: &RgbaImage, img2: &RgbaImage, shift: u32) -> Option<f64> {
    let (width, height) = img1.dimensions();
    if width != img2.width() || height != img2.height() || height < 200 {
        return None;
    }

    // Ignore window borders/toolbars as much as possible.  For a downward
    // scroll by `shift`, img1[y + shift] should equal img2[y].
    let left = width / 8;
    let right = width.saturating_sub(width / 8);
    let top = height / 8;
    let bottom = height.saturating_sub(height / 10);
    if top + shift + 24 >= bottom || left + 24 >= right {
        return None;
    }

    let mut difference = 0u64;
    let mut samples = 0u64;
    for y in (top..bottom - shift).step_by(8) {
        for x in (left..right).step_by(10) {
            let before = img1.get_pixel(x, y + shift);
            let after = img2.get_pixel(x, y);
            difference += (before[0] as i32 - after[0] as i32).unsigned_abs() as u64
                + (before[1] as i32 - after[1] as i32).unsigned_abs() as u64
                + (before[2] as i32 - after[2] as i32).unsigned_abs() as u64;
            samples += 3;
        }
    }
    (samples > 0).then_some(difference as f64 / samples as f64)
}

fn frames_are_stable(img1: &RgbaImage, img2: &RgbaImage) -> bool {
    sampled_difference(img1, img2, 0).is_some_and(|difference| difference < 2.5)
}

pub fn find_scroll_shift(img1: &RgbaImage, img2: &RgbaImage) -> Option<u32> {
    let (_, height) = img1.dimensions();
    if img1.dimensions() != img2.dimensions() || height < 200 {
        return None;
    }

    let zero_difference = sampled_difference(img1, img2, 0)?;
    if zero_difference < 2.5 {
        return None;
    }

    let max_shift = (height * 3 / 4).max(8);
    let mut best = None::<(u32, f64)>;

    // Coarse search followed by a small local refinement keeps matching fast
    // enough for high-DPI captures while allowing a broad scroll range.
    for shift in (4..=max_shift).step_by(4) {
        if let Some(difference) = sampled_difference(img1, img2, shift) {
            if best.is_none_or(|(_, best_difference)| difference < best_difference) {
                best = Some((shift, difference));
            }
        }
    }

    let (coarse_shift, _) = best?;
    let start = coarse_shift.saturating_sub(3).max(1);
    let end = (coarse_shift + 3).min(max_shift);
    for shift in start..=end {
        if let Some(difference) = sampled_difference(img1, img2, shift) {
            if best.is_none_or(|(_, best_difference)| difference < best_difference) {
                best = Some((shift, difference));
            }
        }
    }

    let (shift, difference) = best?;
    (difference < 28.0 && difference + 1.0 < zero_difference).then_some(shift)
}

fn find_scroll_shift_near(
    img1: &RgbaImage,
    img2: &RgbaImage,
    expected: u32,
) -> Option<u32> {
    let zero_difference = sampled_difference(img1, img2, 0)?;
    if zero_difference < 2.5 {
        return None;
    }
    let min_shift = expected.saturating_sub(48).max(1);
    let max_shift = expected.saturating_add(48).min(img1.height().saturating_sub(25));
    let mut best = None::<(u32, f64)>;
    for shift in min_shift..=max_shift {
        if let Some(difference) = sampled_difference(img1, img2, shift) {
            if best.is_none_or(|(_, best_difference)| difference < best_difference) {
                best = Some((shift, difference));
            }
        }
    }
    let (shift, difference) = best?;
    (difference < 28.0 && difference + 1.0 < zero_difference).then_some(shift)
}

fn fixed_row_mask(before: &RgbaImage, after: &RgbaImage) -> Vec<bool> {
    let width = after.width();
    let height = after.height();
    let mut candidates = vec![false; height as usize];
    for y in 0..height {
        let mut difference = 0u64;
        let mut sum = 0f64;
        let mut sum_sq = 0f64;
        let mut samples = 0u64;
        for x in (0..width).step_by(8) {
            let a = before.get_pixel(x, y);
            let b = after.get_pixel(x, y);
            difference += (a[0] as i32 - b[0] as i32).unsigned_abs() as u64
                + (a[1] as i32 - b[1] as i32).unsigned_abs() as u64
                + (a[2] as i32 - b[2] as i32).unsigned_abs() as u64;
            let luminance = (b[0] as f64 + b[1] as f64 + b[2] as f64) / 3.0;
            sum += luminance;
            sum_sq += luminance * luminance;
            samples += 1;
        }
        let mean = sum / samples.max(1) as f64;
        let variance = sum_sq / samples.max(1) as f64 - mean * mean;
        candidates[y as usize] = samples > 0
            && (difference as f64 / (samples * 3) as f64) < 4.0
            && variance > 80.0;
    }

    // Isolated unchanged rows are normal document content. Only classify a
    // substantial contiguous band as a fixed overlay.
    let mut mask = vec![false; height as usize];
    let mut start = 0usize;
    while start < candidates.len() {
        if !candidates[start] {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < candidates.len() && candidates[end] {
            end += 1;
        }
        if end - start >= 16 {
            mask[start..end].fill(true);
        }
        start = end;
    }
    mask
}

fn fixed_column_mask(before: &RgbaImage, after: &RgbaImage) -> Vec<bool> {
    let width = after.width();
    let height = after.height();
    let mut candidates = vec![false; width as usize];
    for x in 0..width {
        let mut difference = 0u64;
        let mut sum = 0f64;
        let mut sum_sq = 0f64;
        let mut samples = 0u64;
        for y in (0..height).step_by(8) {
            let a = before.get_pixel(x, y);
            let b = after.get_pixel(x, y);
            difference += (a[0] as i32 - b[0] as i32).unsigned_abs() as u64
                + (a[1] as i32 - b[1] as i32).unsigned_abs() as u64
                + (a[2] as i32 - b[2] as i32).unsigned_abs() as u64;
            let luminance = (b[0] as f64 + b[1] as f64 + b[2] as f64) / 3.0;
            sum += luminance;
            sum_sq += luminance * luminance;
            samples += 1;
        }
        let mean = sum / samples.max(1) as f64;
        let variance = sum_sq / samples.max(1) as f64 - mean * mean;
        candidates[x as usize] = samples > 0
            && (difference as f64 / (samples * 3) as f64) < 4.0
            && variance > 80.0;
    }
    let mut mask = vec![false; width as usize];
    let mut start = 0usize;
    while start < candidates.len() {
        if !candidates[start] {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < candidates.len() && candidates[end] {
            end += 1;
        }
        if end - start >= 16 {
            mask[start..end].fill(true);
        }
        start = end;
    }
    mask
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrollCaptureStrategy {
    BrowserPage,
    DocumentApp,
    DesktopStitch,
}

fn classify_scroll_target(app_name: &str, title: &str) -> ScrollCaptureStrategy {
    let app = app_name.to_ascii_lowercase();
    let title = title.to_ascii_lowercase();
    if ["google chrome", "chrome", "brave browser", "microsoft edge", "safari", "firefox"]
        .iter()
        .any(|name| app.contains(name))
        || title.contains("google chrome")
        || title.contains("microsoft edge")
    {
        ScrollCaptureStrategy::BrowserPage
    } else if ["microsoft word", "pages", "libreoffice", "preview", "acrobat"]
        .iter()
        .any(|name| app.contains(name) || title.contains(name))
    {
        ScrollCaptureStrategy::DocumentApp
    } else {
        ScrollCaptureStrategy::DesktopStitch
    }
}

#[tauri::command]
pub fn auto_scroll_capture_window(
    app: tauri::AppHandle,
    monitor_index: usize,
    selection_x: f64,
    selection_y: f64,
    selection_width: f64,
    selection_height: f64,
) -> Result<String, String> {
    SCROLL_CANCELLED.store(false, Ordering::SeqCst);

    let tauri_monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to list Tauri monitors: {}", e))?;
    let tauri_monitor = tauri_monitors
        .get(monitor_index)
        .or_else(|| tauri_monitors.first())
        .ok_or_else(|| "No monitor found".to_string())?;
    let monitor_x = tauri_monitor.position().x;
    let monitor_y = tauri_monitor.position().y;
    let scale_factor = tauri_monitor.scale_factor();

    let monitors =
        xcap::Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {}", e))?;
    let mon = monitors
        .iter()
        .find(|monitor| monitor.x().ok() == Some(monitor_x) && monitor.y().ok() == Some(monitor_y))
        .or_else(|| {
            let name = tauri_monitor.name().cloned().unwrap_or_default();
            monitors
                .iter()
                .find(|monitor| monitor.name().unwrap_or_default() == name)
        })
        .or_else(|| monitors.get(monitor_index))
        .or_else(|| monitors.first())
        .ok_or_else(|| "No matching xcap monitor found".to_string())?;

    let mon_x = mon.x().map_err(|e| e.to_string())?;
    let mon_y = mon.y().map_err(|e| e.to_string())?;
    // The capture overlay reports logical WebView points, while xcap window
    // images are physical pixels on Retina macOS displays.  Use the monitor
    // scale for the selected rectangle on every platform (1.0 on standard
    // DPI displays) so a full-width selection is not cropped to half width.
    let coordinate_scale = scale_factor;
    let global_selection_x = mon_x + (selection_x * coordinate_scale).round() as i32;
    let global_selection_y = mon_y + (selection_y * coordinate_scale).round() as i32;
    let global_selection_width = (selection_width * coordinate_scale).round().max(1.0) as u32;
    let global_selection_height = (selection_height * coordinate_scale).round().max(1.0) as u32;

    let windows = xcap::Window::all().map_err(|e| format!("Failed to list windows: {}", e))?;

    // Find top-most window under click coordinates
    let target_win = windows.into_iter().find(|w| {
        let title = w.title().unwrap_or_default();
        let app = w.app_name().unwrap_or_default();
        if app == "Dock"
            || app == "Window Server"
            || title.starts_with("Capture Window")
            || title.starts_with("tauri-app")
        {
            return false;
        }
        if let (Ok(wx), Ok(wy), Ok(ww), Ok(wh), Ok(min)) =
            (w.x(), w.y(), w.width(), w.height(), w.is_minimized())
        {
            if min || ww < 200 || wh < 200 {
                return false;
            }
            global_selection_x >= wx
                && global_selection_x < wx + ww as i32
                && global_selection_y >= wy
                && global_selection_y < wy + wh as i32
        } else {
            false
        }
    });

    let win = target_win.ok_or_else(|| "未找到對應的應用程式視窗，請點擊有效視窗".to_string())?;
    let target_app = win.app_name().unwrap_or_default();
    let target_title = win.title().unwrap_or_default();
    let strategy = classify_scroll_target(&target_app, &target_title);
    let strategy_name = match strategy {
        ScrollCaptureStrategy::BrowserPage => "browser-page",
        ScrollCaptureStrategy::DocumentApp => "document-app",
        ScrollCaptureStrategy::DesktopStitch => "desktop-stitch",
    };
    eprintln!(
        "[scroll] target app={:?} title={:?} strategy={}",
        target_app, target_title, strategy_name
    );
    if strategy != ScrollCaptureStrategy::DesktopStitch {
        eprintln!(
            "[scroll] fallback=desktop-stitch reason=content-layer-export-not-available"
        );
        let message = match strategy {
            ScrollCaptureStrategy::BrowserPage => {
                "已辨識為瀏覽器；目前使用桌面拼接，無法保證固定網頁元件後方內容完整"
            }
            ScrollCaptureStrategy::DocumentApp => {
                "已辨識為文件應用程式；目前使用桌面拼接，建議優先使用文件匯出或列印功能"
            }
            ScrollCaptureStrategy::DesktopStitch => "",
        };
        let _ = app.emit(
            "scroll-capture-strategy",
            serde_json::json!({ "strategy": strategy_name, "message": message }),
        );
    }

    let win_x = win.x().map_err(|e| e.to_string())?;
    let win_y = win.y().map_err(|e| e.to_string())?;
    let crop_x = global_selection_x.saturating_sub(win_x) as u32;
    let crop_y = global_selection_y.saturating_sub(win_y) as u32;
    let crop_frame = |frame: RgbaImage| -> Result<RgbaImage, String> {
        if crop_x >= frame.width()
            || crop_y >= frame.height()
            || crop_x.saturating_add(global_selection_width) > frame.width()
            || crop_y.saturating_add(global_selection_height) > frame.height()
        {
            return Err(format!(
                "選取範圍超出目標視窗：selection=({},{} {}x{}) window-frame={}x{}",
                crop_x,
                crop_y,
                global_selection_width,
                global_selection_height,
                frame.width(),
                frame.height()
            ));
        }
        Ok(image::imageops::crop_imm(
            &frame,
            crop_x,
            crop_y,
            global_selection_width,
            global_selection_height,
        )
        .to_image())
    };
    eprintln!(
        "[scroll] selection logical=({},{} {}x{}) scale={:.3} global=({},{} {}x{}) window_origin=({},{}), crop=({},{} {}x{})",
        selection_x,
        selection_y,
        selection_width,
        selection_height,
        coordinate_scale,
        global_selection_x,
        global_selection_y,
        global_selection_width,
        global_selection_height,
        win_x,
        win_y,
        crop_x,
        crop_y,
        global_selection_width,
        global_selection_height
    );

    let mut input = ScrollController::new()?;
    // Move the pointer over the selected content without synthesizing a
    // click.  Clicking here can activate a browser tab or press a control
    // before scrolling starts.
    input.position_pointer(global_selection_x, global_selection_y)?;
    std::thread::sleep(std::time::Duration::from_millis(250));

    let first_frame = crop_frame(win
        .capture_image()
        .map_err(|e| format!("Capture frame 1 failed: {}", e))?)?;
    let frame_width = first_frame.width();
    let frame_height = first_frame.height();
    let mut previous_frame = first_frame.clone();
    let mut frames = vec![first_frame.clone()];
    let mut frame_offsets = vec![0u32];
    let mut fixed_masks: Vec<(Vec<bool>, Vec<bool>)> = Vec::new();
    let mut total_height = frame_height;
    let mut stable_attempts = 0usize;
    let mut reached_bottom = false;
    let mut was_cancelled = false;

    for step in 1..=MAX_SCROLL_STEPS {
        if SCROLL_CANCELLED.load(Ordering::SeqCst) {
            was_cancelled = true;
            break;
        }

        input
            .scroll_down(6)
            .map_err(|e| format!("Failed to scroll target window at step {}: {}", step, e))?;
        std::thread::sleep(std::time::Duration::from_millis(350));

        if SCROLL_CANCELLED.load(Ordering::SeqCst) {
            was_cancelled = true;
            break;
        }

        let mut next_frame = crop_frame(win
            .capture_image()
            .map_err(|e| format!("Capture frame {} failed: {}", step + 1, e))?)?;
        let mut settle_attempts = 0usize;
        // Do not stitch a frame while the target is still animating or
        // re-laying out lazy content.  Capture successive samples until two
        // adjacent samples are visually stable, up to a bounded timeout.
        for attempt in 1..=8 {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let candidate = crop_frame(win
                .capture_image()
                .map_err(|e| format!("Capture settle frame {} failed: {}", step + 1, e))?)?;
            settle_attempts = attempt;
            if frames_are_stable(&next_frame, &candidate) {
                next_frame = candidate;
                break;
            }
            next_frame = candidate;
        }
        eprintln!("[scroll] step={} settle_attempts={}", step, settle_attempts);
        if next_frame.dimensions() != previous_frame.dimensions() {
            return Err("Target window size changed during long capture".to_string());
        }

        if SCROLL_CANCELLED.load(Ordering::SeqCst) {
            was_cancelled = true;
            break;
        }

        let detected_shift = if frames.len() > 1 {
            let expected = frame_offsets[1].saturating_sub(frame_offsets[0]);
            find_scroll_shift_near(&previous_frame, &next_frame, expected)
        } else {
            find_scroll_shift(&previous_frame, &next_frame)
        };
        if let Some(shift) = detected_shift {
            stable_attempts = 0;
            let current_offset = total_height.saturating_sub(frame_height);
            let next_offset = current_offset
                .checked_add(shift)
                .ok_or_else(|| "Long screenshot height overflow".to_string())?;
            let next_total_height = frame_height
                .checked_add(next_offset)
                .ok_or_else(|| "Long screenshot height overflow".to_string())?;
            if next_total_height > MAX_STITCHED_HEIGHT {
                return Err(format!(
                    "長截圖超過安全高度 {} 像素，請縮小範圍或分段擷取",
                    MAX_STITCHED_HEIGHT
                ));
            }
            fixed_masks.push((
                fixed_row_mask(&previous_frame, &next_frame),
                fixed_column_mask(&previous_frame, &next_frame),
            ));
            frames.push(next_frame.clone());
            frame_offsets.push(next_offset);
            eprintln!("[scroll] step={} shift={} offset={} frame={}x{}", step, shift, next_offset, frame_width, frame_height);
            total_height = next_total_height;
            previous_frame = next_frame;
        } else {
            if frames_are_stable(&previous_frame, &next_frame) {
                stable_attempts += 1;
                if stable_attempts >= 2 {
                    reached_bottom = true;
                    break;
                }
            } else {
                if SCROLL_CANCELLED.load(Ordering::SeqCst) {
                    was_cancelled = true;
                    break;
                }
                // Some windows do not expose a detectable scrollable region
                // (or their content changes without a stable pixel shift).
                // Keep the initial frame instead of reporting a failed long
                // screenshot; the user still receives the selected window.
                if frames.len() == 1 {
                    let mut buffer = Vec::new();
                    first_frame
                        .write_to(
                            &mut std::io::Cursor::new(&mut buffer),
                            image::ImageFormat::Png,
                        )
                        .map_err(|e| format!("Encode error: {}", e))?;
                    return Ok(format!(
                        "data:image/png;base64,{}",
                        STANDARD.encode(&buffer)
                    ));
                }
                return Err(format!(
                    "第 {} 次捲動後無法可靠比對影像；已停止以避免產生缺段截圖",
                    step
                ));
            }
        }
    }

    if !reached_bottom && !was_cancelled {
        return Err(format!(
            "已達 {} 次安全上限但尚未確認頁尾，未輸出可能不完整的截圖",
            MAX_SCROLL_STEPS
        ));
    }

    // Compose in document coordinates, but only accept each destination pixel
    // once.  Fixed overlays are skipped on later frames; overlapping frames
    // can therefore back-fill pixels hidden by a composer/sidebar instead of
    // leaving a white hole or duplicating the overlay at every boundary.
    let mut composite = RgbaImage::from_pixel(
        frame_width,
        total_height,
        image::Rgba([255, 255, 255, 255]),
    );
    let mut filled = vec![false; (frame_width * total_height) as usize];
    let mut blocked_rows = vec![vec![false; frame_height as usize]; frames.len()];
    let mut blocked_columns = vec![vec![false; frame_width as usize]; frames.len()];
    for (transition, (row_mask, column_mask)) in fixed_masks.iter().enumerate() {
        // Omit fixed overlays in both adjacent frames.  Otherwise pixels
        // hidden by a bottom composer in the first frame are marked as filled
        // too early and remain as pale/partial text in the final document.
        for (row, is_fixed) in row_mask.iter().enumerate() {
            if *is_fixed {
                blocked_rows[transition][row] = true;
                blocked_rows[transition + 1][row] = true;
            }
        }
        for (column, is_fixed) in column_mask.iter().enumerate() {
            if *is_fixed {
                blocked_columns[transition][column] = true;
                blocked_columns[transition + 1][column] = true;
            }
        }
    }
    for (frame_index, frame) in frames.iter().enumerate() {
        let offset = frame_offsets[frame_index];
        for y in 0..frame_height {
            if blocked_rows[frame_index][y as usize] {
                continue;
            }
            let destination_y = offset + y;
            if destination_y >= total_height {
                continue;
            }
            for x in 0..frame_width {
                if blocked_columns[frame_index][x as usize] {
                    continue;
                }
                let index = (destination_y * frame_width + x) as usize;
                if !filled[index] {
                    composite.put_pixel(x, destination_y, *frame.get_pixel(x, y));
                    filled[index] = true;
                }
            }
        }
    }

    let mut buffer = Vec::new();
    eprintln!("[scroll] composite frames={} output={}x{}", frames.len(), frame_width, total_height);
    composite
        .write_to(
            &mut std::io::Cursor::new(&mut buffer),
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("Encode error: {}", e))?;

    let base64_image = STANDARD.encode(&buffer);
    Ok(format!("data:image/png;base64,{}", base64_image))
}

#[cfg(test)]
mod capture_tests {
    use super::{
        classify_scroll_target, find_scroll_shift, find_scroll_shift_near, fixed_column_mask,
        fixed_row_mask, frames_are_stable, work_area_crop_bounds, CropBounds,
        ScrollCaptureStrategy,
    };
    use image::{imageops::crop_imm, Rgba, RgbaImage};

    fn patterned_document(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            Rgba([
                ((x * 17 + y * 3) % 251) as u8,
                ((x * 5 + y * 11) % 247) as u8,
                ((x * 13 + y * 7) % 241) as u8,
                255,
            ])
        })
    }

    #[test]
    fn detects_known_vertical_scroll_shift() {
        let document = patterned_document(320, 900);
        let before = crop_imm(&document, 0, 100, 320, 360).to_image();
        let after = crop_imm(&document, 0, 237, 320, 360).to_image();

        assert_eq!(find_scroll_shift(&before, &after), Some(137));
        assert_eq!(find_scroll_shift_near(&before, &after, 137), Some(137));
    }

    #[test]
    fn identical_frames_are_treated_as_stopped() {
        let frame = patterned_document(320, 360);

        assert!(frames_are_stable(&frame, &frame));
        assert_eq!(find_scroll_shift(&frame, &frame), None);
    }

    #[test]
    fn rejects_frames_with_different_dimensions() {
        let before = patterned_document(320, 360);
        let after = patterned_document(300, 360);

        assert_eq!(find_scroll_shift(&before, &after), None);
    }

    #[test]
    fn identifies_fixed_sidebar_without_marking_document_columns() {
        let document = patterned_document(160, 320);
        let mut before = crop_imm(&document, 0, 0, 160, 240).to_image();
        let mut after = crop_imm(&document, 0, 24, 160, 240).to_image();
        for y in 0..240 {
            for x in 0..32 {
                let pixel = if ((x / 8) + (y / 8)) % 2 == 0 {
                    Rgba([20, 40, 80, 255])
                } else {
                    Rgba([220, 180, 60, 255])
                };
                before.put_pixel(x, y, pixel);
                after.put_pixel(x, y, pixel);
            }
        }
        let mask = fixed_column_mask(&before, &after);
        assert!(mask[..32].iter().all(|value| *value));
        assert!(mask[48..].iter().any(|value| !*value));
    }

    #[test]
    fn identifies_fixed_bottom_band_only_when_contiguous() {
        let document = patterned_document(160, 320);
        let mut before = crop_imm(&document, 0, 0, 160, 240).to_image();
        let mut after = crop_imm(&document, 0, 24, 160, 240).to_image();
        for y in 208..240 {
            for x in 0..160 {
                let pixel = if ((x / 8) + (y / 8)) % 2 == 0 {
                    Rgba([32, 32, 32, 255])
                } else {
                    Rgba([210, 210, 210, 255])
                };
                before.put_pixel(x, y, pixel);
                after.put_pixel(x, y, pixel);
            }
        }
        let mask = fixed_row_mask(&before, &after);
        assert!(mask[208..].iter().all(|value| *value));
        assert!(mask[..192].iter().any(|value| !*value));
    }

    #[test]
    fn classifies_browser_and_document_targets_before_generic_stitching() {
        assert_eq!(
            classify_scroll_target("Google Chrome", "Example"),
            ScrollCaptureStrategy::BrowserPage
        );
        assert_eq!(
            classify_scroll_target("Microsoft Word", "Document1"),
            ScrollCaptureStrategy::DocumentApp
        );
        assert_eq!(
            classify_scroll_target("Preview", "Screenshot"),
            ScrollCaptureStrategy::DocumentApp
        );
        assert_eq!(
            classify_scroll_target("Terminal", "zsh"),
            ScrollCaptureStrategy::DesktopStitch
        );
    }

    #[test]
    fn work_area_on_offset_monitor_is_converted_to_local_image_coordinates() {
        // Secondary monitor begins at desktop x=2408, while its usable area
        // begins 40 physical pixels below its own top edge (menu bar).
        assert_eq!(
            work_area_crop_bounds(2408, 0, 2408, 40, 1920, 1040, 1920, 1080),
            Some(CropBounds {
                x: 0,
                y: 40,
                width: 1920,
                height: 1040,
            })
        );
    }

    #[test]
    fn work_area_is_not_scaled_a_second_time_on_retina_monitor() {
        assert_eq!(
            work_area_crop_bounds(0, 0, 0, 72, 2408, 1434, 2408, 1506),
            Some(CropBounds {
                x: 0,
                y: 72,
                width: 2408,
                height: 1434,
            })
        );
    }
}

// Fix 9: Frontend handles its own show/hide for full/work area captures.
// Backend only captures and saves; no restore needed here.
#[tauri::command]
pub fn capture_full_screen(
    app: tauri::AppHandle,
    monitor_index: Option<usize>,
) -> Result<String, String> {
    std::thread::sleep(std::time::Duration::from_millis(400));

    let tauri_monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to get Tauri monitors: {}", e))?;
    if tauri_monitors.is_empty() {
        return Err("No monitors found".to_string());
    }
    let idx = monitor_index.unwrap_or(0).min(tauri_monitors.len() - 1);
    let tauri_mon = &tauri_monitors[idx];
    let phys_x = tauri_mon.position().x;
    let phys_y = tauri_mon.position().y;

    let xcap_monitors =
        Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {}", e))?;
    let matched_xcap = xcap_monitors
        .iter()
        .find(|xm| {
            let xm_x = xm.x().unwrap_or(i32::MIN);
            let xm_y = xm.y().unwrap_or(i32::MIN);
            xm_x == phys_x && xm_y == phys_y
        })
        .or_else(|| {
            let tname = tauri_mon.name().cloned().unwrap_or_default();
            xcap_monitors
                .iter()
                .find(|xm| xm.name().unwrap_or_default() == tname)
        })
        .or_else(|| xcap_monitors.get(idx));

    let target_xcap =
        matched_xcap.ok_or_else(|| format!("No matching monitor at index {}", idx))?;
    let img = target_xcap
        .capture_image()
        .map_err(|e| format!("Capture error: {}", e))?;

    let config = crate::config::load_config(app.clone()).unwrap_or_default();
    let mut buffer = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buffer),
        image::ImageFormat::Png,
    )
    .map_err(|e| format!("Encode error: {}", e))?;
    let base64_image = STANDARD.encode(&buffer);

    let default_name = format!(
        "Screenshot_{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let save_path = std::path::PathBuf::from(&config.save_directory).join(default_name);
    let path_str = save_path.to_string_lossy().to_string();
    let _ = save_and_copy_screenshot(
        app.clone(),
        base64_image,
        Some(path_str.clone()),
        config.auto_copy_to_clipboard,
    );
    Ok(path_str)
}

#[tauri::command]
pub fn capture_work_area(
    app: tauri::AppHandle,
    monitor_index: Option<usize>,
) -> Result<String, String> {
    std::thread::sleep(std::time::Duration::from_millis(400));

    let tauri_monitors = app
        .available_monitors()
        .map_err(|e| format!("Failed to get Tauri monitors: {}", e))?;
    if tauri_monitors.is_empty() {
        return Err("No monitors found".to_string());
    }
    let idx = monitor_index.unwrap_or(0).min(tauri_monitors.len() - 1);
    let tauri_mon = &tauri_monitors[idx];
    let phys_x = tauri_mon.position().x;
    let phys_y = tauri_mon.position().y;
    let work_area = tauri_mon.work_area();

    let xcap_monitors =
        Monitor::all().map_err(|e| format!("Failed to list xcap monitors: {}", e))?;
    let matched_xcap = xcap_monitors
        .iter()
        .find(|xm| {
            let xm_x = xm.x().unwrap_or(i32::MIN);
            let xm_y = xm.y().unwrap_or(i32::MIN);
            xm_x == phys_x && xm_y == phys_y
        })
        .or_else(|| {
            let tname = tauri_mon.name().cloned().unwrap_or_default();
            xcap_monitors
                .iter()
                .find(|xm| xm.name().unwrap_or_default() == tname)
        })
        .or_else(|| xcap_monitors.get(idx));

    let target_xcap =
        matched_xcap.ok_or_else(|| format!("No matching monitor at index {}", idx))?;
    let full_img = target_xcap
        .capture_image()
        .map_err(|e| format!("Capture error: {}", e))?;
    let mut rgba_img = full_img;

    let bounds = work_area_crop_bounds(
        phys_x,
        phys_y,
        work_area.position.x,
        work_area.position.y,
        work_area.size.width,
        work_area.size.height,
        rgba_img.width(),
        rgba_img.height(),
    )
    .ok_or_else(|| "Work area is outside the selected monitor".to_string())?;

    let cropped_img = image::imageops::crop(
        &mut rgba_img,
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
    )
    .to_image();

    let config = crate::config::load_config(app.clone()).unwrap_or_default();
    let mut buffer = Vec::new();
    cropped_img
        .write_to(
            &mut std::io::Cursor::new(&mut buffer),
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("Encode error: {}", e))?;
    let base64_image = STANDARD.encode(&buffer);

    let default_name = format!(
        "Screenshot_WorkArea_{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let save_path = std::path::PathBuf::from(&config.save_directory).join(default_name);
    let path_str = save_path.to_string_lossy().to_string();
    let _ = save_and_copy_screenshot(
        app.clone(),
        base64_image,
        Some(path_str.clone()),
        config.auto_copy_to_clipboard,
    );
    Ok(path_str)
}
