use std::fs;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

use mp4::Bytes;
use openh264::encoder::{
    FrameType,
};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
pub use crate::record_types::RecordingState;
use crate::record_types::{RecordingResult, RecordingSession, ScrollConfig};
use crate::audio_capture::{start_audio_capture, AudioCapture};
use crate::frame_source::{CaptureRegion, FrameSource};
use crate::recording_crop::{crop_rgba, resolve_crop, Crop};
use crate::recording_output::recording_output_path;
use crate::recording_audio::audio_track_info;
use crate::h264_sample::extract_h264_sample;
use crate::recording_tracks::add_recording_tracks;
use crate::recording_encoder_init::{create_h264_encoder, create_mp4_writer};
use crate::recording_finalize::finalize_mp4;
use crate::recording_timing::next_timed_frame;
use crate::recording_audio_writer::write_microphone_track;
use crate::recording_video_writer::{write_video_sample, PendingVideoSample};
use crate::scroll_trigger::trigger_scroll_capture as trigger_scroll_capture_impl;
use tauri::AppHandle;
use xcap::{Frame, Monitor};

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn sck_start_recording(
        display_index: usize,
        x: f64, y: f64, width: f64, height: f64,
        canvas_width: f64, canvas_height: f64,
        fps: u32,
        capture_microphone: bool,
        capture_system_audio: bool,
        output_path: *const std::ffi::c_char,
        error_buffer: *mut std::ffi::c_char,
        error_buffer_size: usize,
    ) -> bool;
    fn sck_stop_recording(
        error_buffer: *mut std::ffi::c_char,
        error_buffer_size: usize,
    ) -> bool;
}

#[cfg(target_os = "macos")]
fn start_system_recording(
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
    let path = recording_output_path(app)?;
    let path_c = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| "錄影路徑含有無效字元".to_string())?;
    let mut error_buffer = vec![0_i8; 512];
    let started = unsafe {
        sck_start_recording(
            monitor_index, x as f64, y as f64, width as f64, height as f64,
            canvas_width as f64, canvas_height as f64,
            fps.clamp(1, 60), record_audio, record_system_audio, path_c.as_ptr(),
            error_buffer.as_mut_ptr(), error_buffer.len(),
        )
    };
    if !started {
        let message = unsafe { std::ffi::CStr::from_ptr(error_buffer.as_ptr()) }
            .to_string_lossy().into_owned();
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
                .to_string_lossy().into_owned();
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
    let _ = crate::capture::open_recording_control(
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

#[tauri::command]
#[allow(unreachable_code)]
pub fn start_recording(
    app: AppHandle,
    state: tauri::State<'_, RecordingState>,
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
    eprintln!(
        "[record] start_recording monitor={} rect=({}, {}, {}, {}) canvas={}x{} fps={} audio={}",
        monitor_index, x, y, width, height, canvas_width, canvas_height, fps, record_audio
    );
    #[cfg(target_os = "macos")]
    {
        return start_system_recording(
            &app,
            &state,
            monitor_index,
            x,
            y,
            width,
            height,
            canvas_width,
            canvas_height,
            record_audio,
            record_system_audio,
            fps,
        );
    }
    let mut active = state
        .inner()
        .0
        .lock()
        .map_err(|_| "無法鎖定錄影狀態".to_string())?;
    if active.is_some() {
        return Err("已有錄影正在進行中".into());
    }
    let tauri_monitors = app
        .available_monitors()
        .map_err(|e| format!("無法列出系統螢幕：{e}"))?;
    let selected_monitor = tauri_monitors
        .get(monitor_index)
        .ok_or_else(|| format!("找不到第 {} 個螢幕", monitor_index + 1))?;
    let desired_x = selected_monitor.position().x;
    let desired_y = selected_monitor.position().y;
    let desired_name = selected_monitor.name().cloned().unwrap_or_default();
    let coordinate_scale = selected_monitor.scale_factor();

    let output_path = recording_output_path(&app)?;
    let audio = if record_audio {
        Some(start_audio_capture()?)
    } else {
        None
    };
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (finished_tx, finished_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let worker_path = output_path.clone();
    let safe_fps = fps.clamp(1, 60);
    std::thread::spawn(move || {
        let result = (|| {
            let monitors = Monitor::all().map_err(|e| format!("無法列出錄影螢幕：{e}"))?;
            let monitor = monitors
                .iter()
                .find(|monitor| {
                    monitor.x().ok() == Some(desired_x) && monitor.y().ok() == Some(desired_y)
                })
                .or_else(|| {
                    monitors
                        .iter()
                        .find(|monitor| monitor.name().unwrap_or_default() == desired_name)
                })
                .or_else(|| monitors.get(monitor_index))
                .cloned()
                .ok_or_else(|| format!("找不到第 {} 個螢幕", monitor_index + 1))?;
            let first_image = monitor
                .capture_image()
                .map_err(|e| format!("啟動後無法取得第一個螢幕影格：{e}"))?;
            let first_frame = Frame::new(
                first_image.width(),
                first_image.height(),
                first_image.into_raw(),
            );
            let crop = resolve_crop(&first_frame, coordinate_scale, x, y, width, height)?;
            let (recorder, receiver) = monitor
                .video_recorder()
                .map_err(|e| format!("無法啟動原生螢幕錄影器：{e}"))?;
            recorder
                .start()
                .map_err(|e| format!("無法開始原生螢幕錄影器：{e}"))?;
            let started = RecordingResult {
                path: worker_path.to_string_lossy().into_owned(),
                frame_count: 0,
                width: crop.width,
                height: crop.height,
            };
            let _ = ready_tx.send(Ok(started));
            let encoded = encode_recording(
                first_frame,
                FrameSource::Native { recorder, receiver },
                crop,
                safe_fps,
                worker_path,
                worker_stop,
                audio,
            );
            encoded
        })();
        if let Err(error) = &result {
            let _ = ready_tx.send(Err(error.clone()));
        }
        let _ = finished_tx.send(result);
    });
    let started = ready_rx
        .recv_timeout(Duration::from_secs(6))
        .map_err(|e| format!("等待原生錄影啟動逾時：{e}"))??;
    eprintln!(
        "[record] start_recording ready path={} size={}x{}",
        started.path, started.width, started.height
    );
    if let Err(error) = crate::capture::open_recording_control(
        app.clone(),
        monitor_index,
        x as f64,
        y as f64,
        width as f64,
        height as f64,
        safe_fps,
        false,
    ) {
        eprintln!("[record] control setup failed: {error}");
        stop.store(true, Ordering::SeqCst);
        let _ = finished_rx.recv_timeout(Duration::from_secs(30));
        return Err(format!("錄影控制列啟動失敗：{error}"));
    }
    *active = Some(RecordingSession {
        stop,
        finished: finished_rx,
    });
    Ok(started)
}

#[tauri::command]
pub fn stop_recording(state: tauri::State<'_, RecordingState>) -> Result<RecordingResult, String> {
    eprintln!("[record] stop_recording requested");
    let session = state
        .inner()
        .0
        .lock()
        .map_err(|_| "無法鎖定錄影狀態".to_string())?
        .take()
        .ok_or_else(|| "目前沒有進行中的錄影".to_string())?;
    session.stop.store(true, Ordering::SeqCst);
    let result = session
        .finished
        .recv_timeout(Duration::from_secs(30))
        .map_err(|e| format!("等待 MP4 存檔逾時：{e}"))?;
    eprintln!(
        "[record] stop_recording finished result={:?}",
        result
            .as_ref()
            .map(|r| (&r.path, r.frame_count))
            .map_err(|e| e)
    );
    result
}

fn encode_recording(
    first: Frame,
    frame_source: FrameSource,
    crop: Crop,
    fps: u32,
    path: PathBuf,
    stop: Arc<AtomicBool>,
    audio: Option<AudioCapture>,
) -> Result<RecordingResult, String> {
    let mut encoder = create_h264_encoder(fps, crop)?;
    let mut writer = create_mp4_writer(&path)?;
    let audio_info = match audio_track_info(audio.as_ref()) {
        Ok(info) => info,
        Err(error) => {
            let _ = fs::remove_file(&path);
            return Err(error);
        }
    };
    let frame_duration = 1_000 / fps;
    let interval = Duration::from_secs_f64(1.0 / fps as f64);
    let recording_started_at = Instant::now();
    let mut next_frame_at = Instant::now();
    let mut pending = Some(first);
    let mut pending_sample: Option<PendingVideoSample> = None;
    let mut tracks_added = false;
    let mut frame_count = 0u64;

    loop {
        let Some(frame) = next_timed_frame(
            &frame_source,
            &mut pending,
            &stop,
            &mut next_frame_at,
            interval,
        )? else {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            continue;
        };
        let rgba = crop_rgba(&frame, crop)?;
        let yuv = YUVBuffer::from_rgb_source(RgbaSliceU8::new(
            &rgba,
            (crop.width as usize, crop.height as usize),
        ));
        let encoded = encoder
            .encode(&yuv)
            .map_err(|e| format!("H.264 編碼失敗：{e}"))?;
        let encoded_sample = extract_h264_sample(&encoded)?;
        let frame_type = encoded_sample.frame_type;
        if matches!(frame_type, FrameType::Skip | FrameType::Invalid) {
            continue;
        }
        if !tracks_added {
            add_recording_tracks(
                &mut writer,
                crop.width,
                crop.height,
                encoded_sample.sps.ok_or("第一個 H.264 影格缺少 SPS")?,
                encoded_sample.pps.ok_or("第一個 H.264 影格缺少 PPS")?,
                audio_info.as_ref(),
            )?;
            tracks_added = true;
        }
        if !encoded_sample.bytes.is_empty() {
            let timestamp = if pending_sample.is_none() {
                0
            } else {
                recording_started_at.elapsed().as_millis() as u64
            };
            if let Some((previous_timestamp, previous_sync, previous_bytes)) = pending_sample.take()
            {
                let duration = timestamp
                    .saturating_sub(previous_timestamp)
                    .clamp(1, u32::MAX as u64) as u32;
                write_video_sample(
                    &mut writer,
                    (previous_timestamp, previous_sync, previous_bytes),
                    duration,
                    false,
                )?;
                frame_count += 1;
            }
            pending_sample = Some((
                timestamp,
                matches!(frame_type, FrameType::IDR | FrameType::I),
                Bytes::from(encoded_sample.bytes),
            ));
        }
    }
    if let Some((timestamp, is_sync, bytes)) = pending_sample.take() {
        let stopped_at = recording_started_at.elapsed().as_millis() as u64;
        let duration = stopped_at
            .saturating_sub(timestamp)
            .max(frame_duration as u64)
            .clamp(1, u32::MAX as u64) as u32;
        write_video_sample(&mut writer, (timestamp, is_sync, bytes), duration, true)?;
        frame_count += 1;
    }
    if frame_count == 0 {
        let _ = fs::remove_file(&path);
        return Err("錄影期間未產生可儲存影格".into());
    }
    if let Some(capture) = audio {
        write_microphone_track(&mut writer, capture)?;
    }
    finalize_mp4(writer, &path)?;
    Ok(RecordingResult {
        path: path.to_string_lossy().into_owned(),
        frame_count,
        width: crop.width,
        height: crop.height,
    })
}

#[tauri::command]
pub fn trigger_scroll_capture(config: ScrollConfig) -> Result<String, String> {
    trigger_scroll_capture_impl(config)
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    use std::fs::File;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn writes_readable_h264_mp4() {
        let path = std::env::temp_dir().join(format!(
            "screenshot-recording-test-{}.mp4",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let width = 320;
        let height = 240;
        let raw = (0..height)
            .flat_map(|y| {
                (0..width).flat_map(move |x| {
                    [
                        ((x * 3) % 255) as u8,
                        ((y * 5) % 255) as u8,
                        ((x + y) % 255) as u8,
                        255,
                    ]
                })
            })
            .collect();
        let first = Frame::new(width, height, raw);
        let (_tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(true));
        let result = encode_recording(
            first,
            FrameSource::Receiver(rx),
            Crop {
                x: 0,
                y: 0,
                width,
                height,
            },
            30,
            path.clone(),
            stop,
            None,
        )
        .unwrap();
        assert_eq!(result.frame_count, 1);
        let file = File::open(&path).unwrap();
        let size = file.metadata().unwrap().len();
        let reader = mp4::Mp4Reader::read_header(file, size).unwrap();
        assert_eq!(reader.tracks().len(), 1);
        assert_eq!(reader.sample_count(1).unwrap(), 1);
        fs::remove_file(path).unwrap();
    }

    #[test]
    #[ignore = "requires local screen-recording permission and a connected monitor"]
    fn records_real_monitor_frames_to_readable_mp4() {
        for (index, monitor) in Monitor::all().unwrap().into_iter().enumerate() {
            let image = monitor.capture_image().unwrap();
            let first = Frame::new(image.width(), image.height(), image.into_raw());
            let width = first.width.min(640) & !1;
            let height = first.height.min(360) & !1;
            let scale = monitor.scale_factor().unwrap_or(1.0).max(1.0) as f64;
            let path =
                std::env::temp_dir().join(format!("screenshot-real-recording-probe-{index}.mp4"));
            let stop = Arc::new(AtomicBool::new(false));
            let timed_stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(10));
                timed_stop.store(true, Ordering::SeqCst);
            });
            let result = encode_recording(
                first,
                FrameSource::MonitorRegion {
                    monitor,
                    region: CaptureRegion {
                        x: 0,
                        y: 0,
                        width: ((width as f64) / scale).round() as u32,
                        height: ((height as f64) / scale).round() as u32,
                        canvas_width: width,
                        canvas_height: height,
                    },
                },
                Crop {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                15,
                path.clone(),
                stop,
                None,
            )
            .unwrap();
            assert!(
                result.frame_count >= 10,
                "monitor {index} encoded only {} frames",
                result.frame_count
            );
            let file = File::open(&path).unwrap();
            let size = file.metadata().unwrap().len();
            let reader = mp4::Mp4Reader::read_header(file, size).unwrap();
            assert_eq!(reader.tracks().len(), 1);
            assert_eq!(reader.sample_count(1).unwrap() as u64, result.frame_count);
            assert!(
                reader.duration() >= Duration::from_secs(9),
                "monitor {index} MP4 duration was only {:?}",
                reader.duration()
            );
            fs::remove_file(path).unwrap();
        }
    }
}
