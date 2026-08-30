use mp4::{AvcConfig, FourCC, Mp4Config, TrackConfig};

pub fn default_mp4_config() -> Result<Mp4Config, String> {
    Ok(Mp4Config {
        major_brand: "isom".parse::<FourCC>().map_err(|e| e.to_string())?,
        minor_version: 512,
        compatible_brands: ["isom", "iso2", "avc1", "mp41"]
            .into_iter()
            .map(|s| s.parse::<FourCC>().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?,
        timescale: 1_000,
    })
}

pub fn h264_video_track(
    width: u32,
    height: u32,
    sps: Vec<u8>,
    pps: Vec<u8>,
) -> TrackConfig {
    TrackConfig::from(AvcConfig {
        width: width as u16,
        height: height as u16,
        seq_param_set: sps,
        pic_param_set: pps,
    })
}
