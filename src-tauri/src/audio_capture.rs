use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

pub struct AudioCapture {
    pub stream: cpal::Stream,
    pub samples: Arc<Mutex<Vec<f32>>>,
    pub channels: u16,
    pub sample_rate: u32,
}

pub fn start_audio_capture() -> Result<AudioCapture, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "找不到麥克風輸入裝置，請確認系統已連接麥克風".to_string())?;
    let config = device
        .default_input_config()
        .map_err(|error| format!("無法讀取麥克風設定，請確認已允許麥克風權限：{error}"))?;
    let channels = config.channels();
    let sample_rate = config.sample_rate();
    if !(1..=2).contains(&channels) {
        return Err(format!("目前只支援單聲道或雙聲道麥克風（偵測到 {channels} 聲道）"));
    }

    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let target = Arc::clone(&samples);
    let err_fn = |error| eprintln!("[record] 麥克風串流錯誤：{error}");
    let stream_config: cpal::StreamConfig = config.into();
    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            stream_config, move |data: &[f32], _| append_f32(data, &target), err_fn, None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            stream_config, move |data: &[i16], _| append_i16(data, &target), err_fn, None,
        ),
        SampleFormat::I32 => device.build_input_stream(
            stream_config, move |data: &[i32], _| append_i32(data, &target), err_fn, None,
        ),
        SampleFormat::U8 => device.build_input_stream(
            stream_config, move |data: &[u8], _| append_u8(data, &target), err_fn, None,
        ),
        format => return Err(format!("麥克風格式 {format:?} 尚未支援")),
    }
    .map_err(|error| format!("建立麥克風串流失敗，請確認麥克風權限：{error}"))?;
    stream
        .play()
        .map_err(|error| format!("啟動麥克風失敗，請確認麥克風權限：{error}"))?;
    eprintln!("[record] microphone ready rate={} channels={}", sample_rate, channels);
    Ok(AudioCapture { stream, samples, channels, sample_rate })
}

fn append_f32(data: &[f32], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() { samples.extend_from_slice(data); }
}

fn append_i16(data: &[i16], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() { samples.extend(data.iter().map(|value| *value as f32 / 32768.0)); }
}

fn append_i32(data: &[i32], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() { samples.extend(data.iter().map(|value| *value as f32 / 2147483648.0)); }
}

fn append_u8(data: &[u8], target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = target.lock() { samples.extend(data.iter().map(|value| (*value as f32 - 128.0) / 128.0)); }
}
