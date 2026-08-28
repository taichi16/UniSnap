use crate::capture::log_backend;
use std::fs::{self, File};
use std::io::BufWriter;
#[cfg(target_os = "windows")]
use std::io::Write;
#[cfg(target_os = "windows")]
use std::process::{Command, Stdio};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

use crate::frame_source::FrameSource;
use crate::h264_sample::{extract_h264_sample, H264Sample};
use crate::platform::windows::{validate_recording_fps, validate_recording_options};
use crate::recording_crop::{crop_rgba, resolve_crop, Crop};
use crate::recording_mp4::{
    aac_packet_duration_ms, add_audio_track, add_video_track, write_sample as write_mp4_sample,
};
use crate::recording_output::recording_output_path;
use crate::recording_session::{RecordingSession, RecordingState as SessionState};
use crate::recording_timing::{elapsed_sample_duration, RecordingClock};
use mp4::{Bytes, ChannelConfig, FourCC, Mp4Config, Mp4Writer, SampleFreqIndex};
use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, UsageType,
};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
use openh264::OpenH264API;
use tauri::AppHandle;
use xcap::{Frame, Monitor};

use crate::recording_audio::{
    encode_aac, mix_audio_samples, start_audio_capture, AudioCapture, AudioSamples,
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
    fps: u32,
) -> Result<RecordingResult, String> {
    validate_recording_options(record_audio, record_system_audio)?;
    validate_recording_fps(fps)?;
    log_backend(&format!(
        "[record] start_recording monitor={} rect=({}, {}, {}, {}) canvas={}x{} fps={} audio={}",
        monitor_index, x, y, width, height, canvas_width, canvas_height, fps, record_audio
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
    let coordinate_scale = selected_monitor.scale_factor();

    let output_path = recording_output_path(&app)?;
    // Audio-device setup must not prevent a video-only recording.  In
    // particular, Windows can reject microphone access while desktop capture
    // remains available (privacy setting, unplugged microphone, or a driver
    // currently in exclusive mode).
    let audio = if record_audio {
        match start_audio_capture() {
            Ok(capture) => Some(capture),
            Err(error) => {
                log_backend(&format!(
                    "[record] microphone unavailable; continuing without microphone: {error}"
                ));
                None
            }
        }
    } else {
        None
    };
    let system_audio = if record_system_audio {
        match start_system_audio_capture() {
            Ok(capture) => Some(capture),
            Err(error) => {
                log_backend(&format!(
                    "[record] system audio unavailable; continuing without system audio: {error}"
                ));
                None
            }
        }
    } else {
        None
    };
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (finished_tx, finished_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let worker_path = output_path.clone();
    let safe_fps = fps;
    std::thread::spawn(move || {
        let result = (|| {
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
            let crop = resolve_crop(&first_frame, coordinate_scale, x, y, width, height)?;
            log_backend(&format!(
                "[record-worker] resolved crop rect: x={}, y={}, w={}, h={}",
                crop.x, crop.y, crop.width, crop.height
            ));

            #[cfg(target_os = "windows")]
            let frame_source = match monitor.video_recorder() {
                Ok((recorder, receiver)) => match recorder.start() {
                    Ok(()) => {
                        log_backend("[record-worker] using DXGI video recorder");
                        FrameSource::Native {
                            recorder,
                            receiver,
                            last_frame: std::sync::Mutex::new(Some(first_frame.clone())),
                        }
                    }
                    Err(error) => {
                        log_backend(&format!(
                            "[record-worker] DXGI recorder could not start ({error}); using GDI fallback"
                        ));
                        FrameSource::GdiDesktop {
                            x: desired_x,
                            y: desired_y,
                            width: monitor.width().map_err(|e| e.to_string())?,
                            height: monitor.height().map_err(|e| e.to_string())?,
                            fallback_monitor: monitor.clone(),
                        }
                    }
                },
                Err(error) => {
                    log_backend(&format!(
                        "[record-worker] DXGI recorder unavailable ({error}); using GDI fallback"
                    ));
                    FrameSource::GdiDesktop {
                        x: desired_x,
                        y: desired_y,
                        width: monitor.width().map_err(|e| e.to_string())?,
                        height: monitor.height().map_err(|e| e.to_string())?,
                        fallback_monitor: monitor.clone(),
                    }
                }
            };
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

            let started = RecordingResult {
                path: worker_path.to_string_lossy().into_owned(),
                frame_count: 0,
                width: crop.width,
                height: crop.height,
            };
            let _ = ready_tx.send(Ok(started));
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
            encoded
        })();
        if let Err(error) = &result {
            log_backend(&format!("[record-worker] error occurred: {error}"));
            let _ = ready_tx.send(Err(error.clone()));
        }
        let _ = finished_tx.send(result);
    });
    let started = ready_rx
        .recv_timeout(Duration::from_secs(6))
        .map_err(|e| format!("等待原生錄影啟動逾時：{e}"))??;
    log_backend(&format!(
        "[record] start_recording ready path={} size={}x{}",
        started.path, started.width, started.height
    ));
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
        log_backend(&format!("[record] control setup failed: {error}"));
        stop.store(true, Ordering::SeqCst);
        let _ = finished_rx.recv_timeout(Duration::from_secs(30));
        return Err(format!("錄影控制列啟動失敗：{error}"));
    }
    state.inner().install(RecordingSession {
        stop,
        finished: finished_rx,
    })?;
    Ok(started)
}

#[tauri::command]
pub fn stop_recording(state: tauri::State<'_, RecordingState>) -> Result<RecordingResult, String> {
    eprintln!("[record] stop_recording requested");
    let session = state.inner().take()?;
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
    system_audio: Option<SystemAudioCapture>,
) -> Result<RecordingResult, String> {
    #[cfg(target_os = "windows")]
    {
        return encode_recording_ffmpeg(first, frame_source, crop, fps, path, stop, audio, system_audio);
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
    if let Some(capture) = audio.as_ref() {
        if !matches!(
            capture.sample_rate,
            96_000
                | 88_200
                | 64_000
                | 48_000
                | 44_100
                | 32_000
                | 24_000
                | 22_050
                | 16_000
                | 12_000
                | 11_025
                | 8_000
                | 7_350
        ) {
            let _ = fs::remove_file(&path);
            return Err(format!(
                "麥克風取樣率 {} Hz 無法封裝為 MP4 AAC 音軌",
                capture.sample_rate
            ));
        }
    }
    let audio_info_source = if audio.is_some() && system_audio.is_some() {
        Some((44_100, 2))
    } else if let Some(capture) = audio.as_ref() {
        Some((capture.sample_rate, capture.channels))
    } else {
        system_audio
            .as_ref()
            .map(|capture| (capture.sample_rate, capture.channels))
    };
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
            H264Sample { frame_type: *frame_type, sps: None, pps: None, bytes: bytes.clone() }
        } else {
            let yuv = YUVBuffer::from_rgb_source(RgbaSliceU8::new(
                &rgba,
                (crop.width as usize, crop.height as usize),
            ));
            let encoded = encoder.encode(&yuv).map_err(|e| format!("H.264 編碼失敗：{e}"))?;
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
            if let Some((_, _, freq_index, chan_conf)) = audio_info {
                add_audio_track(&mut writer, freq_index, chan_conf)?;
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
    if let Some(samples) = mix_audio_samples(microphone_samples, system_samples) {
        if !samples.samples.is_empty() {
            let sample_rate = samples.sample_rate;
            let channels = samples.channels;
            let packets = encode_aac(&samples)?;
            let mut audio_sample_index = 0u64;
            for packet in packets {
                let bytes = Bytes::from(packet.bytes);
                let duration = aac_packet_duration_ms(packet.duration, sample_rate);
                write_mp4_sample(&mut writer, 2, audio_sample_index, duration, true, bytes)?;
                audio_sample_index += duration as u64;
            }
            eprintln!(
                "[record] audio samples={} rate={} channels={}",
                samples.samples.len(),
                sample_rate,
                channels
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
fn encode_recording_ffmpeg(
    first: Frame,
    frame_source: FrameSource,
    crop: Crop,
    fps: u32,
    path: PathBuf,
    stop: Arc<AtomicBool>,
    audio: Option<AudioCapture>,
    system_audio: Option<SystemAudioCapture>,
) -> Result<RecordingResult, String> {
    let ffmpeg = std::env::var_os("UNISNAP_FFMPEG")
        .map(PathBuf::from)
        .or_else(|| {
            [PathBuf::from(r"K:\ffmpeg\bin\ffmpeg.exe"), PathBuf::from("ffmpeg.exe")]
                .into_iter()
                .find(|candidate| candidate.exists())
        })
        .ok_or("找不到 FFmpeg；請安裝 FFmpeg 或設定 UNISNAP_FFMPEG")?;
    let size = format!("{}x{}", crop.width, crop.height);
    let video_path = path.with_extension("video.mp4");
    let mut child = Command::new(&ffmpeg)
        .args([
            "-hide_banner", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba",
            "-s", &size, "-r", &fps.to_string(), "-i", "-", "-c:v", "libx264",
            "-preset", "ultrafast", "-tune", "zerolatency", "-pix_fmt", "yuv420p",
            "-r", &fps.to_string(), "-fps_mode", "cfr", "-y",
        ])
        .arg(&video_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("啟動 FFmpeg 失敗：{e}"))?;
    let mut input = child.stdin.take().ok_or("FFmpeg stdin unavailable")?;
    let mut clock = RecordingClock::new(fps);
    let mut pending = Some(first);
    let mut frame_count = 0u64;
    loop {
        if stop.load(Ordering::SeqCst) && pending.is_none() { break; }
        let frame = if let Some(frame) = pending.take() {
            frame
        } else {
            match frame_source.next_frame(Duration::from_millis(1))? {
                Some(frame) => frame,
                None if stop.load(Ordering::SeqCst) => break,
                None => continue,
            }
        };
        if !clock.should_capture() { continue; }
        let rgba = crop_rgba(&frame, crop)?;
        input.write_all(&rgba).map_err(|e| format!("FFmpeg 寫入影格失敗：{e}"))?;
        frame_count += 1;
    }
    drop(input);
    let output = child.wait_with_output().map_err(|e| format!("等待 FFmpeg 結束失敗：{e}"))?;
    if !output.status.success() {
        return Err(format!("FFmpeg 編碼失敗：{}", String::from_utf8_lossy(&output.stderr)));
    }
    if frame_count == 0 || !video_path.exists() {
        return Err("FFmpeg 未產生有效影片".to_string());
    }
    let microphone = audio.map(AudioCapture::finish).transpose()?;
    let system = system_audio.map(SystemAudioCapture::finish).transpose()?;
    if let Some(samples) = mix_audio_samples(microphone, system) {
        if !samples.samples.is_empty() {
            let wav_path = path.with_extension("audio.wav");
            write_pcm_wav(&wav_path, &samples)?;
            let muxed_path = path.with_extension("muxed.mp4");
            let mux = Command::new(&ffmpeg).args(["-hide_banner", "-loglevel", "error", "-i"]).arg(&video_path).args(["-i"]).arg(&wav_path).args(["-c:v", "copy", "-c:a", "aac", "-shortest", "-y"]).arg(&muxed_path).output().map_err(|e| format!("啟動 FFmpeg 音訊合併失敗：{e}"))?;
            if !mux.status.success() { return Err(format!("FFmpeg 音訊合併失敗：{}", String::from_utf8_lossy(&mux.stderr))); }
            std::fs::rename(&muxed_path, &path).map_err(|e| format!("替換含音訊影片失敗：{e}"))?;
            let _ = std::fs::remove_file(wav_path);
            let _ = std::fs::remove_file(&video_path);
        } else { std::fs::rename(&video_path, &path).map_err(|e| format!("完成影片輸出失敗：{e}"))?; }
    } else { std::fs::rename(&video_path, &path).map_err(|e| format!("完成影片輸出失敗：{e}"))?; }
    Ok(RecordingResult { path: path.to_string_lossy().into_owned(), frame_count, width: crop.width, height: crop.height })
}

#[cfg(target_os = "windows")]
fn write_pcm_wav(path: &PathBuf, samples: &AudioSamples) -> Result<(), String> {
    let channels = samples.channels.max(1);
    let data_len = samples.samples.len().saturating_mul(2) as u32;
    let byte_rate = samples.sample_rate.saturating_mul(channels as u32).saturating_mul(2);
    let block_align = channels.saturating_mul(2);
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF"); bytes.extend_from_slice(&(36u32.saturating_add(data_len)).to_le_bytes()); bytes.extend_from_slice(b"WAVEfmt "); bytes.extend_from_slice(&16u32.to_le_bytes()); bytes.extend_from_slice(&1u16.to_le_bytes()); bytes.extend_from_slice(&channels.to_le_bytes()); bytes.extend_from_slice(&samples.sample_rate.to_le_bytes()); bytes.extend_from_slice(&byte_rate.to_le_bytes()); bytes.extend_from_slice(&block_align.to_le_bytes()); bytes.extend_from_slice(&16u16.to_le_bytes()); bytes.extend_from_slice(b"data"); bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in &samples.samples { bytes.extend_from_slice(&((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes()); }
    std::fs::write(path, bytes).map_err(|e| format!("寫入 WAV 音訊失敗：{e}"))
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
    use crate::frame_source::CaptureRegion;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn accepts_only_supported_scroll_modes() {
        assert!(validate_scroll_mode("auto").is_ok());
        assert!(validate_scroll_mode("manual").is_ok());
        assert!(validate_scroll_mode("unexpected").is_err());
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
