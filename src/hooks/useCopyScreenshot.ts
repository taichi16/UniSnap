import { useCallback, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { cropCanvasToBase64 } from "../editor/imageExport";

interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function useCopyScreenshot(
  canvasRef: MutableRefObject<HTMLCanvasElement | null>,
  cropRect: CropRect | null,
  imageLoaded: boolean,
  showToast: (message: string) => void,
) {
  return useCallback(async () => {
    console.debug("[clipboard-ui] copy start", {
      hasCanvas: Boolean(canvasRef.current),
      hasCropRect: Boolean(cropRect),
      cropRect,
      imageLoaded,
    });
    const base64 = canvasRef.current && cropRect
      ? cropCanvasToBase64(canvasRef.current, cropRect)
      : null;
    if (!base64) {
      const reason = !canvasRef.current
        ? "編輯畫布尚未建立"
        : !cropRect
          ? "尚未建立可複製的選取範圍"
          : "目前畫面尚未完成載入";
      console.error("[clipboard-ui] no image data", reason);
      showToast(`複製失敗：${reason}`);
      return;
    }

    try {
      console.debug("[clipboard-ui] invoking copy_screenshot_to_clipboard", {
        base64Chars: base64.length,
      });
      await invoke("copy_screenshot_to_clipboard", { base64Image: base64 });
      console.debug("[clipboard-ui] copy command succeeded");
      showToast("已複製截圖到剪貼簿！可直接貼上使用 (Ctrl+V)");
    } catch (err) {
      console.error("Copy error:", err);
      showToast(`複製到剪貼簿失敗：${String(err)}`);
    }
  }, [canvasRef, cropRect, imageLoaded, showToast]);
}
