#![cfg(target_os = "macos")]

use std::fs;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

use tauri::AppHandle;

use crate::record_types::RecordingState;
use crate::record_types::{RecordingResult, RecordingSession};
use crate::recording_output::recording_output_path;

unsafe extern "C" {
    fn sck_start_recording(
        display_index: usize,
        monitor_x: f64,
        monitor_y: f64,
        monitor_width: f64,
        monitor_height: f64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        canvas_width: f64,
        canvas_height: f64,
        fps: u32,
        capture_microphone: bool,
        capture_system_audio: bool,
        output_path: *const std::ffi::c_char,
        error_buffer: *mut std::ffi::c_char,
        error_buffer_size: usize,
    ) -> bool;
    fn sck_stop_recording(error_buffer: *mut std::ffi::c_char, error_buffer_size: usize) -> bool;
}

#[allow(clippy::too_many_arguments)]
pub fn start_system_recording(
    app: &AppHandle,
    state: &tauri::State<'_, RecordingState>,
    monitor_index: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    canvas_width: u32,
    canvas_height: u32,
    record_audio: bool,
    record_system_audio: bool,
    fps: u32,
) -> Result<RecordingResult, String> {
    let mut active = state
        .inner()
        .0
        .lock()
        .map_err(|_| "無法鎖定錄影狀態".to_string())?;
    if active.is_some() {
        return Err("已有錄影正在進行中".into());
    }
    let monitors = app
        .available_monitors()
        .map_err(|e| format!("無法列出系統螢幕：{e}"))?;
    let _monitor = monitors
        .get(monitor_index)
        .ok_or_else(|| format!("找不到第 {} 個螢幕", monitor_index + 1))?;
    let monitor_x = _monitor.position().x as f64;
    let monitor_y = _monitor.position().y as f64;
    let monitor_width = _monitor.size().width as f64;
    let monitor_height = _monitor.size().height as f64;
    let path = recording_output_path(app)?;
    let path_c = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| "錄影路徑含有無效字元".to_string())?;
    let mut error_buffer = vec![0_i8; 512];
    let started = unsafe {
        sck_start_recording(
            monitor_index,
            monitor_x,
            monitor_y,
            monitor_width,
            monitor_height,
            x as f64,
            y as f64,
            width as f64,
            height as f64,
            canvas_width as f64,
            canvas_height as f64,
            fps.clamp(1, 60),
            record_audio,
            record_system_audio,
            path_c.as_ptr(),
            error_buffer.as_mut_ptr(),
            error_buffer.len(),
        )
    };
    if !started {
        let message = unsafe { std::ffi::CStr::from_ptr(error_buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        return Err(format!("啟動 ScreenCaptureKit 錄影失敗：{message}"));
    }
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (finished_tx, finished_rx) = mpsc::channel();
    let worker_path = path.clone();
    std::thread::spawn(move || {
        while !worker_stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut stop_error = vec![0_i8; 512];
        let stopped = unsafe { sck_stop_recording(stop_error.as_mut_ptr(), stop_error.len()) };
        if !stopped {
            let message = unsafe { std::ffi::CStr::from_ptr(stop_error.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            let _ = finished_tx.send(Err(format!("停止 ScreenCaptureKit 錄影失敗：{message}")));
            return;
        }
        let result = fs::metadata(&worker_path)
            .map_err(|e| format!("macOS 錄影檔案不存在：{e}"))
            .and_then(|meta| {
                if meta.len() == 0 {
                    Err("macOS 錄影檔案為空".into())
                } else {
                    Ok(RecordingResult {
                        path: worker_path.to_string_lossy().into_owned(),
                        frame_count: 0,
                        width,
                        height,
                    })
                }
            });
        let _ = finished_tx.send(result);
    });
    *active = Some(RecordingSession {
        stop,
        finished: finished_rx,
    });
    let _ = crate::capture_controls::open_recording_control(
        app.clone(),
        monitor_index,
        x as f64,
        y as f64,
        width as f64,
        height as f64,
        fps,
        record_audio,
    );
    eprintln!(
        "[record] macOS system recording ready path={}",
        path.display()
    );
    Ok(RecordingResult {
        path: path.to_string_lossy().into_owned(),
        frame_count: 0,
        width,
        height,
    })
}
