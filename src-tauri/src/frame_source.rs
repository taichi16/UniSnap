use std::sync::{mpsc, Mutex};
use std::time::Duration;

use xcap::{Frame, Monitor, VideoRecorder};

#[allow(dead_code)]
pub enum FrameSource {
    Receiver(mpsc::Receiver<Frame>),
    Monitor(Monitor),
    Native {
        recorder: VideoRecorder,
        receiver: mpsc::Receiver<Frame>,
        last_frame: Mutex<Option<Frame>>,
    },
    /// Fallback for Windows systems where DXGI video capture is unavailable.
    GdiDesktop {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        fallback_monitor: Monitor,
    },
    MonitorRegion {
        monitor: Monitor,
        region: CaptureRegion,
    },
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct CaptureRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub canvas_width: u32,
    pub canvas_height: u32,
}

impl FrameSource {
    pub fn next_frame(&self, timeout: Duration) -> Result<Option<Frame>, String> {
        match self {
            Self::Receiver(receiver) => match receiver.recv_timeout(timeout) {
                Ok(frame) => Ok(Some(frame)),
                Err(mpsc::RecvTimeoutError::Timeout)
                | Err(mpsc::RecvTimeoutError::Disconnected) => Ok(None),
            },
            Self::Native {
                receiver,
                last_frame,
                ..
            } => {
                if let Ok(frame) = receiver.recv_timeout(timeout) {
                    let mut cached = last_frame
                        .lock()
                        .map_err(|_| "Frame cache poisoned".to_string())?;
                    *cached = Some(frame.clone());
                    return Ok(Some(frame));
                }
                Ok(last_frame
                    .lock()
                    .map_err(|_| "Frame cache poisoned".to_string())?
                    .clone())
            }
            Self::Monitor(monitor) => {
                let image = monitor
                    .capture_image()
                    .map_err(|e| format!("擷取錄影影格失敗：{e}"))?;
                Ok(Some(Frame::new(
                    image.width(),
                    image.height(),
                    image.into_raw(),
                )))
            }
            Self::MonitorRegion { monitor, region } => {
                let image = monitor
                    .capture_region(region.x, region.y, region.width, region.height)
                    .map_err(|e| format!("擷取錄影區域影格失敗：{e}"))?;
                Ok(Some(Frame::new(
                    image.width(),
                    image.height(),
                    image.into_raw(),
                )))
            }
            Self::GdiDesktop {
                x,
                y,
                width,
                height,
                fallback_monitor,
            } => {
                let image = crate::capture::capture_monitor_gdi(*x, *y, *width, *height)
                    .or_else(|gdi_error| {
                        fallback_monitor.capture_image().map_err(|xcap_error| {
                            format!("Desktop capture failed (GDI: {gdi_error}; DXGI fallback: {xcap_error})")
                        })
                    })?;
                Ok(Some(Frame::new(
                    image.width(),
                    image.height(),
                    image.into_raw(),
                )))
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
