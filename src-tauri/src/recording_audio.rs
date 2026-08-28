use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use rusty_aac::{AacEncoder, AacEncoderConfig};

pub struct AudioCapture {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    pub channels: u16,
    pub sample_rate: u32,
}

pub struct AudioSamples {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

pub struct EncodedAudioPacket {
    pub bytes: Vec<u8>,
    pub duration: u32,
}

/// Merge microphone and system audio into the common AAC input format.
///
/// WASAPI loopback is normalized to 44.1 kHz stereo. When both sources are
/// enabled, the microphone is resampled with nearest-frame selection and
/// mono input is duplicated to stereo. The mixer is intentionally bounded to
/// the longer source and clamps the sum to avoid integer-style clipping.
pub fn mix_audio_samples(
    microphone: Option<AudioSamples>,
    system_audio: Option<AudioSamples>,
) -> Option<AudioSamples> {
    match (microphone, system_audio) {
        (None, None) => None,
        (Some(samples), None) | (None, Some(samples)) => Some(samples),
        (Some(microphone), Some(system_audio)) => {
            let microphone = resample_to_stereo_44100(&microphone);
            let system_audio = resample_to_stereo_44100(&system_audio);
            let frame_count = (microphone.samples.len() / 2).max(system_audio.samples.len() / 2);
            let mut samples = vec![0.0; frame_count * 2];
            for frame in 0..frame_count {
                for channel in 0..2 {
                    let mic = microphone
                        .samples
                        .get(frame * 2 + channel)
                        .copied()
                        .unwrap_or(0.0);
                    let system = system_audio
                        .samples
                        .get(frame * 2 + channel)
                        .copied()
                        .unwrap_or(0.0);
                    samples[frame * 2 + channel] = (mic + system).clamp(-1.0, 1.0);
                }
            }
            Some(AudioSamples {
                samples,
                channels: 2,
                sample_rate: 44_100,
            })
        }
    }
}

fn resample_to_stereo_44100(source: &AudioSamples) -> AudioSamples {
    let source_channels = source.channels.clamp(1, 2) as usize;
    let source_frames = source.samples.len() / source_channels;
    if source_frames == 0 {
        return AudioSamples {
            samples: Vec::new(),
            channels: 2,
            sample_rate: 44_100,
        };
    }
    let output_frames = ((source_frames as u64 * 44_100 + source.sample_rate as u64 - 1)
        / source.sample_rate.max(1) as u64) as usize;
    let mut samples = Vec::with_capacity(output_frames * 2);
    for frame in 0..output_frames {
        let source_frame = ((frame as u64 * source.sample_rate as u64) / 44_100) as usize;
        let source_frame = source_frame.min(source_frames - 1);
        let left = source.samples[source_frame * source_channels];
        let right = if source_channels == 2 {
            source.samples[source_frame * source_channels + 1]
        } else {
            left
        };
        samples.extend([left, right]);
    }
    AudioSamples {
        samples,
        channels: 2,
        sample_rate: 44_100,
    }
}

impl AudioCapture {
    pub fn finish(self) -> Result<AudioSamples, String> {
        drop(self.stream);
        let samples = self
            .samples
            .lock()
            .map_err(|_| "無法讀取麥克風資料".to_string())?
            .clone();
        Ok(AudioSamples {
            samples,
            channels: self.channels,
            sample_rate: self.sample_rate,
        })
    }
}

pub fn start_audio_capture() -> Result<AudioCapture, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "找不到麥克風輸入裝置，請確認系統已連接麥克風".to_string())?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("無法讀取麥克風設定，請確認已允許麥克風權限：{e}"))?;
    let channels = config.channels();
    let sample_rate = config.sample_rate();
    if !(1..=2).contains(&channels) {
        return Err(format!(
            "目前只支援單聲道或雙聲道麥克風（偵測到 {channels} 聲道）"
        ));
    }
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let target = Arc::clone(&samples);
    let err_fn = |error| eprintln!("[record] 麥克風串流錯誤：{error}");
    let stream_config: cpal::StreamConfig = config.clone().into();
    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[f32], _| append_audio(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::F64 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[f64], _| append_audio_f64(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::I8 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i8], _| append_audio_i8(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i16], _| append_audio_i16(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::I24 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i32], _| append_audio_i24(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::I32 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i32], _| append_audio_i32(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::I64 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i64], _| append_audio_i64(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::U8 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[u8], _| append_audio_u8(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[u16], _| append_audio_u16(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::U24 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[u32], _| append_audio_u24(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::U32 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[u32], _| append_audio_u32(data, &target),
            err_fn,
            None,
        ),
        SampleFormat::U64 => device.build_input_stream(
            stream_config,
            move |data: &[u64], _| append_audio_u64(data, &target),
            err_fn,
            None,
        ),
        format => return Err(format!("麥克風格式 {format:?} 尚未支援")),
    }
    .map_err(|e| format!("建立麥克風串流失敗，請確認麥克風權限：{e}"))?;
    stream
        .play()
        .map_err(|e| format!("啟動麥克風失敗，請確認麥克風權限：{e}"))?;
    eprintln!(
        "[record] microphone ready rate={} channels={}",
        sample_rate, channels
    );
    Ok(AudioCapture {
        stream,
        samples,
        channels,
        sample_rate,
    })
}

pub fn encode_aac(samples: &AudioSamples) -> Result<Vec<EncodedAudioPacket>, String> {
    if samples.samples.is_empty() {
        return Ok(Vec::new());
    }
    let mut encoder = AacEncoder::new(AacEncoderConfig {
        bitrate_bps: 128_000,
        ..Default::default()
    });
    encoder
        .push_pcm(&samples.samples, samples.channels, samples.sample_rate)
        .map_err(|e| format!("AAC 編碼失敗：{e}"))?;
    encoder.finish();
    let mut packets = Vec::new();
    while let Ok(packet) = encoder.next_packet() {
        packets.push(EncodedAudioPacket {
            bytes: packet.data,
            duration: packet.duration,
        });
    }
    Ok(packets)
}

fn append_audio(data: &[f32], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend_from_slice(data);
    }
}

fn append_audio_f64(data: &[f64], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(data.iter().map(|v| (*v as f32).clamp(-1.0, 1.0)));
    }
}

fn append_audio_i8(data: &[i8], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(data.iter().map(|v| *v as f32 / 128.0));
    }
}

fn append_audio_i16(data: &[i16], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(data.iter().map(|v| *v as f32 / 32768.0));
    }
}

fn append_audio_i24(data: &[i32], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(
            data.iter()
                .map(|v| (*v as f32 / 8_388_608.0).clamp(-1.0, 1.0)),
        );
    }
}

fn append_audio_i32(data: &[i32], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(data.iter().map(|v| *v as f32 / 2147483648.0));
    }
}

fn append_audio_i64(data: &[i64], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(
            data.iter()
                .map(|v| (*v as f64 / 9_223_372_036_854_775_808.0) as f32),
        );
    }
}

fn append_audio_u8(data: &[u8], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(data.iter().map(|v| (*v as f32 - 128.0) / 128.0));
    }
}

fn append_audio_u16(data: &[u16], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(data.iter().map(|v| (*v as f32 - 32_768.0) / 32_768.0));
    }
}

fn append_audio_u24(data: &[u32], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(data.iter().map(|v| (*v as f32 - 8_388_608.0) / 8_388_608.0));
    }
}

fn append_audio_u32(data: &[u32], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(
            data.iter()
                .map(|v| (*v as f64 - 2_147_483_648.0) as f32 / 2_147_483_648.0),
        );
    }
}

fn append_audio_u64(data: &[u64], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() {
        samples.extend(data.iter().map(|v| {
            (*v as f64 - 9_223_372_036_854_775_808.0) as f32 / 9_223_372_036_854_775_808.0
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(values: &[f32], channels: u16, sample_rate: u32) -> AudioSamples {
        AudioSamples {
            samples: values.to_vec(),
            channels,
            sample_rate,
        }
    }

    #[test]
    fn keeps_single_audio_source_unchanged() {
        let source = samples(&[0.1, -0.2], 2, 44_100);
        let mixed = mix_audio_samples(Some(source), None).unwrap();
        assert_eq!(mixed.samples, vec![0.1, -0.2]);
        assert_eq!(mixed.channels, 2);
        assert_eq!(mixed.sample_rate, 44_100);
    }

    #[test]
    fn mixes_mono_microphone_into_stereo_system_audio() {
        let microphone = samples(&[0.25, 0.5], 1, 44_100);
        let system = samples(&[0.1, 0.2, -0.1, -0.2], 2, 44_100);
        let mixed = mix_audio_samples(Some(microphone), Some(system)).unwrap();
        assert_eq!(mixed.channels, 2);
        assert_eq!(mixed.sample_rate, 44_100);
        assert_eq!(mixed.samples, vec![0.35, 0.45, 0.4, 0.3]);
    }

    #[test]
    fn clamps_mixed_audio_to_float_range() {
        let microphone = samples(&[0.9, 0.9], 2, 44_100);
        let system = samples(&[0.8, -0.8], 2, 44_100);
        let mixed = mix_audio_samples(Some(microphone), Some(system)).unwrap();
        assert_eq!(mixed.samples[0], 1.0);
        assert!((mixed.samples[1] - 0.1).abs() < 0.00001);
    }
}
