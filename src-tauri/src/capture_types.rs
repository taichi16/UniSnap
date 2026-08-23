#[derive(serde::Serialize, Clone, Debug)]
pub struct MonitorBasicInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct MonitorScreenshot {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub base64_image: String,
}
