use std::fs::{self, File};
use std::io::BufWriter;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mp4::{
    AacConfig, AvcConfig, Bytes, ChannelConfig, FourCC, Mp4Config, Mp4Sample, Mp4Writer,
    SampleFreqIndex, TrackConfig,
};
use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, UsageType,
};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
use crate::record_types::{RecordingResult, ScrollConfig};
use crate::audio_capture::{start_audio_capture, AudioCapture};
use crate::frame_source::{CaptureRegion, FrameSource};
use openh264::OpenH264API;
use rusty_aac::{AacEncoder, AacEncoderConfig};
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

pub struct RecordingState(pub Mutex<Option<RecordingSession>>);

pub struct RecordingSession {
    stop: Arc<AtomicBool>,
    finished: mpsc::Receiver<Result<RecordingResult, String>>,
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

#[derive(Clone, Copy)]
struct Crop {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn resolve_crop(
    frame: &Frame,
    coordinate_scale: f64,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<Crop, String> {
    let px = ((x.max(0) as f64) * coordinate_scale).round() as u32;
    let py = ((y.max(0) as f64) * coordinate_scale).round() as u32;
    if px >= frame.width || py >= frame.height {
        return Err("錄影選取範圍位於螢幕外".into());
    }
    let mut pw = ((width as f64) * coordinate_scale).round() as u32;
    let mut ph = ((height as f64) * coordinate_scale).round() as u32;
    pw = pw.min(frame.width - px) & !1;
    ph = ph.min(frame.height - py) & !1;
    if pw < 2 || ph < 2 {
        return Err("錄影選取範圍太小".into());
    }
    if pw > u16::MAX as u32 || ph > u16::MAX as u32 {
        return Err("錄影解析度超過 MP4 支援範圍".into());
    }
    Ok(Crop {
        x: px,
        y: py,
        width: pw,
        height: ph,
    })
}

fn recording_output_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config = crate::config::load_config(app.clone()).unwrap_or_default();
    let directory = PathBuf::from(config.save_directory);
    fs::create_dir_all(&directory).map_err(|e| format!("無法建立錄影存檔資料夾：{e}"))?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    Ok(directory.join(format!("ScreenRec_{millis}.mp4")))
}

fn crop_rgba(frame: &Frame, crop: Crop) -> Result<Vec<u8>, String> {
    let expected = frame.width as usize * frame.height as usize * 4;
    if frame.raw.len() < expected {
        return Err("收到的螢幕影格資料不完整".into());
    }
    // Region capture already returns the requested rectangle. The first
    // startup frame is still a full-monitor image and uses original offsets.
    let crop = if crop.x + crop.width > frame.width || crop.y + crop.height > frame.height {
        Crop {
            x: 0,
            y: 0,
            width: frame.width,
            height: frame.height,
        }
    } else {
        crop
    };
    let row_bytes = crop.width as usize * 4;
    let mut out = Vec::with_capacity(row_bytes * crop.height as usize);
    for row in crop.y..crop.y + crop.height {
        let start = ((row * frame.width + crop.x) * 4) as usize;
        out.extend_from_slice(&frame.raw[start..start + row_bytes]);
    }
    Ok(out)
}

fn strip_start_code(nal: &[u8]) -> &[u8] {
    if nal.starts_with(&[0, 0, 0, 1]) {
        &nal[4..]
    } else if nal.starts_with(&[0, 0, 1]) {
        &nal[3..]
    } else {
        nal
    }
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
    let audio_info = audio.as_ref().map(|capture| {
        let freq_index = match capture.sample_rate {
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
        let chan_conf = if capture.channels == 1 {
            ChannelConfig::Mono
        } else {
            ChannelConfig::Stereo
        };
        (capture.sample_rate, capture.channels, freq_index, chan_conf)
    });
    let frame_duration = 1_000 / fps;
    let interval = Duration::from_secs_f64(1.0 / fps as f64);
    let recording_started_at = Instant::now();
    let mut next_frame_at = Instant::now();
    let mut pending = Some(first);
    let mut pending_sample: Option<(u64, bool, Bytes)> = None;
    let mut track_added = false;
    let mut frame_count = 0u64;

    loop {
        if stop.load(Ordering::SeqCst) && pending.is_none() {
            break;
        }
        let frame = if let Some(frame) = pending.take() {
            frame
        } else {
            match frame_source.next_frame(Duration::from_millis(80))? {
                Some(frame) => frame,
                None if stop.load(Ordering::SeqCst) => break,
                None => continue,
            }
        };
        if Instant::now() < next_frame_at {
            continue;
        }
        next_frame_at = Instant::now() + interval;
        let rgba = crop_rgba(&frame, crop)?;
        let yuv = YUVBuffer::from_rgb_source(RgbaSliceU8::new(
            &rgba,
            (crop.width as usize, crop.height as usize),
        ));
        let encoded = encoder
            .encode(&yuv)
            .map_err(|e| format!("H.264 編碼失敗：{e}"))?;
        let frame_type = encoded.frame_type();
        if matches!(frame_type, FrameType::Skip | FrameType::Invalid) {
            continue;
        }
        let mut sps = None;
        let mut pps = None;
        let mut sample = Vec::new();
        for layer_index in 0..encoded.num_layers() {
            let layer = encoded.layer(layer_index).ok_or("無法讀取 H.264 圖層")?;
            for nal_index in 0..layer.nal_count() {
                let nal = strip_start_code(layer.nal_unit(nal_index).ok_or("無法讀取 H.264 NAL")?);
                if nal.is_empty() {
                    continue;
                }
                match nal[0] & 0x1f {
                    7 => sps = Some(nal.to_vec()),
                    8 => pps = Some(nal.to_vec()),
                    _ => {
                        sample.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                        sample.extend_from_slice(nal);
                    }
                }
            }
        }
        if !track_added {
            writer
                .add_track(&TrackConfig::from(AvcConfig {
                    width: crop.width as u16,
                    height: crop.height as u16,
                    seq_param_set: sps.ok_or("第一個 H.264 影格缺少 SPS")?,
                    pic_param_set: pps.ok_or("第一個 H.264 影格缺少 PPS")?,
                }))
                .map_err(|e| format!("建立 MP4 視訊軌失敗：{e}"))?;
            track_added = true;
            // Track 1 is deliberately the video track because all video
            // samples below are written to track 1. Add audio second so it
            // receives track 2 and cannot silently swap the two streams.
            if let Some((_, _, freq_index, chan_conf)) = audio_info {
                writer
                    .add_track(&TrackConfig::from(AacConfig {
                        bitrate: 128_000,
                        profile: mp4::AudioObjectType::AacLowComplexity,
                        freq_index,
                        chan_conf,
                    }))
                    .map_err(|e| format!("建立 MP4 麥克風音軌失敗：{e}"))?;
            }
        }
        if !sample.is_empty() {
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
                writer
                    .write_sample(
                        1,
                        &Mp4Sample {
                            start_time: previous_timestamp,
                            duration,
                            rendering_offset: 0,
                            is_sync: previous_sync,
                            bytes: previous_bytes,
                        },
                    )
                    .map_err(|e| format!("寫入 MP4 影格失敗：{e}"))?;
                frame_count += 1;
            }
            pending_sample = Some((
                timestamp,
                matches!(frame_type, FrameType::IDR | FrameType::I),
                Bytes::from(sample),
            ));
        }
    }
    if let Some((timestamp, is_sync, bytes)) = pending_sample.take() {
        let stopped_at = recording_started_at.elapsed().as_millis() as u64;
        let duration = stopped_at
            .saturating_sub(timestamp)
            .max(frame_duration as u64)
            .clamp(1, u32::MAX as u64) as u32;
        writer
            .write_sample(
                1,
                &Mp4Sample {
                    start_time: timestamp,
                    duration,
                    rendering_offset: 0,
                    is_sync,
                    bytes,
                },
            )
            .map_err(|e| format!("寫入最後一個 MP4 影格失敗：{e}"))?;
        frame_count += 1;
    }
    if frame_count == 0 {
        let _ = fs::remove_file(&path);
        return Err("錄影期間未產生可儲存影格".into());
    }
    if let Some(capture) = audio {
        drop(capture.stream);
        let samples = capture
            .samples
            .lock()
            .map_err(|_| "無法讀取麥克風資料".to_string())?
            .clone();
        if !samples.is_empty() {
            let sample_rate = capture.sample_rate;
            let channels = capture.channels;
            let mut encoder = AacEncoder::new(AacEncoderConfig {
                bitrate_bps: 128_000,
                ..Default::default()
            });
            encoder
                .push_pcm(&samples, channels, sample_rate)
                .map_err(|e| format!("AAC 編碼失敗：{e}"))?;
            encoder.finish();
            let mut audio_sample_index = 0u64;
            while let Ok(packet) = encoder.next_packet() {
                let bytes = Bytes::from(packet.data);
                writer
                    .write_sample(
                        2,
                        &Mp4Sample {
                            start_time: audio_sample_index,
                            duration: ((packet.duration as u64 * 1_000) / sample_rate as u64).max(1)
                                as u32,
                            rendering_offset: 0,
                            is_sync: true,
                            bytes,
                        },
                    )
                    .map_err(|e| format!("寫入 MP4 麥克風音訊失敗：{e}"))?;
                audio_sample_index +=
                    ((packet.duration as u64 * 1_000) / sample_rate as u64).max(1);
            }
            eprintln!(
                "[record] microphone samples={} rate={} channels={}",
                samples.len(),
                sample_rate,
                channels
            );
        } else {
            eprintln!("[record] microphone produced no samples; video saved without usable audio");
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

#[tauri::command]
pub fn trigger_scroll_capture(config: ScrollConfig) -> Result<String, String> {
    let offset_x = config.monitor_offset_x.unwrap_or(0);
    let offset_y = config.monitor_offset_y.unwrap_or(0);

    let cx = (offset_x + config.x + (config.width as i32 / 2)) as f64;
    let cy = (offset_y + config.y + (config.height as i32 / 2)) as f64;

    let amount = config.scroll_amount.unwrap_or(5);

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
