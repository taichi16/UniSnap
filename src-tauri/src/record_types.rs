#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    pub path: String,
    pub frame_count: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(serde::Deserialize)]
pub struct ScrollConfig {
    pub mode: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scroll_amount: Option<i32>,
    pub monitor_offset_x: Option<i32>,
    pub monitor_offset_y: Option<i32>,
}
