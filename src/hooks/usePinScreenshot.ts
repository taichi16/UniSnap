import { useCallback, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { cropCanvasToBase64 } from "../editor/imageExport";

interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function usePinScreenshot(
  canvasRef: MutableRefObject<HTMLCanvasElement | null>,
  cropRect: CropRect | null,
  closeEditor: () => Promise<void>,
  showToast: (message: string) => void,
) {
  return useCallback(async () => {
    if (!canvasRef.current || !cropRect) return;
    const base64 = cropCanvasToBase64(canvasRef.current, cropRect);
    if (!base64) return;
    try {
      await invoke("pin_screenshot", {
        imageBase64: base64,
        x: cropRect.x,
        y: cropRect.y,
        width: cropRect.w,
        height: cropRect.h,
      });
      await closeEditor();
    } catch (err) {
      console.error("Pin error:", err);
      showToast("貼圖失敗");
    }
  }, [canvasRef, cropRect, closeEditor, showToast]);
}
