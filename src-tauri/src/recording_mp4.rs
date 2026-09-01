use std::io::{Seek, Write};

use mp4::{
    AacConfig, AvcConfig, Bytes, ChannelConfig, Mp4Sample, Mp4Writer, SampleFreqIndex, TrackConfig,
};

pub fn add_video_track<W: Write + Seek>(
    writer: &mut Mp4Writer<W>,
    width: u16,
    height: u16,
    sps: Vec<u8>,
    pps: Vec<u8>,
) -> Result<(), String> {
    writer
        .add_track(&TrackConfig::from(AvcConfig {
            width,
            height,
            seq_param_set: sps,
            pic_param_set: pps,
        }))
        .map_err(|e| format!("建立 MP4 視訊軌失敗：{e}"))
}

pub fn add_audio_track<W: Write + Seek>(
    writer: &mut Mp4Writer<W>,
    sample_rate: SampleFreqIndex,
    channels: ChannelConfig,
    sample_rate_hz: u32,
) -> Result<(), String> {
    let mut track_config = TrackConfig::from(AacConfig {
        bitrate: 128_000,
        profile: mp4::AudioObjectType::AacLowComplexity,
        freq_index: sample_rate,
        chan_conf: channels,
    });
    // AAC packet durations are counts of PCM frames (normally 1024), not
    // integer milliseconds. A 1 kHz track clock truncates every 44.1 kHz
    // packet and accumulates visible A/V drift, so use the real sample rate.
    track_config.timescale = sample_rate_hz.max(1);
    writer
        .add_track(&track_config)
        .map_err(|e| format!("建立 MP4 麥克風音軌失敗：{e}"))
}

pub fn write_sample<W: Write + Seek>(
    writer: &mut Mp4Writer<W>,
    track_id: u32,
    start_time: u64,
    duration: u32,
    is_sync: bool,
    bytes: Bytes,
) -> Result<(), String> {
    writer
        .write_sample(
            track_id,
            &Mp4Sample {
                start_time,
                duration,
                rendering_offset: 0,
                is_sync,
                bytes,
            },
        )
        .map_err(|e| format!("寫入 MP4 sample 失敗：{e}"))
}

#[cfg(test)]
mod tests {
    use super::write_sample;
    use mp4::{Bytes, FourCC, Mp4Config, Mp4Writer};
    use std::io::Cursor;

    #[test]
    fn exposes_sample_writer_as_a_single_error_boundary() {
        let config = Mp4Config {
            major_brand: "isom".parse::<FourCC>().unwrap(),
            minor_version: 512,
            compatible_brands: vec!["isom".parse::<FourCC>().unwrap()],
            timescale: 1_000,
        };
        let mut writer = Mp4Writer::write_start(Cursor::new(Vec::new()), &config).unwrap();
        let error = write_sample(&mut writer, 0, 0, 1, true, Bytes::from(vec![1])).unwrap_err();
        assert!(error.contains("MP4 sample"));
    }
}
