import { useCallback, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { cropCanvasToBase64 } from "../editor/imageExport";

interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function useSaveScreenshot(
  canvasRef: MutableRefObject<HTMLCanvasElement | null>,
  cropRect: CropRect | null,
  saveFormat: "png" | "jpg",
  closeEditor: () => Promise<void>,
  showToast: (message: string) => void,
) {
  return useCallback(async () => {
    if (!canvasRef.current || !cropRect) return;
    const base64 = cropCanvasToBase64(canvasRef.current, cropRect);
    if (!base64) return;
    try {
      const filepath = await save({
        filters: saveFormat === "png"
          ? [{ name: "PNG 圖片", extensions: ["png"] }]
          : [{ name: "JPG 圖片", extensions: ["jpg", "jpeg"] }],
        defaultPath: saveFormat === "png" ? "Screenshot.png" : "Screenshot.jpg",
      });
      if (filepath) {
        await invoke("save_and_copy_screenshot", {
          base64Image: base64,
          savePath: filepath,
          autoCopy: false,
        });
        showToast("圖片已儲存");
        setTimeout(async () => { await closeEditor(); }, 600);
      }
    } catch (err) {
      console.error("Save error:", err);
      showToast("儲存失敗");
    }
  }, [canvasRef, cropRect, saveFormat, closeEditor, showToast]);
}
