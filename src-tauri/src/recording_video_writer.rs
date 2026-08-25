use std::io::{Seek, Write};

use mp4::{Bytes, Mp4Sample, Mp4Writer};

pub type PendingVideoSample = (u64, bool, Bytes);

/// Flushes one delayed sample and reports whether a sample was written.
/// Delaying one sample lets the encoder derive its duration from the next
/// timestamp while keeping the MP4 writer details in this module.
pub fn flush_pending_sample<W: Write + Seek>(
    writer: &mut Mp4Writer<W>,
    pending: &mut Option<PendingVideoSample>,
    duration: u32,
    final_sample: bool,
) -> Result<bool, String> {
    let Some(sample) = pending.take() else {
        return Ok(false);
    };
    write_video_sample(writer, sample, duration, final_sample)?;
    Ok(true)
}

pub fn write_video_sample<W: Write + Seek>(
    writer: &mut Mp4Writer<W>,
    sample: PendingVideoSample,
    duration: u32,
    final_sample: bool,
) -> Result<(), String> {
    let (timestamp, is_sync, bytes) = sample;
    writer
        .write_sample(
            1,
            &Mp4Sample {
                start_time: timestamp,
                duration,
                rendering_offset: 0,
                is_sync,
                bytes,
            },
        )
        .map_err(|e| {
            if final_sample {
                format!("寫入最後一個 MP4 影格失敗：{e}")
            } else {
                format!("寫入 MP4 影格失敗：{e}")
            }
        })
}
