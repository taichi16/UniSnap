use std::fs;
use std::io::{Seek, Write};
use std::path::Path;

use mp4::Mp4Writer;

pub fn finalize_mp4<W: Write + Seek>(
    mut writer: Mp4Writer<W>,
    path: &Path,
) -> Result<(), String> {
    writer
        .write_end()
        .map_err(|e| format!("完成 MP4 存檔失敗：{e}"))?;
    let size = fs::metadata(path)
        .map_err(|e| format!("無法驗證 MP4 檔案：{e}"))?
        .len();
    if size == 0 {
        return Err("MP4 檔案為空".into());
    }
    Ok(())
}
