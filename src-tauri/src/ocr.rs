use base64::Engine;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn vision_ocr_png(
        data: *const u8,
        data_len: usize,
        output: *mut std::ffi::c_char,
        output_len: usize,
    ) -> bool;
}

/// Prefer native on-device Vision OCR; the frontend retains its Tesseract
/// fallback when this command returns an error.
#[tauri::command]
pub fn recognize_text_vision(base64_image: String) -> Result<String, String> {
    let payload = base64_image
        .split_once(',')
        .map(|(_, data)| data)
        .unwrap_or(base64_image.as_str());
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("Vision OCR 圖片解碼失敗：{e}"))?;

    #[cfg(target_os = "macos")]
    {
        let mut output = vec![0_i8; 1024 * 1024];
        let ok = unsafe {
            vision_ocr_png(
                bytes.as_ptr(),
                bytes.len(),
                output.as_mut_ptr(),
                output.len(),
            )
        };
        if !ok {
            return Err("macOS Vision OCR 無法處理圖片".into());
        }
        let text = unsafe { std::ffi::CStr::from_ptr(output.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        Ok(if text.is_empty() {
            "未辨識到任何文字。".into()
        } else {
            text
        })
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = bytes;
        Err("Vision OCR 僅支援 macOS".into())
    }
}
