//! Recording backend selection for Windows.
//!
//! Media Foundation is probed independently before it is allowed to replace
//! the existing FFmpeg pipeline. The probe is intentionally side-effect free
//! (it starts and immediately shuts down the MF platform).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingBackend {
    MediaFoundation,
    Ffmpeg,
}

#[cfg(target_os = "windows")]
pub fn media_foundation_available() -> bool {
    // MFStartup/MFShutdown are process-global and must be paired. A failed
    // startup means this machine cannot use the native backend safely.
    unsafe {
        let hr = windows::Win32::Media::MediaFoundation::MFStartup(
            windows::Win32::Media::MediaFoundation::MF_VERSION,
            windows::Win32::Media::MediaFoundation::MFSTARTUP_LITE,
        );
        if hr.is_err() {
            return false;
        }
        windows::Win32::Media::MediaFoundation::MFShutdown();
        true
    }
}

#[cfg(not(target_os = "windows"))]
pub fn media_foundation_available() -> bool {
    false
}

pub fn select_backend() -> RecordingBackend {
    // Do not switch production recording yet. The native encoder will be
    // enabled only after the real MP4/audio acceptance tests pass.
    let _ = media_foundation_available();
    RecordingBackend::Ffmpeg
}
