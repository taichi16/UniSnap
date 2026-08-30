mod audio_capture;
mod capture;
mod capture_controls;
mod capture_geometry;
mod capture_io;
mod capture_modes;
mod capture_output;
mod capture_overlay;
mod capture_types;
mod config;
mod editor_image;
mod editor_windows;
mod frame_source;
mod h264_sample;
mod image_data;
mod monitor_capture;
mod monitor_resolution;
mod mp4_config;
mod ocr;
mod pin_windows;
mod quick_access;
mod record;
mod record_types;
mod recording_audio;
mod recording_audio_writer;
mod recording_crop;
mod recording_encoder_init;
mod recording_finalize;
mod recording_output;
mod record_session;
mod recording_timing;
mod recording_tracks;
mod recording_video_writer;
mod scroll_composite;
mod scroll_capture_helpers;
mod scroll_input;
mod scroll_masks;
mod scroll_matching;
mod scroll_metrics;
mod scroll_session;
mod scroll_target;
mod scroll_target_window;
mod scroll_trigger;
mod system_recording;

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub use pin_windows::PinnedImageState;

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
                        let state = handle.state::<pin_windows::PinnedImageState>();
                        let _ = crate::capture_overlay::trigger_screenshot(
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
                        let state = handle.state::<pin_windows::PinnedImageState>();
                        let _ = crate::capture_overlay::trigger_screenshot(
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(pin_windows::PinnedImageState(Default::default()))
        .manage(quick_access::QuickAccessState(Mutex::new(Default::default())))
        .manage(record::RecordingState(Mutex::new(None)))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings = config::load_config(app.handle().clone())
                .map_err(|e| format!("載入快捷鍵設定失敗：{e}"))?;
            apply_global_shortcuts(app.handle(), &settings).map_err(|e| e.to_string())?;

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
                            let state = app_handle.state::<crate::pin_windows::PinnedImageState>();
                            let _ = crate::capture_overlay::trigger_screenshot(
                                app_handle.clone(),
                                state.clone(),
                                None,
                                None,
                            );
                        });
                    }
                    "open_image" => {
                        let _ = crate::editor_windows::open_image_in_main_editor(app.clone());
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
            monitor_capture::list_monitors,
            monitor_capture::capture_screens,
            capture_overlay::trigger_screenshot,
            editor_windows::open_image_editor,
            editor_windows::open_image_editor_data,
            editor_windows::open_image_in_main_editor,
            capture_controls::close_capture_windows,
            capture_controls::open_recording_control,
            capture_controls::open_recording_start_control,
            capture_io::save_and_copy_screenshot,
            capture_io::copy_screenshot_to_clipboard,
            capture_io::inspect_saved_file,
            capture_io::capture_screen_region,
            ocr::recognize_text_vision,
            capture_modes::capture_full_screen,
            capture_modes::capture_work_area,
            config::load_config,
            config::save_config,
            record::start_recording,
            record::stop_recording,
            record::trigger_scroll_capture,
            capture::auto_scroll_capture_window,
            capture::cancel_scroll_capture,
            pin_windows::pin_screenshot,
            pin_windows::get_pinned_image,
            pin_windows::unpin_screenshot,
            quick_access::open_quick_access,
            quick_access::get_quick_access,
            quick_access::close_quick_access
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
