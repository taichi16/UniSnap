use openh264::encoder::{EncodedBitStream, FrameType};

pub struct H264Sample {
    pub frame_type: FrameType,
    pub sps: Option<Vec<u8>>,
    pub pps: Option<Vec<u8>>,
    pub bytes: Vec<u8>,
}

fn strip_start_code(nal: &[u8]) -> &[u8] {
    if nal.starts_with(&[0, 0, 0, 1]) {
        &nal[4..]
    } else if nal.starts_with(&[0, 0, 1]) {
        &nal[3..]
    } else {
        nal
    }
}

/// 將 OpenH264 的 Annex-B 圖層轉換為 MP4 使用的 length-prefixed sample。
pub fn extract_h264_sample(encoded: &EncodedBitStream<'_>) -> Result<H264Sample, String> {
    let mut sps = None;
    let mut pps = None;
    let mut bytes = Vec::new();
    for layer_index in 0..encoded.num_layers() {
        let layer = encoded.layer(layer_index).ok_or("無法讀取 H.264 圖層")?;
        for nal_index in 0..layer.nal_count() {
            let nal = strip_start_code(layer.nal_unit(nal_index).ok_or("無法讀取 H.264 NAL")?);
            if nal.is_empty() {
                continue;
            }
            match nal[0] & 0x1f {
                7 => sps = Some(nal.to_vec()),
                8 => pps = Some(nal.to_vec()),
                _ => {
                    bytes.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                    bytes.extend_from_slice(nal);
                }
            }
        }
    }
    Ok(H264Sample {
        frame_type: encoded.frame_type(),
        sps,
        pps,
        bytes,
    })
}
