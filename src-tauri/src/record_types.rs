#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    pub path: String,
    pub frame_count: u64,
    pub width: u32,
    pub height: u32,
}

/// Shared state for the single active recording session.
pub struct RecordingState(pub Mutex<Option<RecordingSession>>);

pub struct RecordingSession {
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) finished: mpsc::Receiver<Result<RecordingResult, String>>,
    pub(crate) lifecycle: Arc<Mutex<crate::record_session::RecordingLifecycle>>,
}

#[derive(serde::Deserialize)]
pub struct ScrollConfig {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scroll_amount: Option<i32>,
    pub monitor_offset_x: Option<i32>,
    pub monitor_offset_y: Option<i32>,
}
use std::sync::{atomic::AtomicBool, mpsc, Arc, Mutex};
