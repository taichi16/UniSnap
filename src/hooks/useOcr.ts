import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import Tesseract from "tesseract.js";

export function useOcr(
  getImageBase64: () => string | null,
  showToast: (message: string) => void,
) {
  const [ocrText, setOcrText] = useState<string | null>(null);
  const [ocrLoading, setOcrLoading] = useState(false);

  const handleOCR = async () => {
    const base64 = getImageBase64();
    if (!base64) return;

    setOcrLoading(true);
    showToast("進行文字辨識中...");
    try {
      let text: string;
      try {
        text = await invoke<string>("recognize_text_vision", { base64Image: base64 });
      } catch (visionError) {
        console.warn("Vision OCR unavailable, using Tesseract fallback:", visionError);
        const result = await Tesseract.recognize(base64, "chi_tra+eng");
        text = result.data.text || "未辨識到任何文字。";
      }
      setOcrText(text);
      await writeText(text);
      showToast("文字辨識完成，已複製到剪貼簿");
    } catch (err) {
      console.error("OCR error:", err);
      showToast("文字辨識失敗");
    } finally {
      setOcrLoading(false);
    }
  };

  return { ocrText, setOcrText, ocrLoading, handleOCR };
}
