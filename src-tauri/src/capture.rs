use crate::platform::windows::logical_to_physical_rect;
use base64::{engine::general_purpose::STANDARD, Engine as _};
#[cfg(not(target_os = "windows"))]
use enigo::{Axis, Coordinate, Enigo, Mouse, Settings};
use image::RgbaImage;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use xcap::Monitor;

static CAPTURE_WINDOW_LAUNCHING: AtomicBool = AtomicBool::new(false);
static SCROLL_CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);

const WINDOWS_CAPTURE_SURFACE_SETTLE_MS: u64 = 140;

struct CaptureLaunchGuard;

impl Drop for CaptureLaunchGuard {
    fn drop(&mut self) {
        CAPTURE_WINDOW_LAUNCHING.store(false, Ordering::Release);
    }
}

struct ScrollCaptureGuard;

impl ScrollCaptureGuard {
    fn acquire() -> Result<Self, String> {
        SCROLL_CAPTURE_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "長截圖已在執行中，請勿重複啟動".to_string())
    }
}

impl Drop for ScrollCaptureGuard {
    fn drop(&mut self) {
        SCROLL_CAPTURE_ACTIVE.store(false, Ordering::Release);
    }
}

fn wait_for_capture_surface_to_hide(app: &tauri::AppHandle, hide_main: bool) {
    if hide_main {
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.hide();
        }
    }

    #[cfg(target_os = "windows")]
    std::thread::sleep(std::time::Duration::from_millis(
        WINDOWS_CAPTURE_SURFACE_SETTLE_MS,
    ));
    #[cfg(not(target_os = "windows"))]
    std::thread::sleep(std::time::Duration::from_millis(40));
}

fn is_unisnap_window(app_name: &str, title: &str) -> bool {
    let app = app_name.to_ascii_lowercase();
    let title = title.to_ascii_lowercase();
    app.contains("unisnap") || title.contains("unisnap") || title.starts_with("capture window")
}

fn xcap_window_coordinate_scale(monitor_scale: f64) -> f64 {
    // On Windows this process is per-monitor DPI aware, so xcap/Win32 window
    // bounds are already physical desktop pixels.  Scaling those values again
    // shifts the target and crop on 125%/150% displays.
    #[cfg(target_os = "windows")]
    {
        let _ = monitor_scale;
        1.0
    }
    #[cfg(not(target_os = "windows"))]
    {
        monitor_scale
    }
}

fn capture_window_route(label: &str, mode: &str) -> String {
    format!("index.html#/capture?label={label}&mode={mode}")
}

pub fn log_backend(msg: &str) {
    println!("{}", msg);
    use std::fs::OpenOptions;
    use std::io::Write;
    let log_path = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|directory| directory.join("UniSnap").join("backend.log"))
        .unwrap_or_else(|| std::path::PathBuf::from("backend.log"));
    if let Some(directory) = log_path.parent() {
        let _ = std::fs::create_dir_all(directory);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{}", msg);
    }
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct MonitorBasicInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

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
            let fallback = m
                .name()
                .cloned()
                .unwrap_or_else(|| "未命名顯示器".to_string());
            let name = xcap_monitors
                .iter()
                .find_map(|xm| {
                    let same_position = xm.x().ok() == Some(x) && xm.y().ok() == Some(y);
                    same_position.then(|| xm.name().ok()).flatten()
                })
                .unwrap_or(fallback);
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

#[derive(serde::Serialize, Clone, Debug)]
pub struct MonitorScreenshot {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub base64_image: String,
}

use crate::capture_geometry::work_area_crop_bounds;
use crate::image_data::{decode_data_url, encode_requested_image, rgba_to_jpeg_data_url};
use crate::scroll_masks::{
    fixed_column_mask as domain_fixed_column_mask, fixed_row_mask as domain_fixed_row_mask,
};
use crate::scroll_matching::{
    find_scroll_shift as domain_find_scroll_shift,
    find_scroll_shift_near as domain_find_scroll_shift_near,
    frames_are_stable as domain_frames_are_stable,
};

/// Converts a desktop-global work-area rectangle into image-local pixels for
/// one monitor. Tauri positions and sizes are already physical pixels, as is
/// the image returned by xcap, so no DPI factor belongs in this conversion.
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
    mode: Option<String>,
    monitor_index: Option<usize>,
) -> Result<(), String> {
    // Ignore key-repeat and duplicate clicks while a capture overlay exists or
    // another overlay is still being created. Concurrent WebView2 builders can
    // otherwise leave an opaque about:blank window over the desktop.
    if app
        .webview_windows()
        .iter()
        .any(|(label, _)| label.starts_with("capture_"))
    {
        return Ok(());
    }
    if CAPTURE_WINDOW_LAUNCHING.swap(true, Ordering::AcqRel) {
        return Ok(());
    }

    log_backend("[capture] trigger_screenshot command received. Spawning background task to prevent GUI deadlock.");
    tauri::async_runtime::spawn(async move {
        let _launch_guard = CaptureLaunchGuard;
        let state = app.state::<crate::PinnedImageState>();
        if let Err(e) = trigger_screenshot_impl(app.clone(), state, mode, monitor_index) {
            log_backend(&format!(
                "[capture] Error in trigger_screenshot_impl: {}",
                e
            ));
        }
    });
    Ok(())
}

fn trigger_screenshot_impl(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::PinnedImageState>,
    mode: Option<String>,
    monitor_index: Option<usize>,
) -> Result<(), String> {
    // Hiding a WebView is asynchronous relative to the Windows desktop
    // compositor.  Enforce the hide in native code and wait before GDI BitBlt,
    // so the UniSnap toolbar cannot be baked into the source screenshot.
    wait_for_capture_surface_to_hide(&app, true);

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

    // Track (x, y) positions we've already opened a window for, so we don't
    // create duplicate overlapping capture windows for Virtual Display Devices
    // that share coordinates with a real physical monitor.
    let mut seen_positions: std::collections::HashSet<(i32, i32)> =
        std::collections::HashSet::new();

    for index in indices {
        let tauri_mon = &tauri_monitors[index];
        let scale_factor = tauri_mon.scale_factor();

        let phys_x = tauri_mon.position().x;
        let phys_y = tauri_mon.position().y;
        let phys_w = tauri_mon.size().width;
        let phys_h = tauri_mon.size().height;

        // Skip monitors with invalid dimensions (e.g. Virtual Display with no active mode)
        if phys_w == 0 || phys_h == 0 {
            eprintln!(
                "[capture] Skipping monitor {} – zero dimensions ({}x{})",
                index, phys_w, phys_h
            );
            continue;
        }

        // Skip duplicate monitor positions (Virtual Display Device may sit at the same
        // (x, y) as the real NVIDIA output, which would create a blank white overlay on top).
        let pos_key = (phys_x, phys_y);
        if seen_positions.contains(&pos_key) {
            eprintln!(
                "[capture] Skipping duplicate monitor {} at position ({}, {})",
                index, phys_x, phys_y
            );
            continue;
        }
        seen_positions.insert(pos_key);

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

        // On Windows, GDI (BitBlt) is the most reliable capture path.
        // xcap uses DXGI Desktop Duplication which can silently return white/blank
        // frames on systems with Virtual Display Devices or certain GPU configs.
        #[cfg(target_os = "windows")]
        let image = {
            log_backend(&format!(
                "[capture] Starting capture for monitor index {} at physical ({}, {}) size {}x{}",
                index, phys_x, phys_y, phys_w, phys_h
            ));
            let gdi_result = capture_monitor_gdi(phys_x, phys_y, phys_w, phys_h);
            match gdi_result {
                Ok(img) => {
                    if !is_blank_frame(&img) {
                        log_backend(&format!(
                            "[capture] GDI capture succeeded! Dimensions: {}x{}",
                            img.width(),
                            img.height()
                        ));
                        img
                    } else {
                        log_backend("[capture] GDI capture returned a blank/white frame. Attempting xcap fallback...");
                        let xcap_img = target_xcap.capture_image().unwrap_or_else(|e| {
                            log_backend(&format!("[capture] xcap fallback failed: {}. Reverting to original GDI frame.", e));
                            capture_monitor_gdi(phys_x, phys_y, phys_w, phys_h)
                                .unwrap_or_else(|_| image::RgbaImage::new(phys_w, phys_h))
                        });
                        if is_blank_frame(&xcap_img) {
                            log_backend("[capture] Warning: xcap fallback also returned a blank/white frame.");
                        } else {
                            log_backend(&format!(
                                "[capture] xcap fallback succeeded! Dimensions: {}x{}",
                                xcap_img.width(),
                                xcap_img.height()
                            ));
                        }
                        xcap_img
                    }
                }
                Err(e) => {
                    log_backend(&format!(
                        "[capture] GDI capture failed: {}. Attempting xcap fallback...",
                        e
                    ));
                    let xcap_img = target_xcap.capture_image().map_err(|ex| {
                        let err_msg = format!("Both GDI and xcap failed. GDI: {}, xcap: {}", e, ex);
                        log_backend(&format!("[capture] {}", err_msg));
                        err_msg
                    })?;
                    log_backend(&format!(
                        "[capture] xcap fallback succeeded after GDI failure. Dimensions: {}x{}",
                        xcap_img.width(),
                        xcap_img.height()
                    ));
                    xcap_img
                }
            }
        };
        #[cfg(not(target_os = "windows"))]
        let image = target_xcap
            .capture_image()
            .map_err(|e| format!("capture failed: {e}"))?;

        // Ultra-fast JPEG encode (15ms vs 1200ms PNG)
        let data_url = rgba_to_jpeg_data_url(&image)?;
        log_backend(&format!(
            "[capture] Encoded JPEG data URL. Length: {}",
            data_url.len()
        ));

        let label = format!("capture_{}_{}_{}", mode_str, index, timestamp);
        {
            let mut map = state.0.lock().unwrap();
            map.insert(label.clone(), data_url);
        }

        // Load a deterministic application route. Reusing the main window's
        // external URL can strand a release WebView at about:blank when a
        // global shortcut creates it while the main window is hidden.
        let route = capture_window_route(&label, &mode_str);
        let window_url = WebviewUrl::App(route.clone().into());

        let logical_x = phys_x as f64 / scale_factor;
        let logical_y = phys_y as f64 / scale_factor;
        let logical_w = phys_w as f64 / scale_factor;
        let logical_h = phys_h as f64 / scale_factor;
        log_backend(&format!(
            "[capture] Creating WebviewWindow '{}' at logical coordinates ({}, {}) size {}x{}",
            label, logical_x, logical_y, logical_w, logical_h
        ));

        let page_loaded = Arc::new(AtomicBool::new(false));
        let page_loaded_from_webview = Arc::clone(&page_loaded);
        let win = tauri::WebviewWindowBuilder::new(&app, &label, window_url)
            .title(format!("Capture Window {}", index))
            .decorations(false)
            .always_on_top(true)
            .transparent(false)
            .resizable(false)
            .focused(true)
            .visible(false)
            .accept_first_mouse(true)
            .inner_size(logical_w, logical_h)
            .position(logical_x, logical_y)
            .on_page_load(move |_window, payload| {
                if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                    page_loaded_from_webview.store(true, Ordering::Release);
                }
            })
            .build()
            .map_err(|e| {
                log_backend(&format!("[capture] Error building window: {}", e));
                format!("Failed to build capture window: {}", e)
            })?;

        // React shows the overlay only after the screenshot has decoded. If
        // initialisation fails, remove the still-hidden overlay and restore the
        // main window instead of covering or trapping the desktop.
        let watchdog_window = win.clone();
        let watchdog_app = app.clone();
        let watchdog_page_loaded = Arc::clone(&page_loaded);
        tauri::async_runtime::spawn(async move {
            std::thread::sleep(std::time::Duration::from_secs(8));
            // Visibility is not a readiness signal: starting a recording
            // deliberately hides this window. Only a WebView that never
            // finished loading should trigger recovery.
            if !watchdog_page_loaded.load(Ordering::Acquire) {
                let _ = watchdog_window.close();
                if let Some(main) = watchdog_app.get_webview_window("main") {
                    let _ = main.show();
                    let _ = main.set_focus();
                }
                log_backend("[capture] capture window readiness timeout; restored main window");
            }
        });

        if let Ok(u) = win.url() {
            log_backend(&format!(
                "[capture] WebviewWindow '{}' built successfully. Route: {} Target URL: {}",
                label, route, u
            ));
        }

        // Keep development builds visually identical to packaged builds. DevTools
        // may cover the capture target and would then be recorded. Developers can
        // opt in explicitly for a diagnostic session when it is actually needed.
        #[cfg(debug_assertions)]
        if matches!(
            std::env::var("UNISNAP_OPEN_DEVTOOLS").as_deref(),
            Ok("1") | Ok("true")
        ) {
            win.open_devtools();
        }
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

    let monitors = app
        .available_monitors()
        .map_err(|e| format!("無法取得螢幕資訊：{e}"))?;
    let screen = monitors
        .first()
        .ok_or_else(|| "找不到可用螢幕".to_string())?;
    let scale = screen.scale_factor();
    let max_w = (screen.size().width as f64 / scale * 0.86).max(640.0);
    let max_h = (screen.size().height as f64 / scale * 0.78).max(480.0);
    let fit = (max_w / width as f64).min(max_h / height as f64).min(1.0);
    let window_w = (width as f64 * fit).max(640.0);
    let window_h = (height as f64 * fit).max(480.0);
    let x =
        screen.position().x as f64 / scale + (screen.size().width as f64 / scale - window_w) / 2.0;
    let y =
        screen.position().y as f64 / scale + (screen.size().height as f64 / scale - window_h) / 2.0;
    let url = tauri::WebviewUrl::App("index.html".into());
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
    eprintln!(
        "[capture] open_image_editor ready label={} size={}x{}",
        label, width, height
    );
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
    let source = std::fs::read(file_path)
        .map_err(|e| format!("無法讀取圖片 {}：{e}", file_path.display()))?;
    let image = image::load_from_memory(&source).map_err(|e| format!("無法開啟圖片：{e}"))?;
    let width = image.width();
    let height = image.height();
    let mut encoded = Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|e| format!("無法準備圖片編輯資料：{e}"))?;
    create_image_editor_window(
        app,
        state,
        format!(
            "data:image/png;base64,{}",
            STANDARD.encode(encoded.into_inner())
        ),
        width,
        height,
    )
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
pub fn open_image_in_main_editor(app: tauri::AppHandle) -> Result<(), String> {
    let parent = app
        .get_webview_window("main")
        .ok_or_else(|| "找不到主視窗".to_string())?;
    let app_handle = app.clone();
    app.dialog()
        .file()
        .set_title("開啟圖片")
        .add_filter(
            "圖片",
            &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"],
        )
        .set_parent(&parent)
        .pick_file(move |selected| {
            let result = (|| -> Result<(), String> {
                let Some(file) = selected else {
                    return Ok(());
                };
                let path = file
                    .into_path()
                    .map_err(|e| format!("無法取得圖片路徑：{e}"))?;
                eprintln!("[capture] main_editor selected path={}", path.display());
                let source = std::fs::read(&path).map_err(|e| format!("無法讀取圖片：{e}"))?;
                let image =
                    image::load_from_memory(&source).map_err(|e| format!("無法開啟圖片：{e}"))?;
                let width = image.width();
                let height = image.height();
                let mut encoded = Cursor::new(Vec::new());
                image
                    .write_to(&mut encoded, image::ImageFormat::Png)
                    .map_err(|e| format!("無法準備圖片編輯資料：{e}"))?;
                let label = format!(
                    "main_editor_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0)
                );
                app_handle
                    .state::<crate::PinnedImageState>()
                    .0
                    .lock()
                    .unwrap()
                    .insert(
                        label.clone(),
                        format!(
                            "data:image/png;base64,{}",
                            STANDARD.encode(encoded.into_inner())
                        ),
                    );
                let main = app_handle
                    .get_webview_window("main")
                    .ok_or_else(|| "找不到主視窗".to_string())?;
                let monitor = main
                    .current_monitor()
                    .map_err(|e| format!("無法取得目前螢幕：{e}"))?
                    .or_else(|| app_handle.primary_monitor().ok().flatten())
                    .ok_or_else(|| "找不到可用螢幕".to_string())?;
                let scale = monitor.scale_factor();
                let fit = ((monitor.size().width as f64 / scale * 0.86) / width as f64)
                    .min((monitor.size().height as f64 / scale * 0.78) / height as f64)
                    .min(1.0);
                main.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
                    (width as f64 * fit).max(640.0),
                    (height as f64 * fit).max(480.0),
                )))
                .map_err(|e| format!("無法調整編輯視窗：{e}"))?;
                main.eval(&format!(
                    "window.location.hash = '#/capture?label={label}&mode=edit-main';"
                ))
                .map_err(|e| format!("無法開啟圖片編輯器：{e}"))?;
                main.show().map_err(|e| format!("無法顯示編輯視窗：{e}"))?;
                main.set_focus()
                    .map_err(|e| format!("無法聚焦編輯視窗：{e}"))?;
                eprintln!(
                    "[capture] main_editor ready label={} size={}x{}",
                    label, width, height
                );
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
        if label.starts_with("capture_")
            || label.starts_with("editor_")
            || label.starts_with("recording_control_")
        {
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
    let monitor_width = monitor.size().width as f64 / scale;
    let monitor_height = monitor.size().height as f64 / scale;
    let (local_x, local_y) =
        recording_control_position(x, y, width, height, monitor_width, monitor_height);
    let position_x = monitor.position().x as f64 / scale + local_x;
    let position_y = monitor.position().y as f64 / scale + local_y;

    let capture_label = app
        .webview_windows()
        .into_iter()
        .find(|(label, _)| {
            label.starts_with("capture_") && label.contains(&format!("_{monitor_index}_"))
        })
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
    #[cfg(target_os = "windows")]
    match exclude_window_from_capture(&control) {
        Ok(()) => log_backend("[capture] recording control excluded from Windows capture"),
        Err(error) => {
            // Positioning outside the selected rectangle remains the fallback
            // on Windows editions or drivers that reject display affinity.
            log_backend(&format!(
                "[capture] recording control exclusion unavailable; using outside-region fallback: {error}"
            ));
        }
    }
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

    let _ = (fps, record_audio);
    Ok(())
}

const RECORDING_CONTROL_WIDTH: f64 = 300.0;
const RECORDING_CONTROL_HEIGHT: f64 = 58.0;
const RECORDING_CONTROL_MARGIN: f64 = 12.0;

fn recording_control_position(
    selection_x: f64,
    selection_y: f64,
    selection_width: f64,
    selection_height: f64,
    monitor_width: f64,
    monitor_height: f64,
) -> (f64, f64) {
    let max_x = (monitor_width - RECORDING_CONTROL_WIDTH).max(0.0);
    let max_y = (monitor_height - RECORDING_CONTROL_HEIGHT).max(0.0);
    let aligned_x = selection_x.clamp(0.0, max_x);
    let aligned_y = selection_y.clamp(0.0, max_y);

    if selection_y >= RECORDING_CONTROL_HEIGHT + RECORDING_CONTROL_MARGIN {
        return (
            aligned_x,
            selection_y - RECORDING_CONTROL_HEIGHT - RECORDING_CONTROL_MARGIN,
        );
    }
    let below = selection_y + selection_height + RECORDING_CONTROL_MARGIN;
    if below + RECORDING_CONTROL_HEIGHT <= monitor_height {
        return (aligned_x, below);
    }
    if selection_x >= RECORDING_CONTROL_WIDTH + RECORDING_CONTROL_MARGIN {
        return (
            selection_x - RECORDING_CONTROL_WIDTH - RECORDING_CONTROL_MARGIN,
            aligned_y,
        );
    }
    let right = selection_x + selection_width + RECORDING_CONTROL_MARGIN;
    if right + RECORDING_CONTROL_WIDTH <= monitor_width {
        return (right, aligned_y);
    }

    // A full-screen or nearly full-screen selection leaves no outside space.
    // The native exclusion flag is the primary protection in this case.
    (12.0_f64.min(max_x), 12.0_f64.min(max_y))
}

#[cfg(target_os = "windows")]
fn exclude_window_from_capture(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
    };

    let hwnd = window
        .hwnd()
        .map_err(|error| format!("無法取得錄影控制列原生視窗：{error}"))?;
    let applied = unsafe { SetWindowDisplayAffinity(hwnd.0 as _, WDA_EXCLUDEFROMCAPTURE) };
    if applied == 0 {
        return Err(format!(
            "Windows 無法排除錄影控制列：{}",
            std::io::Error::last_os_error()
        ));
    }
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
    record_system_audio: bool,
) -> Result<(), String> {
    eprintln!(
        "[capture] open_recording_start_control monitor={} rect=({}, {}, {}, {}) fps={} microphone={} system_audio={}",
        monitor_index, x, y, width, height, fps, record_audio, record_system_audio
    );
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
    let canvas_width = (monitor.size().width as f64 / scale).round().max(1.0) as u32;
    let canvas_height = (monitor.size().height as f64 / scale).round().max(1.0) as u32;
    let url = format!(
        "index.html#/recording-start-control?monitorIndex={monitor_index}&x={x}&y={y}&width={width}&height={height}&canvasWidth={canvas_width}&canvasHeight={canvas_height}&fps={fps}&recordAudio={record_audio}&recordSystemAudio={record_system_audio}"
    );
    let control = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title("開始錄影")
        .decorations(false)
        .always_on_top(true)
        .focused(true)
        .resizable(false)
        .inner_size(680.0, 58.0)
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

        if default_dir.as_os_str().is_empty() {
            return Err("預設存檔資料夾未設定，請先在設定中選擇可寫入的資料夾".to_string());
        }
        if !default_dir.exists() {
            std::fs::create_dir_all(&default_dir)
                .map_err(|e| format!("無法建立預設存檔資料夾：{e}"))?;
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
    eprintln!(
        "[clipboard] image decoded width={} height={}",
        width, height
    );
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
            let delta = crate::platform::windows::wheel_delta_for_down_lines(lines);
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

#[cfg(test)]
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

#[cfg(test)]
fn frames_are_stable(img1: &RgbaImage, img2: &RgbaImage) -> bool {
    sampled_difference(img1, img2, 0).is_some_and(|difference| difference < 2.5)
}

#[cfg(test)]
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

#[cfg(test)]
fn find_scroll_shift_near(img1: &RgbaImage, img2: &RgbaImage, expected: u32) -> Option<u32> {
    let zero_difference = sampled_difference(img1, img2, 0)?;
    if zero_difference < 2.5 {
        return None;
    }
    let min_shift = expected.saturating_sub(48).max(1);
    let max_shift = expected
        .saturating_add(48)
        .min(img1.height().saturating_sub(25));
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

#[cfg(test)]
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
        candidates[y as usize] =
            samples > 0 && (difference as f64 / (samples * 3) as f64) < 4.0 && variance > 80.0;
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

#[cfg(test)]
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
        candidates[x as usize] =
            samples > 0 && (difference as f64 / (samples * 3) as f64) < 4.0 && variance > 80.0;
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
    if [
        "google chrome",
        "chrome",
        "brave browser",
        "microsoft edge",
        "safari",
        "firefox",
    ]
    .iter()
    .any(|name| app.contains(name))
        || title.contains("google chrome")
        || title.contains("microsoft edge")
    {
        ScrollCaptureStrategy::BrowserPage
    } else if [
        "microsoft word",
        "pages",
        "libreoffice",
        "preview",
        "acrobat",
    ]
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
    let _capture_guard = ScrollCaptureGuard::acquire()?;
    SCROLL_CANCELLED.store(false, Ordering::SeqCst);

    // The capture overlay initiated this command after hiding itself.  Wait
    // for DWM to publish that state before looking for the window underneath.
    wait_for_capture_surface_to_hide(&app, false);

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
    let selection = logical_to_physical_rect(
        mon_x,
        mon_y,
        coordinate_scale,
        selection_x,
        selection_y,
        selection_width,
        selection_height,
    );
    let global_selection_x = selection.x;
    let global_selection_y = selection.y;
    let global_selection_width = selection.width;
    let global_selection_height = selection.height;

    let windows = xcap::Window::all().map_err(|e| format!("Failed to list windows: {}", e))?;
    let window_coordinate_scale = xcap_window_coordinate_scale(scale_factor);

    // Find window under the center point of selection to prevent adjacent window mismatch
    let center_x = global_selection_x + global_selection_width as i32 / 2;
    let center_y = global_selection_y + global_selection_height as i32 / 2;

    let target_win = windows.into_iter().find(|w| {
        let title = w.title().unwrap_or_default();
        let app = w.app_name().unwrap_or_default();
        if app == "Dock"
            || app == "Window Server"
            || app == "NVIDIA app"
            || app == "GeForce Experience"
            || app == "Steam"
            || title.contains("Overlay")
            || is_unisnap_window(&app, &title)
        {
            return false;
        }
        if let (Ok(wx), Ok(wy), Ok(ww), Ok(wh), Ok(min)) =
            (w.x(), w.y(), w.width(), w.height(), w.is_minimized())
        {
            let phys_wx = (wx as f64 * window_coordinate_scale).round() as i32;
            let phys_wy = (wy as f64 * window_coordinate_scale).round() as i32;
            let phys_ww = (ww as f64 * window_coordinate_scale).round() as i32;
            let phys_wh = (wh as f64 * window_coordinate_scale).round() as i32;

            if min || phys_ww < 200 || phys_wh < 200 {
                return false;
            }
            center_x >= phys_wx
                && center_x < phys_wx + phys_ww
                && center_y >= phys_wy
                && center_y < phys_wy + phys_wh
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
    log_backend(&format!(
        "[scroll] target app={:?} title={:?} strategy={}",
        target_app, target_title, strategy_name
    ));
    if strategy != ScrollCaptureStrategy::DesktopStitch {
        log_backend("[scroll] fallback=desktop-stitch reason=content-layer-export-not-available");
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
    let phys_win_x = (win_x as f64 * window_coordinate_scale).round() as i32;
    let phys_win_y = (win_y as f64 * window_coordinate_scale).round() as i32;
    let crop_x = global_selection_x.saturating_sub(phys_win_x) as u32;
    let crop_y = global_selection_y.saturating_sub(phys_win_y) as u32;
    let crop_frame = |frame: RgbaImage| -> Result<RgbaImage, String> {
        let frame_w = frame.width();
        let frame_h = frame.height();

        // Clamp crop boundaries dynamically to fit within the captured window frame
        let clamped_x = std::cmp::min(crop_x, frame_w.saturating_sub(1));
        let clamped_y = std::cmp::min(crop_y, frame_h.saturating_sub(1));
        let clamped_w = std::cmp::min(global_selection_width, frame_w - clamped_x);
        let clamped_h = std::cmp::min(global_selection_height, frame_h - clamped_y);

        if clamped_w == 0 || clamped_h == 0 {
            return Err("選取範圍與目標視窗完全無重合空間".to_string());
        }

        Ok(
            image::imageops::crop_imm(&frame, clamped_x, clamped_y, clamped_w, clamped_h)
                .to_image(),
        )
    };
    log_backend(&format!(
        "[scroll] selection logical=({},{} {}x{}) overlay_scale={:.3} global=({},{} {}x{}) window_origin_xcap=({},{}), window_scale={:.3}, window_origin_phys=({},{}), crop=({},{} {}x{})",
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
        window_coordinate_scale,
        phys_win_x,
        phys_win_y,
        crop_x,
        crop_y,
        global_selection_width,
        global_selection_height
    ));

    let mut input = ScrollController::new()?;
    // SetCursorPos takes physical virtual-desktop pixels on Windows. The
    // selection was already converted to that coordinate system above.
    input.position_pointer(center_x, center_y)?;
    std::thread::sleep(std::time::Duration::from_millis(250));

    let first_frame = crop_frame(
        win.capture_image()
            .map_err(|e| format!("Capture frame 1 failed: {}", e))?,
    )?;
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

        let mut next_frame = crop_frame(
            win.capture_image()
                .map_err(|e| format!("Capture frame {} failed: {}", step + 1, e))?,
        )?;
        let mut settle_attempts = 0usize;
        // Do not stitch a frame while the target is still animating or
        // re-laying out lazy content.  Capture successive samples until two
        // adjacent samples are visually stable, up to a bounded timeout.
        for attempt in 1..=8 {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let candidate = crop_frame(
                win.capture_image()
                    .map_err(|e| format!("Capture settle frame {} failed: {}", step + 1, e))?,
            )?;
            settle_attempts = attempt;
            if domain_frames_are_stable(&next_frame, &candidate) {
                next_frame = candidate;
                break;
            }
            next_frame = candidate;
        }
        log_backend(&format!(
            "[scroll] step={} settle_attempts={}",
            step, settle_attempts
        ));
        if next_frame.dimensions() != previous_frame.dimensions() {
            return Err("Target window size changed during long capture".to_string());
        }

        if SCROLL_CANCELLED.load(Ordering::SeqCst) {
            was_cancelled = true;
            break;
        }

        let detected_shift = if frames.len() > 1 {
            let expected = frame_offsets[1].saturating_sub(frame_offsets[0]);
            domain_find_scroll_shift_near(&previous_frame, &next_frame, expected)
        } else {
            domain_find_scroll_shift(&previous_frame, &next_frame)
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
                domain_fixed_row_mask(&previous_frame, &next_frame),
                domain_fixed_column_mask(&previous_frame, &next_frame),
            ));
            frames.push(next_frame.clone());
            frame_offsets.push(next_offset);
            log_backend(&format!(
                "[scroll] step={} shift={} offset={} frame={}x{}",
                step, shift, next_offset, frame_width, frame_height
            ));
            total_height = next_total_height;
            previous_frame = next_frame;
        } else {
            if domain_frames_are_stable(&previous_frame, &next_frame) {
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
                // Do not present a failed long capture as a successful
                // one-frame screenshot.
                if frames.len() == 1 {
                    return Err(
                        "No scrollable content or stable scroll movement was detected".to_string(),
                    );
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

    crate::scroll_composite::compose_scroll_frames(
        &frames,
        &frame_offsets,
        &fixed_masks,
        total_height,
    )
}

#[cfg(test)]
mod capture_tests {
    use super::{
        capture_window_route, classify_scroll_target, find_scroll_shift, find_scroll_shift_near,
        fixed_column_mask, fixed_row_mask, frames_are_stable, is_unisnap_window,
        recording_control_position, work_area_crop_bounds, xcap_window_coordinate_scale,
        ScrollCaptureStrategy,
    };
    use crate::capture_geometry::CropBounds;
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
    fn capture_window_route_carries_label_and_mode_without_external_navigation() {
        assert_eq!(
            capture_window_route("capture_record_0_123", "record"),
            "index.html#/capture?label=capture_record_0_123&mode=record"
        );
    }

    #[test]
    fn own_windows_are_never_long_capture_targets() {
        assert!(is_unisnap_window("unisnap-windows.exe", "UniSnap"));
        assert!(is_unisnap_window("msedgewebview2.exe", "Capture Window 1"));
        assert!(!is_unisnap_window("chrome.exe", "購物網站 - Google Chrome"));
    }

    #[test]
    fn windows_xcap_bounds_are_not_scaled_twice_on_high_dpi_displays() {
        #[cfg(target_os = "windows")]
        assert_eq!(xcap_window_coordinate_scale(1.5), 1.0);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(xcap_window_coordinate_scale(1.5), 1.5);
    }

    #[test]
    fn recording_control_prefers_space_outside_selection() {
        assert_eq!(
            recording_control_position(200.0, 200.0, 800.0, 500.0, 1920.0, 1080.0),
            (200.0, 130.0)
        );
        assert_eq!(
            recording_control_position(40.0, 20.0, 800.0, 500.0, 1920.0, 1080.0),
            (40.0, 532.0)
        );
    }

    #[test]
    fn recording_control_stays_on_monitor_for_full_screen_selection() {
        assert_eq!(
            recording_control_position(0.0, 0.0, 1920.0, 1080.0, 1920.0, 1080.0),
            (12.0, 12.0)
        );
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
    save_and_copy_screenshot(
        app.clone(),
        base64_image,
        Some(path_str.clone()),
        config.auto_copy_to_clipboard,
    )?;
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
    save_and_copy_screenshot(
        app.clone(),
        base64_image,
        Some(path_str.clone()),
        config.auto_copy_to_clipboard,
    )?;
    Ok(path_str)
}

#[cfg(target_os = "windows")]
/// Detects if a captured frame is blank (all-white or all-black).
/// Used to identify silent capture failures where xcap/GDI succeeds
/// but returns an empty frame (common with Virtual Display Devices).
#[cfg(target_os = "windows")]
fn is_blank_frame(img: &image::RgbaImage) -> bool {
    let total_pixels = img.width() as usize * img.height() as usize;
    if total_pixels == 0 {
        return true;
    }
    // Sample up to 200 pixels evenly across the image
    let step = (total_pixels / 200).max(1);
    let pixels = img.as_raw();
    let mut white_count = 0usize;
    let mut black_count = 0usize;
    let mut sampled = 0usize;
    let mut i = 0usize;
    while i + 3 < pixels.len() {
        let r = pixels[i];
        let g = pixels[i + 1];
        let b = pixels[i + 2];
        if r > 250 && g > 250 && b > 250 {
            white_count += 1;
        }
        if r < 5 && g < 5 && b < 5 {
            black_count += 1;
        }
        sampled += 1;
        i += step * 4;
    }
    // If >95% of sampled pixels are the same extreme color, it's a blank frame
    sampled > 0 && (white_count * 100 / sampled > 95 || black_count * 100 / sampled > 95)
}

pub fn capture_monitor_gdi(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<image::RgbaImage, String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDCW, DeleteDC, DeleteObject,
        GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HGDIOBJ, RGBQUAD, SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

    unsafe {
        let desktop_hwnd = GetDesktopWindow();
        let mut hdc_screen = GetDC(desktop_hwnd);
        if hdc_screen.is_null() {
            hdc_screen = GetDC(0 as HWND);
        }
        if hdc_screen.is_null() {
            let display_name: Vec<u16> = "DISPLAY\0".encode_utf16().collect();
            hdc_screen = CreateDCW(
                display_name.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            );
        }
        if hdc_screen.is_null() {
            return Err("Failed to obtain screen DC".to_string());
        }

        let hdc_mem = CreateCompatibleDC(hdc_screen);
        if hdc_mem.is_null() {
            ReleaseDC(desktop_hwnd, hdc_screen);
            return Err("CreateCompatibleDC failed".to_string());
        }

        let w = if width == 0 { 1920 } else { width };
        let h = if height == 0 { 1080 } else { height };

        let hbmp = CreateCompatibleBitmap(hdc_screen, w as i32, h as i32);
        if hbmp.is_null() {
            DeleteDC(hdc_mem);
            ReleaseDC(desktop_hwnd, hdc_screen);
            return Err("CreateCompatibleBitmap failed".to_string());
        }

        let old_bmp = SelectObject(hdc_mem, hbmp as HGDIOBJ);

        let bit_blt_res = BitBlt(hdc_mem, 0, 0, w as i32, h as i32, hdc_screen, x, y, SRCCOPY);

        if bit_blt_res == 0 {
            SelectObject(hdc_mem, old_bmp);
            DeleteObject(hbmp as HGDIOBJ);
            DeleteDC(hdc_mem);
            ReleaseDC(desktop_hwnd, hdc_screen);
            return Err("BitBlt failed".to_string());
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB as u32,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }; 1],
        };

        let mut bgra_buf: Vec<u8> = vec![0; (w * h * 4) as usize];
        let get_bits_res = GetDIBits(
            hdc_mem,
            hbmp,
            0,
            h,
            bgra_buf.as_mut_ptr() as _,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, old_bmp);
        DeleteObject(hbmp as HGDIOBJ);
        DeleteDC(hdc_mem);
        ReleaseDC(desktop_hwnd, hdc_screen);

        if get_bits_res == 0 {
            return Err("GetDIBits failed".to_string());
        }

        let mut rgba_buf = vec![0u8; bgra_buf.len()];
        for i in (0..bgra_buf.len()).step_by(4) {
            let b = bgra_buf[i];
            let g = bgra_buf[i + 1];
            let r = bgra_buf[i + 2];
            let a = bgra_buf[i + 3];
            rgba_buf[i] = r;
            rgba_buf[i + 1] = g;
            rgba_buf[i + 2] = b;
            rgba_buf[i + 3] = if a == 0 { 255 } else { a };
        }

        image::RgbaImage::from_raw(w, h, rgba_buf)
            .ok_or_else(|| "Failed to construct RgbaImage from GDI buffer".to_string())
    }
}
