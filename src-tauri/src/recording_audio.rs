use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use rusty_aac::{AacEncoder, AacEncoderConfig};

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub channels: u16,
    pub sample_rate: u32,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputTestResult {
    pub peak: f32,
    pub rms: f32,
    pub has_signal: bool,
}

pub struct AudioCapture {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    first_packet_timestamp_100ns: Arc<AtomicU64>,
    pub channels: u16,
    pub sample_rate: u32,
}

#[derive(Clone, Debug)]
pub struct AudioSamples {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
    /// QPC-correlated timestamp of the first PCM frame, expressed in the
    /// Windows multimedia convention of 100-nanosecond units.
    pub start_timestamp_100ns: Option<u64>,
}

pub struct EncodedAudioPacket {
    pub bytes: Vec<u8>,
    pub duration: u32,
}

/// Raises an unusually quiet recording to a practical level while preserving
/// loud sources and limiting gain so ambient noise is not amplified without
/// bound. A peak below -60 dBFS is treated as silence.
pub fn normalize_recording_audio(samples: &mut AudioSamples) -> f32 {
    let peak = samples
        .samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);
    if peak < 0.001 || peak >= 0.85 {
        return 1.0;
    }
    let gain = (0.85 / peak).min(16.0);
    for sample in &mut samples.samples {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }
    gain
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
            let common_start = match (
                microphone.start_timestamp_100ns,
                system_audio.start_timestamp_100ns,
            ) {
                (Some(microphone), Some(system)) => Some(microphone.min(system)),
                (Some(timestamp), None) | (None, Some(timestamp)) => Some(timestamp),
                (None, None) => None,
            };
            let microphone_offset = audio_start_frame_offset(&microphone, common_start);
            let system_offset = audio_start_frame_offset(&system_audio, common_start);
            let frame_count = (microphone_offset + microphone.samples.len() / 2)
                .max(system_offset + system_audio.samples.len() / 2);
            let mut samples = vec![0.0; frame_count * 2];
            for frame in 0..frame_count {
                for channel in 0..2 {
                    let mic = microphone
                        .samples
                        .get(frame.saturating_sub(microphone_offset) * 2 + channel)
                        .filter(|_| frame >= microphone_offset)
                        .copied()
                        .unwrap_or(0.0);
                    let system = system_audio
                        .samples
                        .get(frame.saturating_sub(system_offset) * 2 + channel)
                        .filter(|_| frame >= system_offset)
                        .copied()
                        .unwrap_or(0.0);
                    samples[frame * 2 + channel] = (mic + system).clamp(-1.0, 1.0);
                }
            }
            Some(AudioSamples {
                samples,
                channels: 2,
                sample_rate: 44_100,
                start_timestamp_100ns: common_start,
            })
        }
    }
}

fn resample_to_stereo_44100(source: &AudioSamples) -> AudioSamples {
    let source_channels = source.channels.max(1) as usize;
    let source_frames = source.samples.len() / source_channels;
    if source_frames == 0 {
        return AudioSamples {
            samples: Vec::new(),
            channels: 2,
            sample_rate: 44_100,
            start_timestamp_100ns: source.start_timestamp_100ns,
        };
    }
    let output_frames = ((source_frames as u64 * 44_100 + source.sample_rate as u64 - 1)
        / source.sample_rate.max(1) as u64) as usize;
    let mut samples = Vec::with_capacity(output_frames * 2);
    for frame in 0..output_frames {
        let source_frame = ((frame as u64 * source.sample_rate as u64) / 44_100) as usize;
        let source_frame = source_frame.min(source_frames - 1);
        let frame_start = source_frame * source_channels;
        let (left, right) = match source_channels {
            1 => {
                let mono = source.samples[frame_start];
                (mono, mono)
            }
            2 => (source.samples[frame_start], source.samples[frame_start + 1]),
            _ => {
                // Microphone arrays commonly expose 4 or more channels. For
                // recording compatibility, average every channel in the
                // frame to mono and duplicate it to stereo. This avoids
                // arbitrarily dropping the physical microphone capsules.
                let mono = source.samples[frame_start..frame_start + source_channels]
                    .iter()
                    .copied()
                    .sum::<f32>()
                    / source_channels as f32;
                (mono, mono)
            }
        };
        samples.extend([left, right]);
    }
    AudioSamples {
        samples,
        channels: 2,
        sample_rate: 44_100,
        start_timestamp_100ns: source.start_timestamp_100ns,
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
        Ok(resample_to_stereo_44100(&AudioSamples {
            samples,
            channels: self.channels,
            sample_rate: self.sample_rate,
            start_timestamp_100ns: match self.first_packet_timestamp_100ns.load(Ordering::Acquire) {
                0 => None,
                timestamp => Some(timestamp),
            },
        }))
    }
}

fn audio_start_frame_offset(source: &AudioSamples, common_start: Option<u64>) -> usize {
    match (source.start_timestamp_100ns, common_start) {
        (Some(source_start), Some(common_start)) if source_start > common_start => {
            (((source_start - common_start) as u128 * 44_100) / 10_000_000).min(usize::MAX as u128)
                as usize
        }
        _ => 0,
    }
}

fn device_name(device: &cpal::Device, fallback: String) -> String {
    device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or(fallback)
}

fn device_id(index: usize, device: &cpal::Device, name: &str) -> String {
    device
        .description()
        .ok()
        .and_then(|description| description.address().map(str::to_owned))
        .filter(|address| !address.trim().is_empty())
        .map(|address| format!("endpoint:{address}"))
        .unwrap_or_else(|| format!("{index}:{name}"))
}

fn resolve_input_device(
    host: &cpal::Host,
    requested_device_id: Option<&str>,
) -> Result<cpal::Device, String> {
    let Some(requested) = requested_device_id.filter(|value| !value.trim().is_empty()) else {
        return host
            .default_input_device()
            .ok_or_else(|| "找不到 Windows 預設麥克風，請在錄影設定中選擇輸入裝置".to_string());
    };

    let (requested_index, requested_name) = requested
        .split_once(':')
        .and_then(|(index, name)| index.parse::<usize>().ok().map(|index| (Some(index), name)))
        .unwrap_or((None, requested));
    let devices = host
        .input_devices()
        .map_err(|e| format!("無法列舉麥克風裝置：{e}"))?;
    let mut name_match = None;
    for (index, device) in devices.enumerate() {
        let name = device_name(&device, format!("麥克風 {}", index + 1));
        if device_id(index, &device, &name) == requested {
            return Ok(device);
        }
        if requested_index == Some(index) && name == requested_name {
            return Ok(device);
        }
        if name_match.is_none() && name == requested_name {
            name_match = Some(device);
        }
    }
    name_match
        .ok_or_else(|| format!("先前選擇的麥克風「{requested_name}」目前未連接，請重新選擇裝置"))
}

#[tauri::command]
pub fn list_audio_input_devices() -> Result<Vec<AudioInputDeviceInfo>, String> {
    let host = cpal::default_host();
    let default_device = host.default_input_device();
    let devices = host
        .input_devices()
        .map_err(|e| format!("無法列舉麥克風裝置：{e}"))?;
    let mut result = Vec::new();
    for (index, device) in devices.enumerate() {
        let name = device_name(&device, format!("麥克風 {}", index + 1));
        let config = device.default_input_config().ok();
        result.push(AudioInputDeviceInfo {
            id: device_id(index, &device, &name),
            is_default: default_device.as_ref() == Some(&device),
            name,
            channels: config.as_ref().map(|value| value.channels()).unwrap_or(0),
            sample_rate: config
                .as_ref()
                .map(|value| value.sample_rate())
                .unwrap_or(0),
        });
    }
    Ok(result)
}

#[tauri::command]
pub fn test_audio_input_device(device_id: Option<String>) -> Result<AudioInputTestResult, String> {
    let capture = start_audio_capture(device_id.as_deref())?;
    std::thread::sleep(std::time::Duration::from_millis(700));
    let samples = capture.finish()?.samples;
    if samples.is_empty() {
        return Ok(AudioInputTestResult {
            peak: 0.0,
            rms: 0.0,
            has_signal: false,
        });
    }
    let peak = samples
        .iter()
        .fold(0.0_f32, |value, sample| value.max(sample.abs()));
    let mean_square =
        samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32;
    let rms = mean_square.sqrt();
    Ok(AudioInputTestResult {
        peak,
        rms,
        has_signal: peak >= 0.002 || rms >= 0.0005,
    })
}

pub fn start_audio_capture(device_id: Option<&str>) -> Result<AudioCapture, String> {
    let host = cpal::default_host();
    let device = resolve_input_device(&host, device_id)?;
    let device_name = device_name(&device, "未知麥克風".to_string());
    let config = device
        .default_input_config()
        .map_err(|e| format!("無法讀取麥克風設定，請確認已允許麥克風權限：{e}"))?;
    let channels = config.channels();
    let sample_rate = config.sample_rate();
    if channels == 0 {
        return Err(format!("麥克風「{device_name}」回報無效的 0 聲道格式"));
    }
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let target = Arc::clone(&samples);
    let first_packet_timestamp_100ns = Arc::new(AtomicU64::new(0));
    let timestamp_target = Arc::clone(&first_packet_timestamp_100ns);
    let err_fn = |error| eprintln!("[record] 麥克風串流錯誤：{error}");
    let stream_config: cpal::StreamConfig = config.clone().into();
    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[f32], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::F64 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[f64], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_f64(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::I8 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i8], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_i8(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i16], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_i16(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::I24 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i32], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_i24(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::I32 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i32], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_i32(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::I64 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i64], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_i64(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::U8 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[u8], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_u8(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[u16], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_u16(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::U24 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[u32], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_u24(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::U32 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[u32], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_u32(data, &target);
            },
            err_fn,
            None,
        ),
        SampleFormat::U64 => device.build_input_stream(
            stream_config,
            move |data: &[u64], info| {
                remember_input_timestamp(info, &timestamp_target);
                append_audio_u64(data, &target);
            },
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
        "[record] microphone ready device={:?} rate={} channels={} normalized=44100Hz/stereo",
        device_name, sample_rate, channels
    );
    Ok(AudioCapture {
        stream,
        samples,
        first_packet_timestamp_100ns,
        channels,
        sample_rate,
    })
}

fn remember_input_timestamp(
    info: &cpal::InputCallbackInfo,
    first_packet_timestamp_100ns: &AtomicU64,
) {
    let timestamp = (info.timestamp().capture.as_nanos() / 100).min(u64::MAX as u128) as u64;
    if timestamp > 0 {
        let _ = first_packet_timestamp_100ns.compare_exchange(
            0,
            timestamp,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
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
            start_timestamp_100ns: None,
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

    #[test]
    fn downmixes_four_channel_microphone_array_to_stereo() {
        let source = samples(&[0.2, 0.4, 0.6, 0.8], 4, 44_100);
        let normalized = resample_to_stereo_44100(&source);
        assert_eq!(normalized.channels, 2);
        assert_eq!(normalized.sample_rate, 44_100);
        assert_eq!(normalized.samples, vec![0.5, 0.5]);
    }

    #[test]
    fn raises_quiet_recording_with_bounded_gain() {
        let mut source = samples(&[0.01, -0.02], 2, 44_100);
        let gain = normalize_recording_audio(&mut source);
        assert_eq!(gain, 16.0);
        assert_eq!(source.samples, vec![0.16, -0.32]);
    }

    #[test]
    fn does_not_amplify_silence_or_loud_audio() {
        let mut silence = samples(&[0.0001, -0.0002], 2, 44_100);
        assert_eq!(normalize_recording_audio(&mut silence), 1.0);
        let mut loud = samples(&[0.9, -0.9], 2, 44_100);
        assert_eq!(normalize_recording_audio(&mut loud), 1.0);
        assert_eq!(loud.samples, vec![0.9, -0.9]);
    }
}
