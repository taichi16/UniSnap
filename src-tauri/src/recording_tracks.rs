use std::io::{Seek, Write};

use mp4::{AacConfig, Mp4Writer, TrackConfig};

use crate::recording_audio::AudioTrackInfo;
use crate::mp4_config::h264_video_track;

/// Adds video track 1 and optional microphone track 2 in the stable order.
pub fn add_recording_tracks<W: Write + Seek>(
    writer: &mut Mp4Writer<W>,
    width: u32,
    height: u32,
    sps: Vec<u8>,
    pps: Vec<u8>,
    audio_info: Option<&AudioTrackInfo>,
) -> Result<(), String> {
    writer
        .add_track(&h264_video_track(width, height, sps, pps))
        .map_err(|e| format!("建立 MP4 視訊軌失敗：{e}"))?;
    // Track 1 remains video and track 2 remains microphone audio.
    if let Some(audio_info) = audio_info {
        writer
            .add_track(&TrackConfig::from(AacConfig {
                bitrate: 128_000,
                profile: mp4::AudioObjectType::AacLowComplexity,
                freq_index: audio_info.freq_index,
                chan_conf: audio_info.channel_config,
            }))
            .map_err(|e| format!("建立 MP4 麥克風音軌失敗：{e}"))?;
    }
    Ok(())
}
