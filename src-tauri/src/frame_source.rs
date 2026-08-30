use std::sync::mpsc;
use std::time::Duration;

use xcap::{Frame, Monitor, VideoRecorder};

/// Owns the active frame producer for video and scrolling captures.
#[allow(dead_code)]
pub enum FrameSource {
    Receiver(mpsc::Receiver<Frame>),
    Native {
        recorder: VideoRecorder,
        receiver: mpsc::Receiver<Frame>,
    },
    MonitorRegion {
        monitor: Monitor,
        region: CaptureRegion,
    },
}

#[derive(Clone, Copy)]
pub struct CaptureRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl FrameSource {
    pub fn next_frame(&self, timeout: Duration) -> Result<Option<Frame>, String> {
        match self {
            Self::Receiver(receiver) | Self::Native { receiver, .. } => {
                match receiver.recv_timeout(timeout) {
                    Ok(frame) => Ok(Some(frame)),
                    Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => Ok(None),
                }
            }
            Self::MonitorRegion { monitor, region } => {
                let image = monitor
                    .capture_region(region.x, region.y, region.width, region.height)
                    .map_err(|e| format!("擷取錄影區域影格失敗：{e}"))?;
                Ok(Some(Frame::new(image.width(), image.height(), image.into_raw())))
            }
        }
    }
}

impl Drop for FrameSource {
    fn drop(&mut self) {
        if let Self::Native { recorder, .. } = self {
            let _ = recorder.stop();
        }
    }
}
