use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

const HNS_PER_SECOND: i64 = 10_000_000;

fn mf_error(context: &str, error: windows::core::Error) -> String {
    format!("{context}：{error}")
}

fn packed_ratio(numerator: u32, denominator: u32) -> u64 {
    ((numerator as u64) << 32) | denominator as u64
}

unsafe fn set_guid(
    media_type: &IMFMediaType,
    key: &windows::core::GUID,
    value: &windows::core::GUID,
) -> Result<(), String> {
    unsafe { media_type.SetGUID(key, value) }
        .map_err(|e| mf_error("設定 Media Foundation GUID 失敗", e))
}

unsafe fn set_u32(
    media_type: &IMFMediaType,
    key: &windows::core::GUID,
    value: u32,
) -> Result<(), String> {
    unsafe { media_type.SetUINT32(key, value) }
        .map_err(|e| mf_error("設定 Media Foundation 數值失敗", e))
}

unsafe fn set_u64(
    media_type: &IMFMediaType,
    key: &windows::core::GUID,
    value: u64,
) -> Result<(), String> {
    unsafe { media_type.SetUINT64(key, value) }
        .map_err(|e| mf_error("設定 Media Foundation 比例失敗", e))
}

struct MediaFoundationRuntime {
    com_initialized: bool,
}

impl MediaFoundationRuntime {
    fn start() -> Result<Self, String> {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let com_initialized = hr.is_ok();
        if hr.is_err() && hr.0 != 0x80010106u32 as i32 {
            return Err(format!("初始化 Windows COM 失敗：0x{:08X}", hr.0 as u32));
        }
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }
            .map_err(|e| mf_error("啟動 Windows Media Foundation 失敗", e))?;
        Ok(Self { com_initialized })
    }
}

impl Drop for MediaFoundationRuntime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            if self.com_initialized {
                CoUninitialize();
            }
        }
    }
}

pub struct MediaFoundationWriter {
    writer: IMFSinkWriter,
    // Fields are dropped in declaration order. Keep the Media Foundation
    // runtime after every COM object so those objects are released first.
    _runtime: MediaFoundationRuntime,
    video_stream: u32,
    width: u32,
    height: u32,
    fps: u32,
    frame_count: u64,
    next_timeline_frame: u64,
    bgra: Vec<u8>,
}

impl MediaFoundationWriter {
    pub fn create(path: &Path, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
            return Err(format!(
                "Media Foundation 需要正偶數影片尺寸，收到 {width}x{height}"
            ));
        }
        let runtime = MediaFoundationRuntime::start()?;
        let mut attributes = None;
        unsafe { MFCreateAttributes(&mut attributes, 1) }
            .map_err(|e| mf_error("建立 Media Foundation 屬性失敗", e))?;
        let attributes = attributes.ok_or("Media Foundation 未回傳屬性物件")?;
        unsafe {
            // Do not opt into vendor hardware encoders here. Some otherwise
            // supported Windows 11 PCs expose an Intel/AMD/NVIDIA Media
            // Foundation encoder that starts successfully and then crashes
            // inside the display-driver DLL after several seconds. That is a
            // process-level access violation and cannot be recovered by Rust.
            // The inbox software H.264 MFT is slower but portable and keeps a
            // bad or old display driver from terminating UniSnap.
            attributes
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 0)
                .map_err(|e| mf_error("設定相容影片編碼模式失敗", e))?;
            attributes
                .SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)
                .map_err(|e| mf_error("設定影片寫入節流模式失敗", e))?;
        }

        let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let writer = unsafe {
            MFCreateSinkWriterFromURL(PCWSTR(wide_path.as_ptr()), None, Some(&attributes))
        }
        .map_err(|e| mf_error("建立 Windows MP4 Sink Writer 失敗", e))?;

        let video_output =
            unsafe { MFCreateMediaType() }.map_err(|e| mf_error("建立 H.264 輸出格式失敗", e))?;
        unsafe {
            set_guid(&video_output, &MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            set_guid(&video_output, &MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            set_u32(
                &video_output,
                &MF_MT_AVG_BITRATE,
                ((width as u64 * height as u64 * fps as u64 / 8).clamp(4_000_000, 20_000_000))
                    as u32,
            )?;
            set_u32(
                &video_output,
                &MF_MT_INTERLACE_MODE,
                MFVideoInterlace_Progressive.0 as u32,
            )?;
            set_u64(
                &video_output,
                &MF_MT_FRAME_SIZE,
                packed_ratio(width, height),
            )?;
            set_u64(&video_output, &MF_MT_FRAME_RATE, packed_ratio(fps, 1))?;
            set_u64(&video_output, &MF_MT_PIXEL_ASPECT_RATIO, packed_ratio(1, 1))?;
        }
        let video_stream = unsafe { writer.AddStream(&video_output) }
            .map_err(|e| mf_error("新增 H.264 影片軌失敗", e))?;

        let video_input =
            unsafe { MFCreateMediaType() }.map_err(|e| mf_error("建立 BGRA 輸入格式失敗", e))?;
        unsafe {
            set_guid(&video_input, &MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            set_guid(&video_input, &MF_MT_SUBTYPE, &MFVideoFormat_ARGB32)?;
            set_u32(
                &video_input,
                &MF_MT_INTERLACE_MODE,
                MFVideoInterlace_Progressive.0 as u32,
            )?;
            set_u64(&video_input, &MF_MT_FRAME_SIZE, packed_ratio(width, height))?;
            set_u64(&video_input, &MF_MT_FRAME_RATE, packed_ratio(fps, 1))?;
            set_u64(&video_input, &MF_MT_PIXEL_ASPECT_RATIO, packed_ratio(1, 1))?;
            set_u32(&video_input, &MF_MT_DEFAULT_STRIDE, width * 4)?;
            writer
                .SetInputMediaType(video_stream, &video_input, None)
                .map_err(|e| mf_error("設定 BGRA 影片輸入失敗", e))?;
        }

        unsafe { writer.BeginWriting() }.map_err(|e| mf_error("開始寫入 Windows MP4 失敗", e))?;

        Ok(Self {
            writer,
            _runtime: runtime,
            video_stream,
            width,
            height,
            fps,
            frame_count: 0,
            next_timeline_frame: 0,
            bgra: vec![0; width as usize * height as usize * 4],
        })
    }

    pub fn write_rgba_frame_at(&mut self, rgba: &[u8], timeline_frame: u64) -> Result<(), String> {
        let required = self.width as usize * self.height as usize * 4;
        if rgba.len() != required {
            return Err(format!(
                "Media Foundation 影格長度錯誤：預期 {required}，收到 {}",
                rgba.len()
            ));
        }
        copy_rgba_to_bgra(rgba, &mut self.bgra);
        if timeline_frame < self.next_timeline_frame {
            return Err("Media Foundation 影片時間戳記不可倒退".to_string());
        }
        let time = (timeline_frame as i64 * HNS_PER_SECOND) / self.fps as i64;
        let next_time = ((timeline_frame + 1) as i64 * HNS_PER_SECOND) / self.fps as i64;
        unsafe {
            write_sample(
                &self.writer,
                self.video_stream,
                &self.bgra,
                time,
                next_time - time,
            )?
        };
        self.frame_count += 1;
        self.next_timeline_frame = timeline_frame + 1;
        Ok(())
    }

    pub fn finish(self) -> Result<u64, String> {
        unsafe { self.writer.Finalize() }.map_err(|e| mf_error("完成 Windows MP4 存檔失敗", e))?;
        Ok(self.frame_count)
    }
}

fn copy_rgba_to_bgra(rgba: &[u8], bgra: &mut [u8]) {
    // xcap supplies top-down rows. Keep that order and advertise a positive
    // stride to Media Foundation; a negative stride makes the software H.264
    // encoder vertically flip the complete desktop image.
    for (source, destination) in rgba.chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
        destination.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
    }
}

unsafe fn write_sample(
    writer: &IMFSinkWriter,
    stream: u32,
    bytes: &[u8],
    time: i64,
    duration: i64,
) -> Result<(), String> {
    let buffer = unsafe { MFCreateMemoryBuffer(bytes.len() as u32) }
        .map_err(|e| mf_error("建立 Media Foundation 緩衝區失敗", e))?;
    let mut destination = std::ptr::null_mut();
    unsafe { buffer.Lock(&mut destination, None, None) }
        .map_err(|e| mf_error("鎖定 Media Foundation 緩衝區失敗", e))?;
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len()) };
    unsafe { buffer.Unlock() }.map_err(|e| mf_error("解除影片緩衝區鎖定失敗", e))?;
    unsafe { buffer.SetCurrentLength(bytes.len() as u32) }
        .map_err(|e| mf_error("設定影片緩衝區長度失敗", e))?;
    let sample = unsafe { MFCreateSample() }
        .map_err(|e| mf_error("建立 Media Foundation sample 失敗", e))?;
    unsafe {
        sample
            .AddBuffer(&buffer)
            .map_err(|e| mf_error("加入 sample 緩衝區失敗", e))?;
        sample
            .SetSampleTime(time)
            .map_err(|e| mf_error("設定 sample 時間失敗", e))?;
        sample
            .SetSampleDuration(duration)
            .map_err(|e| mf_error("設定 sample 長度失敗", e))?;
        writer
            .WriteSample(stream, &sample)
            .map_err(|e| mf_error("寫入 Media Foundation sample 失敗", e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{copy_rgba_to_bgra, packed_ratio, MediaFoundationWriter};
    use std::fs::File;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn packs_media_foundation_ratios() {
        assert_eq!(packed_ratio(30, 1), (30u64 << 32) | 1);
        assert_eq!(packed_ratio(1920, 1080), (1920u64 << 32) | 1080);
    }

    #[test]
    fn rgba_conversion_preserves_top_down_row_order() {
        // Two one-pixel rows: red is the top row and blue is the bottom row.
        let rgba = [255, 0, 0, 255, 0, 0, 255, 255];
        let mut bgra = [0; 8];
        copy_rgba_to_bgra(&rgba, &mut bgra);
        assert_eq!(bgra, [0, 0, 255, 255, 255, 0, 0, 255]);
    }

    #[test]
    fn writes_h264_to_mp4() {
        let path = std::env::temp_dir().join(format!(
            "unisnap-media-foundation-av-{}.mp4",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut writer = MediaFoundationWriter::create(&path, 320, 240, 30).unwrap();
        let mut frame = vec![0u8; 320 * 240 * 4];
        for index in 0..30u8 {
            for pixel in frame.chunks_exact_mut(4) {
                pixel.copy_from_slice(&[index.saturating_mul(4), 64, 160, 255]);
            }
            writer.write_rgba_frame_at(&frame, index as u64).unwrap();
        }
        assert_eq!(writer.finish().unwrap(), 30);

        let file = File::open(&path).unwrap();
        let size = file.metadata().unwrap().len();
        let reader = mp4::Mp4Reader::read_header(file, size).unwrap();
        assert_eq!(reader.tracks().len(), 1);
        assert_eq!(reader.sample_count(1).unwrap(), 30);
        std::fs::remove_file(path).unwrap();
    }
}
