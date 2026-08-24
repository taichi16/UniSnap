use mp4::{ChannelConfig, SampleFreqIndex};

use crate::audio_capture::AudioCapture;

pub struct AudioTrackInfo {
    pub freq_index: SampleFreqIndex,
    pub channel_config: ChannelConfig,
}

/// Validates microphone settings before an AAC track is added to the MP4.
pub fn audio_track_info(audio: Option<&AudioCapture>) -> Result<Option<AudioTrackInfo>, String> {
    let Some(capture) = audio else {
        return Ok(None);
    };
    let freq_index = match capture.sample_rate {
        96_000 => SampleFreqIndex::Freq96000,
        88_200 => SampleFreqIndex::Freq88200,
        64_000 => SampleFreqIndex::Freq64000,
        48_000 => SampleFreqIndex::Freq48000,
        44_100 => SampleFreqIndex::Freq44100,
        32_000 => SampleFreqIndex::Freq32000,
        24_000 => SampleFreqIndex::Freq24000,
        22_050 => SampleFreqIndex::Freq22050,
        16_000 => SampleFreqIndex::Freq16000,
        12_000 => SampleFreqIndex::Freq12000,
        11_025 => SampleFreqIndex::Freq11025,
        8_000 => SampleFreqIndex::Freq8000,
        7_350 => SampleFreqIndex::Freq7350,
        rate => return Err(format!("麥克風取樣率 {rate} Hz 無法封裝為 MP4 AAC 音軌")),
    };
    let channel_config = if capture.channels == 1 {
        ChannelConfig::Mono
    } else {
        ChannelConfig::Stereo
    };
    Ok(Some(AudioTrackInfo { freq_index, channel_config }))
}
