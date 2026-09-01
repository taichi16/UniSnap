use crate::capture::log_backend;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

use crate::frame_source::FrameSource;
use crate::h264_sample::{extract_h264_sample, H264Sample};
use crate::platform::windows::{validate_recording_fps, validate_recording_options};
use crate::recording_crop::{crop_rgba, crop_rgba_into, resolve_crop_from_canvas, Crop};
use crate::recording_mp4::{add_audio_track, add_video_track, write_sample as write_mp4_sample};
use crate::recording_output::recording_output_path;
use crate::recording_session::{RecordingPoll, RecordingSession, RecordingState as SessionState};
use crate::recording_timing::{elapsed_sample_duration, RecordingClock};
use mp4::{Bytes, ChannelConfig, FourCC, Mp4Config, Mp4Writer, SampleFreqIndex, TrackType};
use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, UsageType,
};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
use openh264::OpenH264API;
use tauri::AppHandle;
#[cfg(target_os = "macos")]
use tauri::Manager;
use xcap::{Frame, Monitor};

use crate::recording_audio::{
    encode_aac, mix_audio_samples, normalize_recording_audio, start_audio_capture, AudioCapture,
    AudioSamples,
};
use crate::recording_system_audio::{start_system_audio_capture, SystemAudioCapture};

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn sck_start_recording(
        display_index: usize,
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

pub type RecordingState = SessionState<RecordingResult>;

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
    if state.inner().is_active()? {
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
            monitor_index,
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
    state.inner().install(RecordingSession {
        stop,
        finished: finished_rx,
    })?;
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

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    pub path: String,
    pub frame_count: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub phase: String,
    pub result: Option<RecordingResult>,
    pub error: Option<String>,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CGEventCreateMouseEvent(
        source: *mut std::ffi::c_void,
        mouse_type: u32,
        position: CGPoint,
        button: u32,
    ) -> *mut std::ffi::c_void;
    fn CGEventCreateScrollWheelEvent2(
        source: *mut std::ffi::c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> *mut std::ffi::c_void;
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
    fn CFRelease(cf: *mut std::ffi::c_void);
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct CGPoint {
    x: f64,
    y: f64,
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
    microphone_device_id: Option<String>,
    fps: u32,
) -> Result<RecordingResult, String> {
    validate_recording_options(record_audio, record_system_audio)?;
    validate_recording_fps(fps)?;
    log_backend(&format!(
        "[record] start_recording monitor={} rect=({}, {}, {}, {}) canvas={}x{} fps={} microphone={} system_audio={} microphone_device={:?}",
        monitor_index, x, y, width, height, canvas_width, canvas_height, fps, record_audio,
        record_system_audio, microphone_device_id
    ));
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
    if state.inner().is_active()? {
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

    let output_path = recording_output_path(&app)?;
    #[cfg(not(target_os = "windows"))]
    let audio = if record_audio {
        Some(
            start_audio_capture(microphone_device_id.as_deref())
                .map_err(|error| format!("麥克風無法啟動：{error}"))?,
        )
    } else {
        None
    };
    #[cfg(not(target_os = "windows"))]
    let system_audio = if record_system_audio {
        Some(start_system_audio_capture().map_err(|error| format!("系統音訊無法啟動：{error}"))?)
    } else {
        None
    };
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (finished_tx, finished_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let worker_path = output_path.clone();
    let worker_partial_path = output_path.with_extension("partial.mp4");
    let safe_fps = fps;
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (|| {
                log_backend("[record-worker] listing monitors...");
                let monitors = Monitor::all().map_err(|e| format!("無法列出錄影螢幕：{e}"))?;
                log_backend(&format!(
                    "[record-worker] found {} monitors",
                    monitors.len()
                ));
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
                log_backend(&format!(
                    "[record-worker] using monitor: {:?}",
                    monitor.name().unwrap_or_default()
                ));

                log_backend("[record-worker] capturing first image...");
                let first_image = monitor
                    .capture_image()
                    .map_err(|e| format!("啟動後無法取得第一個螢幕影格：{e}"))?;
                log_backend(&format!(
                    "[record-worker] first image size: {}x{}",
                    first_image.width(),
                    first_image.height()
                ));

                let first_frame = Frame::new(
                    first_image.width(),
                    first_image.height(),
                    first_image.into_raw(),
                );
                log_backend("[record-worker] resolving crop rect...");
                let crop = resolve_crop_from_canvas(
                    &first_frame,
                    canvas_width,
                    canvas_height,
                    x,
                    y,
                    width,
                    height,
                )?;
                log_backend(&format!(
                    "[record-worker] resolved crop rect: x={}, y={}, w={}, h={}",
                    crop.x, crop.y, crop.width, crop.height
                ));

                #[cfg(target_os = "windows")]
                {
                    let monitor_device_name = monitor
                        .name()
                        .unwrap_or_else(|_| desired_name.clone());
                    log_backend(
                        "[record-worker] preparing paused Windows.Graphics.Capture pipeline",
                    );
                    let recording = crate::recording_wgc::WgcRecording::start(
                        &monitor_device_name,
                        crop,
                        safe_fps,
                        &worker_partial_path,
                    )?;
                    // WGC and MediaTranscoder are now ready but frames are
                    // gated. Start audio afterwards so encoder initialization
                    // can never appear as leading audio in the final MP4.
                    let audio = if record_audio {
                        Some(
                            start_audio_capture(microphone_device_id.as_deref())
                                .map_err(|error| format!("麥克風無法啟動：{error}"))?,
                        )
                    } else {
                        None
                    };
                    let system_audio = if record_system_audio {
                        Some(
                            start_system_audio_capture()
                                .map_err(|error| format!("系統音訊無法啟動：{error}"))?,
                        )
                    } else {
                        None
                    };
                    recording.begin();
                    log_backend(
                        "[record-worker] audio ready; released first Windows.Graphics.Capture frame",
                    );
                    let started = RecordingResult {
                        path: worker_path.to_string_lossy().into_owned(),
                        frame_count: 0,
                        width: crop.width,
                        height: crop.height,
                    };
                    let _ = ready_tx.send(Ok(started));
                    let capture_result = recording.wait_and_finish(worker_stop)?;
                    log_backend(&format!(
                        "[record-worker] Windows.Graphics.Capture submitted frames={} first_qpc_100ns={}",
                        capture_result.submitted_frames,
                        capture_result.first_frame_timestamp_100ns,
                    ));
                    return finalize_windows_captured_video(
                        &worker_partial_path,
                        &worker_path,
                        crop,
                        capture_result.submitted_frames,
                        capture_result.first_frame_timestamp_100ns,
                        safe_fps,
                        audio,
                        system_audio,
                    );
                }
                #[cfg(not(target_os = "windows"))]
                log_backend("[record-worker] initializing video recorder...");
                #[cfg(not(target_os = "windows"))]
                let (recorder, receiver) = monitor
                    .video_recorder()
                    .map_err(|e| format!("無法啟動原生螢幕錄影器：{e}"))?;

                #[cfg(not(target_os = "windows"))]
                log_backend("[record-worker] starting video recorder...");
                #[cfg(not(target_os = "windows"))]
                recorder
                    .start()
                    .map_err(|e| format!("無法開始原生螢幕錄影器：{e}"))?;
                #[cfg(not(target_os = "windows"))]
                log_backend("[record-worker] video recorder started successfully.");
                #[cfg(not(target_os = "windows"))]
                let frame_source = FrameSource::Native {
                    recorder,
                    receiver,
                    last_frame: std::sync::Mutex::new(Some(first_frame.clone())),
                };

                #[cfg(not(target_os = "windows"))]
                let started = RecordingResult {
                    path: worker_path.to_string_lossy().into_owned(),
                    frame_count: 0,
                    width: crop.width,
                    height: crop.height,
                };
                #[cfg(not(target_os = "windows"))]
                let _ = ready_tx.send(Ok(started));
                #[cfg(not(target_os = "windows"))]
                let encoded = encode_recording(
                    first_frame,
                    frame_source,
                    crop,
                    safe_fps,
                    worker_path,
                    worker_stop,
                    audio,
                    system_audio,
                );
                #[cfg(not(target_os = "windows"))]
                return encoded;
            })()
        }))
        .unwrap_or_else(|payload| {
            let detail = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic");
            Err(format!("錄影背景工作發生未預期錯誤：{detail}"))
        });
        if let Err(error) = &result {
            let _ = fs::remove_file(&worker_partial_path);
            log_backend(&format!("[record-worker] error occurred: {error}"));
            let _ = ready_tx.send(Err(error.clone()));
        }
        let _ = finished_tx.send(result);
    });
    let started = match ready_rx.recv_timeout(Duration::from_secs(6)) {
        Ok(Ok(started)) => started,
        Ok(Err(error)) => {
            stop.store(true, Ordering::SeqCst);
            std::thread::spawn(move || {
                let _ = finished_rx.recv();
            });
            return Err(error);
        }
        Err(error) => {
            stop.store(true, Ordering::SeqCst);
            std::thread::spawn(move || {
                let _ = finished_rx.recv();
            });
            return Err(format!("等待原生錄影啟動逾時：{error}"));
        }
    };
    log_backend(&format!(
        "[record] start_recording ready path={} size={}x{}",
        started.path, started.width, started.height
    ));
    state.inner().install(RecordingSession {
        stop,
        finished: finished_rx,
    })?;
    Ok(started)
}

fn current_recording_status(state: &RecordingState) -> Result<RecordingStatus, String> {
    match state.poll()? {
        RecordingPoll::Inactive => Ok(RecordingStatus {
            phase: "idle".to_string(),
            result: None,
            error: None,
        }),
        RecordingPoll::Active { stopping } => Ok(RecordingStatus {
            phase: if stopping { "finalizing" } else { "recording" }.to_string(),
            result: None,
            error: None,
        }),
        RecordingPoll::Finished(Ok(result)) => Ok(RecordingStatus {
            phase: "completed".to_string(),
            result: Some(result),
            error: None,
        }),
        RecordingPoll::Finished(Err(error)) => Ok(RecordingStatus {
            phase: "failed".to_string(),
            result: None,
            error: Some(error),
        }),
    }
}

#[tauri::command]
pub fn stop_recording(state: tauri::State<'_, RecordingState>) -> Result<RecordingStatus, String> {
    eprintln!("[record] stop_recording requested");
    state.inner().request_stop()?;
    current_recording_status(state.inner())
}

#[tauri::command]
pub fn get_recording_status(
    state: tauri::State<'_, RecordingState>,
) -> Result<RecordingStatus, String> {
    current_recording_status(state.inner())
}

#[allow(unreachable_code)]
fn encode_recording(
    first: Frame,
    frame_source: FrameSource,
    crop: Crop,
    fps: u32,
    path: PathBuf,
    stop: Arc<AtomicBool>,
    audio: Option<AudioCapture>,
    system_audio: Option<SystemAudioCapture>,
) -> Result<RecordingResult, String> {
    #[cfg(target_os = "windows")]
    {
        return encode_recording_media_foundation(
            first,
            frame_source,
            crop,
            fps,
            path,
            stop,
            audio,
            system_audio,
        );
    }
    let encoder_config = EncoderConfig::new()
        .usage_type(UsageType::ScreenContentRealTime)
        .max_frame_rate(FrameRate::from_hz(fps as f32))
        .bitrate(BitRate::from_bps(
            (crop.width * crop.height * fps / 8).clamp(2_000_000, 20_000_000),
        ))
        .intra_frame_period(IntraFramePeriod::from_num_frames(fps * 2))
        .skip_frames(false);
    let mut encoder = Encoder::with_api_config(OpenH264API::from_source(), encoder_config)
        .map_err(|e| format!("建立 H.264 編碼器失敗：{e}"))?;
    let file = File::create(&path).map_err(|e| format!("建立 MP4 檔案失敗：{e}"))?;
    let config = Mp4Config {
        major_brand: "isom".parse::<FourCC>().map_err(|e| e.to_string())?,
        minor_version: 512,
        compatible_brands: ["isom", "iso2", "avc1", "mp41"]
            .into_iter()
            .map(|s| s.parse::<FourCC>().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?,
        timescale: 1_000,
    };
    let mut writer = Mp4Writer::write_start(BufWriter::new(file), &config)
        .map_err(|e| format!("初始化 MP4 失敗：{e}"))?;
    // Both microphone and system sources are normalized by their `finish`
    // implementations before AAC encoding, so the MP4 track format is stable
    // even when a USB microphone exposes an unusual native format.
    let audio_info_source = (audio.is_some() || system_audio.is_some()).then_some((44_100, 2));
    let audio_info = audio_info_source.map(|(sample_rate, channels)| {
        let freq_index = match sample_rate {
            96_000 => SampleFreqIndex::Freq96000,
            88_200 => SampleFreqIndex::Freq88200,
            64_000 => SampleFreqIndex::Freq64000,
            48_000 => SampleFreqIndex::Freq48000,
            44_100 => SampleFreqIndex::Freq44100,
            32_000 => SampleFreqIndex::Freq32000,
            24_000 => SampleFreqIndex::Freq24000,
            22_050 => SampleFreqIndex::Freq22050,
            16_000 => SampleFreqIndex::Freq16000,
            12_000 => SampleFreqIndex::Freq12000,
            11_025 => SampleFreqIndex::Freq11025,
            8_000 => SampleFreqIndex::Freq8000,
            7_350 => SampleFreqIndex::Freq7350,
            _ => unreachable!("unsupported microphone sample rate was rejected above"),
        };
        let chan_conf = if channels == 1 {
            ChannelConfig::Mono
        } else {
            ChannelConfig::Stereo
        };
        (sample_rate, channels, freq_index, chan_conf)
    });
    let mut recording_clock = RecordingClock::new(fps);
    let mut pending = Some(first);
    let mut pending_sample: Option<(u64, bool, Bytes)> = None;
    let mut track_added = false;
    let mut frame_count = 0u64;
    let mut previous_rgba: Option<Vec<u8>> = None;
    let mut previous_encoded: Option<(Vec<u8>, FrameType)> = None;

    loop {
        if stop.load(Ordering::SeqCst) && pending.is_none() {
            break;
        }
        let frame = if let Some(frame) = pending.take() {
            frame
        } else {
            // Do not block the cadence while waiting for a desktop-change
            // event; FrameSource repeats its cached frame for static scenes.
            match frame_source.next_frame(Duration::from_millis(1))? {
                Some(frame) => frame,
                None if stop.load(Ordering::SeqCst) => break,
                None => continue,
            }
        };
        if !recording_clock.should_capture() {
            continue;
        }
        let rgba = crop_rgba(&frame, crop)?;
        let encoded_sample = if previous_rgba.as_deref() == Some(rgba.as_slice()) {
            let (bytes, frame_type) = previous_encoded.as_ref().ok_or("缺少快取的 H.264 影格")?;
            H264Sample {
                frame_type: *frame_type,
                sps: None,
                pps: None,
                bytes: bytes.clone(),
            }
        } else {
            let yuv = YUVBuffer::from_rgb_source(RgbaSliceU8::new(
                &rgba,
                (crop.width as usize, crop.height as usize),
            ));
            let encoded = encoder
                .encode(&yuv)
                .map_err(|e| format!("H.264 編碼失敗：{e}"))?;
            let sample = extract_h264_sample(&encoded)?;
            previous_rgba = Some(rgba);
            previous_encoded = Some((sample.bytes.clone(), sample.frame_type));
            sample
        };
        let frame_type = encoded_sample.frame_type;
        if matches!(frame_type, FrameType::Skip | FrameType::Invalid) {
            continue;
        }
        if !track_added {
            add_video_track(
                &mut writer,
                crop.width as u16,
                crop.height as u16,
                encoded_sample.sps.ok_or("第一個 H.264 影格缺少 SPS")?,
                encoded_sample.pps.ok_or("第一個 H.264 影格缺少 PPS")?,
            )?;
            track_added = true;
            // Track 1 is deliberately the video track because all video
            // samples below are written to track 1. Add audio second so it
            // receives track 2 and cannot silently swap the two streams.
            if let Some((sample_rate, _, freq_index, chan_conf)) = audio_info {
                add_audio_track(&mut writer, freq_index, chan_conf, sample_rate)?;
            }
        }
        if !encoded_sample.bytes.is_empty() {
            let timestamp = if pending_sample.is_none() {
                0
            } else {
                recording_clock.elapsed_millis()
            };
            if let Some((previous_timestamp, previous_sync, previous_bytes)) = pending_sample.take()
            {
                let duration = elapsed_sample_duration(previous_timestamp, timestamp);
                write_mp4_sample(
                    &mut writer,
                    1,
                    previous_timestamp,
                    duration,
                    previous_sync,
                    previous_bytes,
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
        let duration = recording_clock.final_sample_duration(timestamp);
        write_mp4_sample(&mut writer, 1, timestamp, duration, is_sync, bytes)?;
        frame_count += 1;
    }
    if frame_count == 0 {
        let _ = fs::remove_file(&path);
        return Err("錄影期間未產生可儲存影格".into());
    }
    let microphone_samples: Option<AudioSamples> = audio.map(AudioCapture::finish).transpose()?;
    let system_samples: Option<AudioSamples> =
        system_audio.map(SystemAudioCapture::finish).transpose()?;
    if let Some(mut samples) = mix_audio_samples(microphone_samples, system_samples) {
        if !samples.samples.is_empty() {
            let gain = normalize_recording_audio(&mut samples);
            let sample_rate = samples.sample_rate;
            let channels = samples.channels;
            let packets = encode_aac(&samples)?;
            let mut audio_sample_index = 0u64;
            for packet in packets {
                let bytes = Bytes::from(packet.bytes);
                let duration = packet.duration.max(1);
                write_mp4_sample(&mut writer, 2, audio_sample_index, duration, true, bytes)?;
                audio_sample_index += duration as u64;
            }
            eprintln!(
                "[record] audio samples={} rate={} channels={} gain={:.2}",
                samples.samples.len(),
                sample_rate,
                channels,
                gain
            );
        } else {
            eprintln!(
                "[record] audio sources produced no samples; video saved without usable audio"
            );
        }
    }
    writer
        .write_end()
        .map_err(|e| format!("完成 MP4 存檔失敗：{e}"))?;
    let size = fs::metadata(&path)
        .map_err(|e| format!("無法驗證 MP4 檔案：{e}"))?
        .len();
    if size == 0 {
        return Err("MP4 檔案為空".into());
    }
    Ok(RecordingResult {
        path: path.to_string_lossy().into_owned(),
        frame_count,
        width: crop.width,
        height: crop.height,
    })
}

#[cfg(target_os = "windows")]
fn encode_recording_media_foundation(
    first: Frame,
    frame_source: FrameSource,
    crop: Crop,
    fps: u32,
    path: PathBuf,
    stop: Arc<AtomicBool>,
    audio: Option<AudioCapture>,
    system_audio: Option<SystemAudioCapture>,
) -> Result<RecordingResult, String> {
    use crate::recording_media_foundation::MediaFoundationWriter;

    let partial_path = path.with_extension("partial.mp4");
    let result = (|| {
        // Keep the Media Foundation sink video-only. Supplying its audio
        // stream only after Stop makes the sink buffer or block the video
        // stream during long recordings.
        let mut writer =
            MediaFoundationWriter::create(&partial_path, crop.width, crop.height, fps)?;
        let mut clock = RecordingClock::new(fps);
        let recording_started = std::time::Instant::now();
        let mut pending = Some(first);
        let mut frame_count = 0u64;
        let mut last_timeline_frame = None;
        let mut rgba = Vec::with_capacity(crop.width as usize * crop.height as usize * 4);

        loop {
            if stop.load(Ordering::SeqCst) && pending.is_none() {
                break;
            }
            let frame = if let Some(frame) = pending.take() {
                frame
            } else {
                match frame_source.next_frame(Duration::from_millis(1))? {
                    Some(frame) => frame,
                    None if stop.load(Ordering::SeqCst) => break,
                    None => continue,
                }
            };
            if !clock.should_capture() {
                continue;
            }
            crop_rgba_into(&frame, crop, &mut rgba)?;
            let timeline_frame =
                (recording_started.elapsed().as_secs_f64() * fps as f64).floor() as u64;
            if last_timeline_frame.is_none_or(|previous| timeline_frame > previous) {
                writer.write_rgba_frame_at(&rgba, timeline_frame)?;
                frame_count += 1;
                last_timeline_frame = Some(timeline_frame);
                if frame_count % (fps as u64 * 5) == 0 {
                    log_backend(&format!(
                        "[record-worker] encoded samples={} timeline={:.1}s",
                        frame_count,
                        timeline_frame as f64 / fps as f64
                    ));
                }
            }
        }

        if let Some(previous) = last_timeline_frame {
            let final_timeline_frame =
                (recording_started.elapsed().as_secs_f64() * fps as f64).ceil() as u64;
            if final_timeline_frame > previous {
                writer.write_rgba_frame_at(&rgba, final_timeline_frame)?;
                frame_count += 1;
            }
        }
        if frame_count == 0 {
            return Err("錄影期間未產生可儲存影格".to_string());
        }

        writer.finish()?;
        finalize_windows_captured_video(
            &partial_path,
            &path,
            crop,
            frame_count,
            0,
            fps,
            audio,
            system_audio,
        )
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial_path);
    }
    result
}

#[cfg(target_os = "windows")]
fn finalize_windows_captured_video(
    partial_path: &PathBuf,
    path: &PathBuf,
    crop: Crop,
    submitted_frames: u64,
    first_video_timestamp_100ns: i64,
    fps: u32,
    audio: Option<AudioCapture>,
    system_audio: Option<SystemAudioCapture>,
) -> Result<RecordingResult, String> {
    let microphone = audio.map(AudioCapture::finish).transpose()?;
    let system = system_audio.map(SystemAudioCapture::finish).transpose()?;
    match mix_audio_samples(microphone, system) {
        Some(mut samples) if !samples.samples.is_empty() => {
            let alignment_ms =
                align_audio_to_video_timeline(&mut samples, first_video_timestamp_100ns);
            log_backend(&format!(
                "[record-worker] QPC audio/video alignment applied offset_ms={alignment_ms:?}"
            ));
            let gain = normalize_recording_audio(&mut samples);
            log_backend(&format!("[record-worker] normalized audio gain={gain:.2}"));
            remux_media_foundation_video(partial_path, path, Some(&samples), fps)?;
            fs::remove_file(partial_path).map_err(|e| format!("清理暫存影片失敗：{e}"))?;
        }
        _ => {
            remux_media_foundation_video(partial_path, path, None, fps)?;
            fs::remove_file(partial_path).map_err(|e| format!("清理暫存影片失敗：{e}"))?;
        }
    }
    let output_size = fs::metadata(path)
        .map_err(|e| format!("錄影完成但無法驗證輸出檔案：{e}"))?
        .len();
    if output_size == 0 {
        return Err("錄影輸出檔案為空".to_string());
    }
    let output_file = File::open(path).map_err(|e| format!("錄影完成但無法讀取輸出檔案：{e}"))?;
    let output_reader = mp4::Mp4Reader::read_header(output_file, output_size)
        .map_err(|e| format!("錄影完成但 MP4 驗證失敗：{e}"))?;
    let actual_frame_count = output_reader
        .tracks()
        .iter()
        .find_map(|(track_id, track)| {
            (track.track_type().ok() == Some(TrackType::Video)).then_some(*track_id)
        })
        .ok_or_else(|| "錄影完成但 MP4 缺少影片軌".to_string())
        .and_then(|track_id| {
            output_reader
                .sample_count(track_id)
                .map(u64::from)
                .map_err(|e| format!("讀取 MP4 影格數失敗：{e}"))
        })?;
    log_backend(&format!(
        "[record-worker] finalized video samples={actual_frame_count} submitted={submitted_frames}"
    ));
    Ok(RecordingResult {
        path: path.to_string_lossy().into_owned(),
        frame_count: actual_frame_count,
        width: crop.width,
        height: crop.height,
    })
}

#[cfg(target_os = "windows")]
fn align_audio_to_video_timeline(
    audio: &mut AudioSamples,
    first_video_timestamp_100ns: i64,
) -> Option<i64> {
    let audio_start = audio.start_timestamp_100ns? as i128;
    let video_start = first_video_timestamp_100ns as i128;
    let delta_100ns = audio_start - video_start;
    // Both values are QPC-derived 100-nanosecond timestamps. A larger gap
    // indicates a clock-origin error or invalid driver timestamp, not a real
    // recorder startup delay; in that case preserve the source instead of
    // deleting an unbounded amount of audio.
    if delta_100ns.unsigned_abs() > 300_000_000 {
        return None;
    }
    let frame_delta = ((delta_100ns.unsigned_abs() * audio.sample_rate as u128) / 10_000_000)
        .min(usize::MAX as u128) as usize;
    let sample_delta = frame_delta.saturating_mul(audio.channels as usize);
    if delta_100ns < 0 {
        let trim = sample_delta.min(audio.samples.len());
        audio.samples.drain(..trim);
    } else if delta_100ns > 0 && sample_delta > 0 {
        let mut aligned = Vec::with_capacity(sample_delta.saturating_add(audio.samples.len()));
        aligned.resize(sample_delta, 0.0);
        aligned.append(&mut audio.samples);
        audio.samples = aligned;
    }
    audio.start_timestamp_100ns = u64::try_from(first_video_timestamp_100ns).ok();
    Some((delta_100ns / 10_000) as i64)
}

#[cfg(target_os = "windows")]
fn remux_media_foundation_video(
    video_path: &PathBuf,
    output_path: &PathBuf,
    audio: Option<&AudioSamples>,
    fps: u32,
) -> Result<(), String> {
    if let Some(audio) = audio {
        if audio.sample_rate != 44_100 || audio.channels != 2 {
            return Err("Windows MP4 音訊必須為 44.1 kHz 雙聲道".to_string());
        }
    }
    let source_file = File::open(video_path).map_err(|e| format!("開啟暫存影片失敗：{e}"))?;
    let source_size = source_file
        .metadata()
        .map_err(|e| format!("讀取暫存影片資訊失敗：{e}"))?
        .len();
    let mut reader = mp4::Mp4Reader::read_header(source_file, source_size)
        .map_err(|e| format!("讀取 Windows 暫存 MP4 失敗：{e}"))?;
    let (source_track_id, source_timescale, sample_count, width, height, sps, pps) = {
        let (track_id, track) = reader
            .tracks()
            .iter()
            .find(|(_, track)| track.track_type().ok() == Some(TrackType::Video))
            .ok_or("Windows 暫存 MP4 缺少影片軌")?;
        (
            *track_id,
            track.timescale(),
            track.sample_count(),
            track.width(),
            track.height(),
            track
                .sequence_parameter_set()
                .map_err(|e| format!("讀取 H.264 SPS 失敗：{e}"))?
                .to_vec(),
            track
                .picture_parameter_set()
                .map_err(|e| format!("讀取 H.264 PPS 失敗：{e}"))?
                .to_vec(),
        )
    };
    if source_timescale == 0 {
        return Err("Windows 暫存 MP4 的影片時間基準無效".to_string());
    }

    let mux_path = output_path.with_extension("mux.partial.mp4");
    let mux_result = (|| {
        let destination =
            File::create(&mux_path).map_err(|e| format!("建立含音訊 MP4 失敗：{e}"))?;
        let config = Mp4Config {
            major_brand: "isom".parse::<FourCC>().map_err(|e| e.to_string())?,
            minor_version: 512,
            compatible_brands: ["isom", "iso2", "avc1", "mp41"]
                .into_iter()
                .map(|brand| brand.parse::<FourCC>().map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?,
            timescale: 1_000,
        };
        let mut writer = Mp4Writer::write_start(BufWriter::new(destination), &config)
            .map_err(|e| format!("初始化含音訊 MP4 失敗：{e}"))?;
        add_video_track(&mut writer, width, height, sps, pps)?;
        if let Some(audio) = audio {
            add_audio_track(
                &mut writer,
                SampleFreqIndex::Freq44100,
                ChannelConfig::Stereo,
                audio.sample_rate,
            )?;
        }

        let mut video_end_ms = 0u64;
        for sample_id in 1..=sample_count {
            let sample = reader
                .read_sample(source_track_id, sample_id)
                .map_err(|e| format!("讀取 H.264 sample {sample_id} 失敗：{e}"))?
                .ok_or_else(|| format!("H.264 sample {sample_id} 不存在"))?;
            // MediaTranscoder commonly emits a 1 kHz MP4 track where 30 FPS
            // becomes a fixed 33 ms/sample (30.303 FPS). Reconstruct the CFR
            // timeline with distributed 33/34 ms durations so elapsed video
            // time remains exact instead of drifting by 1%.
            let frame_index = u64::from(sample_id - 1);
            let fps = fps.max(1) as u64;
            let start_time = (frame_index * 1_000 + fps / 2) / fps;
            let next_time = ((frame_index + 1) * 1_000 + fps / 2) / fps;
            let duration = next_time
                .saturating_sub(start_time)
                .clamp(1, u32::MAX as u64) as u32;
            let rendering_offset = scale_mp4_signed_time(sample.rendering_offset, source_timescale);
            video_end_ms = video_end_ms.max(next_time);
            writer
                .write_sample(
                    1,
                    &mp4::Mp4Sample {
                        start_time,
                        duration,
                        rendering_offset,
                        is_sync: sample.is_sync,
                        bytes: sample.bytes,
                    },
                )
                .map_err(|e| format!("寫入 H.264 sample {sample_id} 失敗：{e}"))?;
        }

        if let Some(audio) = audio {
            let audio_frame_limit = ((video_end_ms as u128 * audio.sample_rate as u128 + 999)
                / 1_000)
                .min(usize::MAX as u128) as usize;
            let sample_limit = audio_frame_limit
                .saturating_mul(audio.channels as usize)
                .min(audio.samples.len());
            let synchronized_audio = AudioSamples {
                samples: audio.samples[..sample_limit].to_vec(),
                channels: audio.channels,
                sample_rate: audio.sample_rate,
                start_timestamp_100ns: audio.start_timestamp_100ns,
            };
            let mut audio_time = 0u64;
            for packet in encode_aac(&synchronized_audio)? {
                let duration = packet.duration.max(1);
                write_mp4_sample(
                    &mut writer,
                    2,
                    audio_time,
                    duration,
                    true,
                    Bytes::from(packet.bytes),
                )?;
                audio_time += duration as u64;
            }
        }
        writer
            .write_end()
            .map_err(|e| format!("完成含音訊 MP4 失敗：{e}"))?;
        fs::rename(&mux_path, output_path).map_err(|e| format!("發布含音訊 MP4 失敗：{e}"))
    })();
    if mux_result.is_err() {
        let _ = fs::remove_file(&mux_path);
    }
    mux_result
}

#[cfg(all(target_os = "windows", test))]
fn mux_media_foundation_video_with_audio(
    video_path: &PathBuf,
    output_path: &PathBuf,
    audio: &AudioSamples,
    fps: u32,
) -> Result<(), String> {
    remux_media_foundation_video(video_path, output_path, Some(audio), fps)
}

#[cfg(target_os = "windows")]
fn scale_mp4_signed_time(value: i32, source_timescale: u32) -> i32 {
    ((value as i64 * 1_000) / source_timescale as i64).clamp(i32::MIN as i64, i32::MAX as i64)
        as i32
}

#[derive(serde::Deserialize)]
pub struct ScrollConfig {
    pub mode: String, // "auto" or "manual"
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scroll_amount: Option<i32>,
    pub monitor_offset_x: Option<i32>,
    pub monitor_offset_y: Option<i32>,
}

fn validate_scroll_mode(mode: &str) -> Result<(), String> {
    if matches!(mode, "auto" | "manual") {
        Ok(())
    } else {
        Err("捲動擷取模式只支援 auto 或 manual".to_string())
    }
}

#[tauri::command]
pub fn trigger_scroll_capture(config: ScrollConfig) -> Result<String, String> {
    validate_scroll_mode(&config.mode)?;
    let offset_x = config.monitor_offset_x.unwrap_or(0);
    let offset_y = config.monitor_offset_y.unwrap_or(0);

    let _cx = (offset_x + config.x + (config.width as i32 / 2)) as f64;
    let _cy = (offset_y + config.y + (config.height as i32 / 2)) as f64;

    let _amount = config.scroll_amount.unwrap_or(5);

    #[cfg(target_os = "macos")]
    unsafe {
        // Step 1: Click at center of selection to activate and focus the target application window
        let pt = CGPoint { x: cx, y: cy };
        // Mouse moved / click down (1) and up (2)
        let down = CGEventCreateMouseEvent(std::ptr::null_mut(), 1, pt, 0);
        if !down.is_null() {
            CGEventPost(0, down);
            CFRelease(down);
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
        let up = CGEventCreateMouseEvent(std::ptr::null_mut(), 2, pt, 0);
        if !up.is_null() {
            CGEventPost(0, up);
            CFRelease(up);
        }
        std::thread::sleep(std::time::Duration::from_millis(60));

        // Step 2: Send pixel-level scroll wheel events downwards (-280 pixels per scroll)
        for _ in 0..amount {
            let ev = CGEventCreateScrollWheelEvent2(
                std::ptr::null_mut(),
                0,    // 0 = kCGScrollEventUnitPixel
                1,    // 1 wheel
                -280, // negative is scroll down
                0,
                0,
            );
            if !ev.is_null() {
                CGEventPost(0, ev);
                CFRelease(ev);
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }

    // Step 3: AppleScript fallback keystrokes to ensure scroll in web browsers / word processors
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"System Events\" to repeat {} times\nkey code 125\ndelay 0.03\nend repeat",
            amount * 2
        );
        let _ = Command::new("osascript").arg("-e").arg(&script).output();
    }

    Ok("scroll_done".to_string())
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    #[cfg(target_os = "windows")]
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn accepts_only_supported_scroll_modes() {
        assert!(validate_scroll_mode("auto").is_ok());
        assert!(validate_scroll_mode("manual").is_ok());
        assert!(validate_scroll_mode("unexpected").is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn trims_audio_that_started_before_first_video_frame() {
        let mut audio = AudioSamples {
            samples: (0..200).map(|value| value as f32).collect(),
            channels: 1,
            sample_rate: 1_000,
            start_timestamp_100ns: Some(9_000_000),
        };
        assert_eq!(
            align_audio_to_video_timeline(&mut audio, 10_000_000),
            Some(-100)
        );
        assert_eq!(audio.samples.len(), 100);
        assert_eq!(audio.samples[0], 100.0);
        assert_eq!(audio.start_timestamp_100ns, Some(10_000_000));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn pads_audio_that_started_after_first_video_frame() {
        let mut audio = AudioSamples {
            samples: vec![1.0; 200],
            channels: 1,
            sample_rate: 1_000,
            start_timestamp_100ns: Some(11_000_000),
        };
        assert_eq!(
            align_audio_to_video_timeline(&mut audio, 10_000_000),
            Some(100)
        );
        assert_eq!(audio.samples.len(), 300);
        assert!(audio.samples[..100].iter().all(|sample| *sample == 0.0));
        assert!(audio.samples[100..].iter().all(|sample| *sample == 1.0));
    }

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
            None,
        )
        .unwrap();
        assert!(result.frame_count >= 1);
        let file = File::open(&path).unwrap();
        let size = file.metadata().unwrap().len();
        let reader = mp4::Mp4Reader::read_header(file, size).unwrap();
        assert_eq!(reader.tracks().len(), 1);
        assert_eq!(reader.sample_count(1).unwrap() as u64, result.frame_count);
        fs::remove_file(path).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn muxes_media_foundation_video_and_aac_without_ffmpeg() {
        use crate::recording_media_foundation::MediaFoundationWriter;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let video_path = std::env::temp_dir().join(format!("unisnap-video-{nonce}.mp4"));
        let output_path = std::env::temp_dir().join(format!("unisnap-av-{nonce}.mp4"));
        let mut writer = MediaFoundationWriter::create(&video_path, 320, 240, 30).unwrap();
        let frame = vec![96u8; 320 * 240 * 4];
        for index in 0..90 {
            writer.write_rgba_frame_at(&frame, index).unwrap();
        }
        writer.finish().unwrap();
        let samples = (0..44_100 * 3)
            .flat_map(|sample| {
                let value = (sample as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin() * 0.15;
                [value, value]
            })
            .collect();
        mux_media_foundation_video_with_audio(
            &video_path,
            &output_path,
            &AudioSamples {
                samples,
                channels: 2,
                sample_rate: 44_100,
                start_timestamp_100ns: None,
            },
            30,
        )
        .unwrap();

        let file = File::open(&output_path).unwrap();
        let size = file.metadata().unwrap().len();
        let reader = mp4::Mp4Reader::read_header(file, size).unwrap();
        assert_eq!(reader.tracks().len(), 2);
        assert_eq!(reader.sample_count(1).unwrap(), 90);
        assert!(reader.sample_count(2).unwrap() > 0);
        assert!(reader.duration() >= Duration::from_millis(2_900));
        fs::remove_file(video_path).unwrap();
        fs::remove_file(output_path).unwrap();
    }

    #[test]
    #[ignore = "requires local screen-recording permission and a connected monitor"]
    fn records_real_monitor_frames_to_readable_mp4() {
        let probe_seconds = std::env::var("UNISNAP_RECORDING_PROBE_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value >= 2)
            .unwrap_or(30);
        let keep_probe_directory =
            std::env::var_os("UNISNAP_KEEP_RECORDING_PROBES").map(PathBuf::from);
        let requested_monitor = std::env::var("UNISNAP_RECORDING_PROBE_MONITOR")
            .ok()
            .and_then(|value| value.parse::<usize>().ok());
        for (index, monitor) in Monitor::all()
            .unwrap()
            .into_iter()
            .enumerate()
            .filter(|(index, _)| requested_monitor.is_none_or(|requested| requested == *index))
        {
            let image = monitor.capture_image().unwrap();
            // Match the pixel load of a typical large user selection instead
            // of only proving that a tiny 640x360 stream can start. Non-zero
            // offsets exercise the GPU crop path used by rectangular capture.
            let crop_x = if image.width() > 128 { 32 } else { 0 };
            let crop_y = if image.height() > 128 { 24 } else { 0 };
            let width = (image.width() - crop_x).min(2520) & !1;
            let height = (image.height() - crop_y).min(1480) & !1;
            let crop = Crop {
                x: crop_x,
                y: crop_y,
                width,
                height,
            };
            let path = keep_probe_directory
                .clone()
                .unwrap_or_else(std::env::temp_dir)
                .join(format!("screenshot-real-recording-probe-{index}.mp4"));
            let stop = Arc::new(AtomicBool::new(false));
            let timed_stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(probe_seconds));
                timed_stop.store(true, Ordering::SeqCst);
            });
            let recording = crate::recording_wgc::WgcRecording::start(
                &monitor.name().unwrap(),
                crop,
                30,
                &path,
            )
            .unwrap();
            recording.begin();
            let capture_result = recording.wait_and_finish(stop).unwrap();
            let submitted_frames = capture_result.submitted_frames;
            assert!(
                submitted_frames >= 10,
                "monitor {index} encoded only {} frames",
                submitted_frames
            );
            let file = File::open(&path).unwrap();
            let size = file.metadata().unwrap().len();
            let reader = mp4::Mp4Reader::read_header(file, size).unwrap();
            let (video_track_id, video_track) = reader
                .tracks()
                .iter()
                .find(|(_, track)| track.track_type().unwrap() == TrackType::Video)
                .expect("Windows encoder output did not contain a video track");
            assert_eq!(video_track.width(), width as u16);
            assert_eq!(video_track.height(), height as u16);
            assert!(
                reader.duration() >= Duration::from_secs(probe_seconds - 1),
                "monitor {index} MP4 duration was only {:?}",
                reader.duration()
            );
            let output_frames = reader.sample_count(*video_track_id).unwrap() as u64;
            let average_fps = output_frames as f64 / reader.duration().as_secs_f64();
            eprintln!(
                "monitor {index}: submitted={submitted_frames} output={output_frames} duration={:.2}s average_fps={average_fps:.2}",
                reader.duration().as_secs_f64()
            );
            assert!(
                (29.0..=31.0).contains(&average_fps),
                "monitor {index} output averaged {average_fps:.2} FPS instead of 30 FPS"
            );
            let mux_path = path.with_file_name(format!(
                "screenshot-real-recording-probe-{index}-with-audio.mp4"
            ));
            let _ = fs::remove_file(&mux_path);
            mux_media_foundation_video_with_audio(
                &path,
                &mux_path,
                &AudioSamples {
                    samples: vec![0.0; probe_seconds as usize * 44_100 * 2],
                    channels: 2,
                    sample_rate: 44_100,
                    start_timestamp_100ns: None,
                },
                30,
            )
            .unwrap();
            let mux_file = File::open(&mux_path).unwrap();
            let mux_size = mux_file.metadata().unwrap().len();
            let mux_reader = mp4::Mp4Reader::read_header(mux_file, mux_size).unwrap();
            assert_eq!(mux_reader.tracks().len(), 2);
            let mux_video_duration = mux_reader
                .tracks()
                .values()
                .find(|track| track.track_type().ok() == Some(TrackType::Video))
                .unwrap()
                .duration();
            let mux_audio_duration = mux_reader
                .tracks()
                .values()
                .find(|track| track.track_type().ok() == Some(TrackType::Audio))
                .filter(|track| track.sample_count() > 0)
                .unwrap()
                .duration();
            assert!(
                mux_audio_duration.abs_diff(mux_video_duration) <= Duration::from_millis(50),
                "audio/video duration drift was {:?}",
                mux_audio_duration.abs_diff(mux_video_duration)
            );
            if keep_probe_directory.is_none() {
                fs::remove_file(path).unwrap();
                fs::remove_file(mux_path).unwrap();
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires a local monitor, WASAPI output, ffplay, and an AV pulse probe"]
    fn records_qpc_aligned_system_audio_probe() {
        let media_path = PathBuf::from(
            std::env::var_os("UNISNAP_AV_SYNC_PROBE_MEDIA")
                .expect("UNISNAP_AV_SYNC_PROBE_MEDIA is required"),
        );
        let output_path = PathBuf::from(
            std::env::var_os("UNISNAP_AV_SYNC_PROBE_OUTPUT")
                .expect("UNISNAP_AV_SYNC_PROBE_OUTPUT is required"),
        );
        let player =
            std::env::var_os("UNISNAP_AV_SYNC_PROBE_PLAYER").unwrap_or_else(|| "ffplay".into());
        let monitor_index = std::env::var("UNISNAP_RECORDING_PROBE_MONITOR")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let monitor = Monitor::all()
            .unwrap()
            .into_iter()
            .nth(monitor_index)
            .expect("requested monitor is unavailable");
        let image = monitor.capture_image().unwrap();
        let crop = Crop {
            x: 0,
            y: 0,
            width: image.width() & !1,
            height: image.height() & !1,
        };
        let partial_path = output_path.with_extension("partial.mp4");
        let _ = fs::remove_file(&partial_path);
        let _ = fs::remove_file(&output_path);
        let recording = crate::recording_wgc::WgcRecording::start(
            &monitor.name().unwrap(),
            crop,
            30,
            &partial_path,
        )
        .unwrap();
        let system_audio = start_system_audio_capture().unwrap();
        recording.begin();
        let mut child = Command::new(player)
            .args(["-autoexit", "-fs", "-loglevel", "error", "-i"])
            .arg(&media_path)
            .spawn()
            .expect("failed to launch AV pulse probe");
        std::thread::sleep(Duration::from_secs(12));
        let stop = Arc::new(AtomicBool::new(true));
        let capture_result = recording.wait_and_finish(stop).unwrap();
        let _ = child.kill();
        let _ = child.wait();
        finalize_windows_captured_video(
            &partial_path,
            &output_path,
            crop,
            capture_result.submitted_frames,
            capture_result.first_frame_timestamp_100ns,
            30,
            None,
            Some(system_audio),
        )
        .unwrap();
        assert!(output_path.exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires UNISNAP_AV_SYNC_PROBE_MEDIA and UNISNAP_AV_SYNC_MUX_OUTPUT"]
    fn muxes_sample_exact_av_pulse_probe() {
        let video_path = PathBuf::from(
            std::env::var_os("UNISNAP_AV_SYNC_PROBE_MEDIA")
                .expect("UNISNAP_AV_SYNC_PROBE_MEDIA is required"),
        );
        let output_path = PathBuf::from(
            std::env::var_os("UNISNAP_AV_SYNC_MUX_OUTPUT")
                .expect("UNISNAP_AV_SYNC_MUX_OUTPUT is required"),
        );
        let sample_rate = 44_100u32;
        let duration_seconds = 20usize;
        let mut samples = Vec::with_capacity(duration_seconds * sample_rate as usize * 2);
        for frame in 0..duration_seconds * sample_rate as usize {
            let time = frame as f32 / sample_rate as f32;
            let value = if time % 2.0 < 0.2 {
                (time * 1_000.0 * std::f32::consts::TAU).sin() * 0.8
            } else {
                0.0
            };
            samples.extend([value, value]);
        }
        let _ = fs::remove_file(&output_path);
        mux_media_foundation_video_with_audio(
            &video_path,
            &output_path,
            &AudioSamples {
                samples,
                channels: 2,
                sample_rate,
                start_timestamp_100ns: None,
            },
            30,
        )
        .unwrap();
        assert!(output_path.exists());
    }
}
