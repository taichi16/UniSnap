use std::io::{Seek, Write};

use mp4::{Bytes, Mp4Sample, Mp4Writer};
use rusty_aac::{AacEncoder, AacEncoderConfig};

use crate::audio_capture::AudioCapture;

/// Encodes microphone PCM and appends it as MP4 track 2.
pub fn write_microphone_track<W: Write + Seek>(
    writer: &mut Mp4Writer<W>,
    capture: AudioCapture,
) -> Result<(), String> {
    drop(capture.stream);
    let samples = capture
        .samples
        .lock()
        .map_err(|_| "無法讀取麥克風資料".to_string())?
        .clone();
    if samples.is_empty() {
        eprintln!("[record] microphone produced no samples; video saved without usable audio");
        return Ok(());
    }
    let sample_rate = capture.sample_rate;
    let channels = capture.channels;
    let mut encoder = AacEncoder::new(AacEncoderConfig {
        bitrate_bps: 128_000,
        ..Default::default()
    });
    encoder
        .push_pcm(&samples, channels, sample_rate)
        .map_err(|e| format!("AAC 編碼失敗：{e}"))?;
    encoder.finish();
    let mut audio_sample_index = 0u64;
    while let Ok(packet) = encoder.next_packet() {
        let duration = ((packet.duration as u64 * 1_000) / sample_rate as u64).max(1);
        writer
            .write_sample(
                2,
                &Mp4Sample {
                    start_time: audio_sample_index,
                    duration: duration as u32,
                    rendering_offset: 0,
                    is_sync: true,
                    bytes: Bytes::from(packet.data),
                },
            )
            .map_err(|e| format!("寫入 MP4 麥克風音訊失敗：{e}"))?;
        audio_sample_index += duration;
    }
    eprintln!("[record] microphone samples={} rate={} channels={}", samples.len(), sample_rate, channels);
    Ok(())
}
