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
) -> Result<(), String> {
    writer
        .add_track(&TrackConfig::from(AacConfig {
            bitrate: 128_000,
            profile: mp4::AudioObjectType::AacLowComplexity,
            freq_index: sample_rate,
            chan_conf: channels,
        }))
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

pub fn aac_packet_duration_ms(packet_duration: u32, sample_rate: u32) -> u32 {
    if sample_rate == 0 {
        return 1;
    }
    ((packet_duration as u64 * 1_000) / sample_rate as u64).max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::{aac_packet_duration_ms, write_sample};
    use mp4::{Bytes, FourCC, Mp4Config, Mp4Writer};
    use std::io::Cursor;

    #[test]
    fn converts_aac_packet_duration_to_milliseconds() {
        assert_eq!(aac_packet_duration_ms(1024, 48_000), 21);
    }

    #[test]
    fn protects_against_zero_sample_rate() {
        assert_eq!(aac_packet_duration_ms(1024, 0), 1);
    }

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
