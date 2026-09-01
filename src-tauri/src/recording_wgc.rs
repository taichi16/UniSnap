use std::sync::{
    atomic::{AtomicBool, AtomicI64, Ordering},
    Arc,
};
use std::time::Duration;

use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
    VideoSettingsSubType,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use crate::recording_crop::Crop;

struct CaptureFlags {
    path: String,
    crop: Crop,
    fps: u32,
    accept_frames: Arc<AtomicBool>,
    first_frame_timestamp_100ns: Arc<AtomicI64>,
}

struct RegionCaptureHandler {
    encoder: Option<VideoEncoder>,
    crop: Crop,
    submitted_frames: u64,
    accept_frames: Arc<AtomicBool>,
    first_frame_timestamp_100ns: Arc<AtomicI64>,
}

impl GraphicsCaptureApiHandler for RegionCaptureHandler {
    type Flags = CaptureFlags;
    type Error = String;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let flags = context.flags;
        let bitrate = ((flags.crop.width as u64 * flags.crop.height as u64 * flags.fps as u64) / 8)
            .clamp(6_000_000, 20_000_000) as u32;
        let encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(flags.crop.width, flags.crop.height)
                .sub_type(VideoSettingsSubType::H264)
                .frame_rate(flags.fps)
                .bitrate(bitrate)
                // GPU capture/cropping remains enabled. Only vendor-specific
                // video encoders are disabled so old display drivers cannot
                // terminate the UniSnap process.
                .hardware_acceleration(false),
            AudioSettingsBuilder::default().disabled(true),
            ContainerSettingsBuilder::default(),
            &flags.path,
        )
        .map_err(|error| format!("建立 Windows 原生影片編碼器失敗：{error}"))?;
        Ok(Self {
            encoder: Some(encoder),
            crop: flags.crop,
            submitted_frames: 0,
            accept_frames: flags.accept_frames,
            first_frame_timestamp_100ns: flags.first_frame_timestamp_100ns,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        // Build the WGC and MediaTranscoder pipeline first, but do not define
        // video time zero until the audio sources are also ready.
        if !self.accept_frames.load(Ordering::Acquire) {
            return Ok(());
        }
        let frame_timestamp = frame
            .timestamp()
            .map_err(|error| format!("讀取 Windows 擷取影格時間戳失敗：{error}"))?
            .Duration;
        let _ = self.first_frame_timestamp_100ns.compare_exchange(
            -1,
            frame_timestamp,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if self.crop.x + self.crop.width > frame.width()
            || self.crop.y + self.crop.height > frame.height()
        {
            return Err(format!(
                "Windows 擷取畫面尺寸變更為 {}x{}，無法套用 {}x{} 區域",
                frame.width(),
                frame.height(),
                self.crop.width,
                self.crop.height
            ));
        }
        self.encoder
            .as_mut()
            .ok_or_else(|| "Windows 原生影片編碼器已停止".to_string())?
            .send_frame_cropped(frame, self.crop.x, self.crop.y)
            .map_err(|error| format!("寫入 Windows 原生錄影影格失敗：{error}"))?;
        self.submitted_frames += 1;
        Ok(())
    }
}

pub struct WgcRecording {
    control: CaptureControl<RegionCaptureHandler, String>,
    accept_frames: Arc<AtomicBool>,
    first_frame_timestamp_100ns: Arc<AtomicI64>,
}

pub struct WgcRecordingResult {
    pub submitted_frames: u64,
    pub first_frame_timestamp_100ns: i64,
}

impl WgcRecording {
    pub fn start(
        monitor_device_name: &str,
        crop: Crop,
        fps: u32,
        path: &std::path::Path,
    ) -> Result<Self, String> {
        let monitors = Monitor::enumerate()
            .map_err(|error| format!("無法列出 Windows Graphics Capture 螢幕：{error}"))?;
        let monitor = monitors
            .into_iter()
            .find(|monitor| monitor.device_name().ok().as_deref() == Some(monitor_device_name))
            .ok_or_else(|| format!("Windows Graphics Capture 找不到螢幕 {monitor_device_name}"))?;
        let accept_frames = Arc::new(AtomicBool::new(false));
        let first_frame_timestamp_100ns = Arc::new(AtomicI64::new(-1));
        let settings = Settings::new(
            monitor,
            CursorCaptureSettings::WithCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Exclude,
            MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(1.0 / fps.max(1) as f64)),
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            CaptureFlags {
                path: path.to_string_lossy().into_owned(),
                crop,
                fps,
                accept_frames: Arc::clone(&accept_frames),
                first_frame_timestamp_100ns: Arc::clone(&first_frame_timestamp_100ns),
            },
        );
        let control = RegionCaptureHandler::start_free_threaded(settings)
            .map_err(|error| format!("啟動 Windows Graphics Capture 失敗：{error}"))?;
        Ok(Self {
            control,
            accept_frames,
            first_frame_timestamp_100ns,
        })
    }

    pub fn begin(&self) {
        self.accept_frames.store(true, Ordering::Release);
    }

    pub fn wait_and_finish(self, stop: Arc<AtomicBool>) -> Result<WgcRecordingResult, String> {
        while !stop.load(Ordering::SeqCst) && !self.control.is_finished() {
            std::thread::sleep(Duration::from_millis(15));
        }

        let callback = self.control.callback();
        let capture_result = if self.control.is_finished() {
            self.control.wait().map_err(|error| error.to_string())
        } else {
            self.control.stop().map_err(|error| error.to_string())
        };

        let mut handler = callback.lock();
        let frame_count = handler.submitted_frames;
        let finish_result = handler
            .encoder
            .take()
            .ok_or_else(|| "Windows 原生影片編碼器未啟動".to_string())?
            .finish()
            .map_err(|error| format!("完成 Windows 原生影片失敗：{error}"));
        drop(handler);

        finish_result?;
        capture_result.map_err(|error| format!("停止 Windows Graphics Capture 失敗：{error}"))?;
        if frame_count == 0 {
            return Err("錄影期間未收到 Windows Graphics Capture 影格".to_string());
        }
        let first_frame_timestamp_100ns = self.first_frame_timestamp_100ns.load(Ordering::Acquire);
        if first_frame_timestamp_100ns < 0 {
            return Err("Windows Graphics Capture 未提供第一個影格時間戳".to_string());
        }
        Ok(WgcRecordingResult {
            submitted_frames: frame_count,
            first_frame_timestamp_100ns,
        })
    }
}
