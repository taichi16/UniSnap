use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use xcap::Frame;

use crate::frame_source::FrameSource;

/// Returns the next frame eligible for the configured FPS cadence.
pub fn next_timed_frame(
    frame_source: &FrameSource,
    pending: &mut Option<Frame>,
    stop: &AtomicBool,
    next_frame_at: &mut Instant,
    interval: Duration,
) -> Result<Option<Frame>, String> {
    if stop.load(Ordering::SeqCst) && pending.is_none() {
        return Ok(None);
    }
    let frame = if let Some(frame) = pending.take() {
        frame
    } else {
        match frame_source.next_frame(Duration::from_millis(80))? {
            Some(frame) => frame,
            None if stop.load(Ordering::SeqCst) => return Ok(None),
            None => return Ok(None),
        }
    };
    if Instant::now() < *next_frame_at {
        return Ok(None);
    }
    *next_frame_at = Instant::now() + interval;
    Ok(Some(frame))
}
