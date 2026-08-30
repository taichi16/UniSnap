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
        // Keep file persistence independent from clipboard availability.
        // A clipboard failure must not turn a successful save into a save error.
        autoCopy: false,
      });

      let copied = true;
      try {
        await invoke("copy_screenshot_to_clipboard", { base64Image: base64 });
      } catch (error) {
        copied = false;
        console.error("Clipboard copy error after save:", error);
      }

      let quickAccessOpened = true;
      try {
        await invoke("open_quick_access", { imageData: base64, path: savedPath });
      } catch (error) {
        quickAccessOpened = false;
        console.error("Quick Access error after save:", error);
      }

      const status = copied ? "已複製到剪貼簿" : "剪貼簿複製失敗";
      const quickAccessStatus = quickAccessOpened ? "" : "；快速取用視窗開啟失敗";
      showToast(`已存檔，${status}${quickAccessStatus}\n路徑: ${savedPath}`);
      setTimeout(async () => { await closeEditor(); }, 800);
    } catch (err) {
      console.error("Confirm error:", err);
      showToast("存檔失敗");
    }
  }, [canvasRef, cropRect, closeEditor, showToast]);
}
