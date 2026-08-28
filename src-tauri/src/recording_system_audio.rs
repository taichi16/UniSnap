//! Windows WASAPI loopback capture.
//!
//! The implementation deliberately returns the captured audio as the same
//! normalized f32 PCM representation used by the microphone path. This keeps
//! device capture separate from AAC/MP4 concerns and makes the source easy to
//! mix or test independently.

use crate::recording_audio::AudioSamples;

#[cfg(not(target_os = "windows"))]
pub struct SystemAudioCapture {
    pub channels: u16,
    pub sample_rate: u32,
}

#[cfg(not(target_os = "windows"))]
pub fn start_system_audio_capture() -> Result<SystemAudioCapture, String> {
    Err("系統音訊錄製需要在 Windows WASAPI 環境執行".to_string())
}

#[cfg(not(target_os = "windows"))]
impl SystemAudioCapture {
    pub fn finish(self) -> Result<AudioSamples, String> {
        let _ = self;
        Err("系統音訊錄製需要在 Windows WASAPI 環境執行".to_string())
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::AudioSamples;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use wasapi::{initialize_mta, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

    pub struct SystemAudioCapture {
        stop: Arc<AtomicBool>,
        join: Option<JoinHandle<Result<(), String>>>,
        samples: Arc<Mutex<Vec<f32>>>,
        pub channels: u16,
        pub sample_rate: u32,
    }

    pub fn start_system_audio_capture() -> Result<SystemAudioCapture, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
        let startup_stop = Arc::clone(&stop);
        let worker_stop = Arc::clone(&stop);
        let worker_samples = Arc::clone(&samples);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);

        let join = thread::Builder::new()
            .name("UniSnap-WASAPI-loopback".to_string())
            .spawn(move || {
                if let Err(error) = capture_loop(worker_stop, worker_samples, ready_tx.clone()) {
                    let _ = ready_tx.send(Err(error.clone()));
                    Err(error)
                } else {
                    Ok(())
                }
            })
            .map_err(|e| format!("建立 Windows 系統音訊執行緒失敗：{e}"))?;

        match ready_rx.recv() {
            Ok(Ok((sample_rate, channels))) => Ok(SystemAudioCapture {
                stop: startup_stop,
                join: Some(join),
                samples,
                channels,
                sample_rate,
            }),
            Ok(Err(error)) => {
                startup_stop.store(true, Ordering::Release);
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                startup_stop.store(true, Ordering::Release);
                let _ = join.join();
                Err("Windows 系統音訊執行緒未回報啟動結果".to_string())
            }
        }
    }

    impl SystemAudioCapture {
        pub fn finish(mut self) -> Result<AudioSamples, String> {
            self.stop.store(true, Ordering::Release);
            if let Some(join) = self.join.take() {
                join.join()
                    .map_err(|_| "Windows 系統音訊執行緒異常結束".to_string())??;
            }
            let samples = self
                .samples
                .lock()
                .map_err(|_| "無法讀取 Windows 系統音訊資料".to_string())?
                .clone();
            Ok(AudioSamples {
                samples,
                channels: self.channels,
                sample_rate: self.sample_rate,
            })
        }
    }

    fn capture_loop(
        stop: Arc<AtomicBool>,
        samples: Arc<Mutex<Vec<f32>>>,
        ready_tx: mpsc::SyncSender<Result<(u32, u16), String>>,
    ) -> Result<(), String> {
        initialize_mta()
            .ok()
            .map_err(|e| format!("初始化 WASAPI COM 執行緒失敗：{e}"))?;
        let enumerator =
            DeviceEnumerator::new().map_err(|e| format!("建立 Windows 音訊裝置列舉器失敗：{e}"))?;
        let device = enumerator
            .get_default_device(&Direction::Render)
            .map_err(|e| {
                "找不到 Windows 預設輸出裝置，請確認音效輸出已啟用：".to_string() + &e.to_string()
            })?;
        let mut audio_client = device
            .get_iaudioclient()
            .map_err(|e| format!("建立 WASAPI 音訊用戶端失敗：{e}"))?;

        // Shared-mode loopback is intentionally normalized to stereo float
        // PCM. WASAPI performs the device-format conversion in shared mode.
        let desired_format = WaveFormat::new(32, 32, &SampleType::Float, 44_100, 2, None);
        let (default_period, _) = audio_client
            .get_device_period()
            .map_err(|e| format!("讀取 WASAPI 裝置週期失敗：{e}"))?;
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: default_period,
        };
        audio_client
            // A render endpoint becomes a loopback capture client when the
            // requested stream direction is Capture. Passing Render here
            // creates a render client, so get_audiocaptureclient() fails with
            // AUDCLNT_E_WRONG_ENDPOINT_TYPE (0x88890003).
            .initialize_client(&desired_format, &Direction::Capture, &mode)
            .map_err(|e| format!("初始化 WASAPI loopback 失敗：{e}"))?;
        let event = audio_client
            .set_get_eventhandle()
            .map_err(|e| format!("建立 WASAPI 事件通知失敗：{e}"))?;
        let capture_client = audio_client
            .get_audiocaptureclient()
            .map_err(|e| format!("取得 WASAPI loopback 擷取介面失敗：{e}"))?;
        audio_client
            .start_stream()
            .map_err(|e| format!("啟動 WASAPI loopback 失敗：{e}"))?;

        ready_tx
            .send(Ok((44_100, 2)))
            .map_err(|_| "系統音訊啟動結果無法回傳".to_string())?;

        let block_align = desired_format.get_blockalign() as usize;
        let mut queue = VecDeque::new();
        while !stop.load(Ordering::Acquire) {
            capture_client
                .read_from_device_to_deque(&mut queue)
                .map_err(|e| format!("讀取 WASAPI loopback 音訊失敗：{e}"))?;
            let usable_bytes = queue.len() - (queue.len() % block_align);
            if usable_bytes > 0 {
                let mut bytes = Vec::with_capacity(usable_bytes);
                for _ in 0..usable_bytes {
                    bytes.push(queue.pop_front().expect("WASAPI queue length checked"));
                }
                let mut guard = samples
                    .lock()
                    .map_err(|_| "WASAPI 系統音訊緩衝區鎖定失敗".to_string())?;
                for chunk in bytes.chunks_exact(4) {
                    guard.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            let _ = event.wait_for_event(100);
        }
        audio_client
            .stop_stream()
            .map_err(|e| format!("停止 WASAPI loopback 失敗：{e}"))?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::{start_system_audio_capture, SystemAudioCapture};

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn non_windows_backend_is_explicitly_unavailable() {
        assert!(super::start_system_audio_capture().is_err());
    }
}
