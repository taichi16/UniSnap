use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use mp4::Mp4Writer;
use openh264::encoder::{BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, UsageType};
use openh264::OpenH264API;

use crate::mp4_config::default_mp4_config;
use crate::recording_crop::Crop;

pub fn create_h264_encoder(fps: u32, crop: Crop) -> Result<Encoder, String> {
    let config = EncoderConfig::new()
        .usage_type(UsageType::ScreenContentRealTime)
        .max_frame_rate(FrameRate::from_hz(fps as f32))
        .bitrate(BitRate::from_bps(
            (crop.width * crop.height * fps / 8).clamp(2_000_000, 20_000_000),
        ))
        .intra_frame_period(IntraFramePeriod::from_num_frames(fps * 2))
        .skip_frames(false);
    Encoder::with_api_config(OpenH264API::from_source(), config)
        .map_err(|e| format!("建立 H.264 編碼器失敗：{e}"))
}

pub fn create_mp4_writer(path: &Path) -> Result<Mp4Writer<BufWriter<File>>, String> {
    let file = File::create(path).map_err(|e| format!("建立 MP4 檔案失敗：{e}"))?;
    let config = default_mp4_config()?;
    Mp4Writer::write_start(BufWriter::new(file), &config)
        .map_err(|e| format!("初始化 MP4 失敗：{e}"))
}
