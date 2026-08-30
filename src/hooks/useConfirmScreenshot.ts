import { useCallback, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { cropCanvasToBase64 } from "../editor/imageExport";

interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function useConfirmScreenshot(
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
      const savedPath = await invoke<string>("save_and_copy_screenshot", {
        base64Image: base64,
        savePath: null,
        autoCopy: true,
      });
      await invoke("open_quick_access", { imageData: base64, path: savedPath });
      showToast(`已存檔且複製到剪貼簿\n路徑: ${savedPath}`);
      setTimeout(async () => { await closeEditor(); }, 800);
    } catch (err) {
      console.error("Confirm error:", err);
      showToast("存檔失敗");
    }
  }, [canvasRef, cropRect, closeEditor, showToast]);
}
