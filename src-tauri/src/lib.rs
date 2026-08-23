mod capture;
mod capture_types;
mod audio_capture;
mod config;
mod editor_image;
mod frame_source;
mod image_data;
mod record;
mod recording_crop;
mod recording_output;
mod record_types;
mod scroll_matching;
mod scroll_target;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

// State to share base64 images with dynamically created pin windows
pub struct PinnedImageState(pub Mutex<HashMap<String, String>>);

/// Register shortcuts at the native application layer. This deliberately does
/// not depend on a particular WebView being focused or even mounted.
pub(crate) fn apply_global_shortcuts(
    app: &AppHandle,
    settings: &config::AppConfig,
) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    shortcuts
        .unregister_all()
        .map_err(|e| format!("清除舊快捷鍵失敗：{e}"))?;

    let screenshot = settings.shortcut_screenshot.trim().to_string();
    if !screenshot.is_empty() {
        shortcuts
            .on_shortcut(screenshot.as_str(), |app, _, event| {
                if event.state == ShortcutState::Pressed {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = handle.state::<PinnedImageState>();
                        let _ = crate::capture::trigger_screenshot(
                            handle.clone(),
                            state.clone(),
                            None,
                            None,
                        );
                    });
                }
            })
            .map_err(|e| format!("註冊截圖快捷鍵失敗：{e}"))?;
    }

    let recording = settings.shortcut_recording.trim().to_string();
    if !recording.is_empty() {
        shortcuts
            .on_shortcut(recording.as_str(), |app, _, event| {
                if event.state == ShortcutState::Pressed {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = handle.state::<PinnedImageState>();
                        let _ = crate::capture::trigger_screenshot(
                            handle.clone(),
                            state.clone(),
                            Some("record".to_string()),
                            None,
                        );
                    });
                }
            })
            .map_err(|e| format!("註冊錄影快捷鍵失敗：{e}"))?;
    }

    // Escape is intentionally kept as a native route because the capture
    // window may be hidden during an automatic scroll operation.
    shortcuts
        .on_shortcut("Escape", |app, _, event| {
            if event.state == ShortcutState::Pressed {
                let _ = app.emit("global-escape", ());
            }
        })
        .map_err(|e| format!("註冊 Escape 快捷鍵失敗：{e}"))?;

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
    let window_url = WebviewUrl::App(format!("index.html#/pin?label={}", label).parse().unwrap());

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
        .manage(record::RecordingState(Mutex::new(None)))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings = config::load_config(app.handle().clone())
                .map_err(|e| format!("載入快捷鍵設定失敗：{e}"))?;
            apply_global_shortcuts(app.handle(), &settings)
                .map_err(|e| format!("{e}"))?;

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
                            let state = app_handle.state::<crate::PinnedImageState>();
                            let _ = crate::capture::trigger_screenshot(
                                app_handle.clone(),
                                state.clone(),
                                None,
                                None,
                            );
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
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
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
