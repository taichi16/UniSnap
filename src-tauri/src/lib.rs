mod capture;
mod capture_geometry;
mod config;
mod frame_source;
mod h264_sample;
mod image_data;
mod platform;
mod record;
mod recording_audio;
mod recording_crop;
mod recording_mp4;
mod recording_output;
mod recording_session;
mod recording_system_audio;
mod recording_timing;
mod scroll_composite;
mod scroll_masks;
mod scroll_matching;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

// State to share base64 images with dynamically created pin windows
pub struct PinnedImageState(pub Mutex<HashMap<String, String>>);
pub struct ExitRequested(pub AtomicBool);

/// Register shortcuts at the native application layer. This deliberately does
/// not depend on a particular WebView being focused or even mounted.
pub(crate) fn apply_global_shortcuts(
    app: &AppHandle,
    settings: &config::AppConfig,
) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();

    let screenshot = settings.shortcut_screenshot.trim().to_string();
    let recording = settings.shortcut_recording.trim().to_string();
    if !screenshot.is_empty() && screenshot.eq_ignore_ascii_case(&recording) {
        return Err("Screenshot and recording shortcuts must be different".to_string());
    }
    if !screenshot.is_empty() {
        if let Err(e) = shortcuts.on_shortcut(screenshot.as_str(), |app, _, event| {
            if event.state == ShortcutState::Pressed {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::capture::trigger_screenshot(handle.clone(), None, None);
                });
            }
        }) {
            eprintln!(
                "[shortcut] 警告：無法註冊截圖快捷鍵 '{}' (可能已被其他程式佔用)：{e}",
                screenshot
            );
            return Err(format!(
                "Could not register screenshot shortcut '{screenshot}': {e}"
            ));
        }
    }

    if !recording.is_empty() {
        if let Err(e) = shortcuts.on_shortcut(recording.as_str(), |app, _, event| {
            if event.state == ShortcutState::Pressed {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::capture::trigger_screenshot(
                        handle.clone(),
                        Some("record".to_string()),
                        None,
                    );
                });
            }
        }) {
            eprintln!(
                "[shortcut] 警告：無法註冊錄影快捷鍵 '{}' (可能已被其他程式佔用)：{e}",
                recording
            );
            return Err(format!(
                "Could not register recording shortcut '{recording}': {e}"
            ));
        }
    }

    // Escape is intentionally kept as a native route because the capture
    // window may be hidden during an automatic scroll operation.
    if let Err(e) = shortcuts.on_shortcut("Escape", |app, _, event| {
        if event.state == ShortcutState::Pressed {
            let _ = app.emit("global-escape", ());
            let state = app.state::<PinnedImageState>();
            let _ = crate::capture::close_capture_windows(app.clone(), state);
        }
    }) {
        eprintln!("[shortcut] 警告：無法註冊 Escape 快捷鍵：{e}");
    }

    println!(
        "[shortcut] registered screenshot='{}' recording='{}' escape='Escape'",
        settings.shortcut_screenshot, settings.shortcut_recording
    );
    Ok(())
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn log_from_frontend(level: String, message: String) {
    let log_line = format!("[Frontend {}] {}\n", level, message);
    print!("{}", log_line);

    use std::fs::OpenOptions;
    use std::io::Write;
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("frontend.log")
    {
        let _ = file.write_all(log_line.as_bytes());
    }
}

#[tauri::command]
fn pin_screenshot(
    app: AppHandle,
    state: tauri::State<'_, PinnedImageState>,
    image_base64: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<String, String> {
    // Generate a unique label using timestamp and a counter
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let label = format!("pin_{}", timestamp);

    // Store the image in global state
    {
        let mut map = state.0.lock().unwrap();
        map.insert(label.clone(), image_base64);
    }

    // Determine the HTML URL path (loads the main index.html with a query param)
    // The React router or hash routing will load the Pin component
    let window_url = WebviewUrl::App("index.html".into());

    // Create a new borderless, always-on-top, draggable window
    let win_builder = WebviewWindowBuilder::new(&app, &label, window_url)
        .title("Pinned Screenshot")
        .decorations(false)
        .always_on_top(true)
        .transparent(true)
        .resizable(true)
        .inner_size(width as f64, height as f64)
        .position(x as f64, y as f64);

    win_builder
        .build()
        .map_err(|e| format!("Failed to build pinned window: {}", e))?;

    Ok(label)
}

#[tauri::command]
fn get_pinned_image(
    state: tauri::State<'_, PinnedImageState>,
    label: String,
) -> Result<String, String> {
    let map = state.0.lock().unwrap();
    map.get(&label)
        .cloned()
        .ok_or_else(|| "Image not found".to_string())
}

#[tauri::command]
fn unpin_screenshot(
    app: AppHandle,
    state: tauri::State<'_, PinnedImageState>,
    label: String,
) -> Result<(), String> {
    // Remove image from state
    {
        let mut map = state.0.lock().unwrap();
        map.remove(&label);
    }

    // Close the corresponding window if it exists
    if let Some(window) = app.get_webview_window(&label) {
        window
            .close()
            .map_err(|e| format!("Failed to close window: {}", e))?;
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(PinnedImageState(Mutex::new(HashMap::new())))
        .manage(ExitRequested(AtomicBool::new(false)))
        .manage(record::RecordingState::new())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings = config::load_config(app.handle().clone())
                .map_err(|e| format!("載入快捷鍵設定失敗：{e}"))?;
            if let Err(error) = apply_global_shortcuts(app.handle(), &settings) {
                // A stale shortcut may be owned by another process. Keep the
                // app usable so the user can choose a new key in Settings.
                eprintln!("[shortcut] startup registration failed: {error}");
            }

            // Setup Tray Icon & Menu
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::TrayIconBuilder;

            let screenshot_i = MenuItemBuilder::with_id("screenshot", "螢幕截圖").build(app)?;
            let open_image_i = MenuItemBuilder::with_id("open_image", "開啟舊檔…").build(app)?;
            let show_settings_i = MenuItemBuilder::with_id("show_settings", "設定").build(app)?;
            let quit_i = MenuItemBuilder::with_id("quit", "結束").build(app)?;

            let menu = MenuBuilder::new(app)
                .items(&[&open_image_i, &screenshot_i, &show_settings_i, &quit_i])
                .build()?;

            if let Some(main) = app.get_webview_window("main") {
                let close_to_tray = settings.close_to_tray;
                let main_for_close = main.clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        if close_to_tray {
                            api.prevent_close();
                            let _ = main_for_close.hide();
                        }
                    }
                });
            }

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap_or_else(|| {
                    // Fallback empty image
                    tauri::image::Image::new(&[], 0, 0)
                }))
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "screenshot" => {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ =
                                crate::capture::trigger_screenshot(app_handle.clone(), None, None);
                        });
                    }
                    "open_image" => {
                        let _ = crate::capture::open_image_in_main_editor(app.clone());
                    }
                    "show_settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.state::<ExitRequested>().0.store(true, Ordering::SeqCst);
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            log_from_frontend,
            capture::list_monitors,
            capture::capture_screens,
            capture::trigger_screenshot,
            capture::open_image_editor,
            capture::open_image_editor_data,
            capture::open_image_in_main_editor,
            capture::close_capture_windows,
            capture::open_recording_control,
            capture::open_recording_start_control,
            capture::save_and_copy_screenshot,
            capture::copy_screenshot_to_clipboard,
            capture::inspect_saved_file,
            capture::capture_screen_region,
            capture::capture_full_screen,
            capture::capture_work_area,
            config::load_config,
            config::save_config,
            record::start_recording,
            record::stop_recording,
            record::trigger_scroll_capture,
            capture::auto_scroll_capture_window,
            capture::cancel_scroll_capture,
            pin_screenshot,
            get_pinned_image,
            unpin_screenshot
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
