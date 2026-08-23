import { useCallback, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getAllWindows, getCurrentWindow } from "@tauri-apps/api/window";
import { getCaptureMonitorIndex } from "../editor/windowIdentity";
import type { Shape } from "../editor/types";

interface CropRect { x: number; y: number; w: number; h: number }

export function useScrollCapture(options: {
  label: string;
  cropRect: CropRect | null;
  imageRef: MutableRefObject<HTMLImageElement | null>;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  isScrollingModeRef: MutableRefObject<boolean>;
  scrollCancelRequestedRef: MutableRefObject<boolean>;
  setIsScrollingMode: Dispatch<SetStateAction<boolean>>;
  setIsStitching: Dispatch<SetStateAction<boolean>>;
  setIsStitchedResult: Dispatch<SetStateAction<boolean>>;
  setImageLoaded: Dispatch<SetStateAction<boolean>>;
  setCropRect: Dispatch<SetStateAction<CropRect | null>>;
  setShapes: Dispatch<SetStateAction<Shape[]>>;
  showToast: (message: string) => void;
}) {
  return useCallback(async () => {
    if (options.isScrollingModeRef.current) return;
    if (!options.cropRect || options.cropRect.w < 80 || options.cropRect.h < 80) {
      options.showToast("請先框選單一可捲動內容區域");
      return;
    }
    options.isScrollingModeRef.current = true;
    options.scrollCancelRequestedRef.current = false;
    options.setIsScrollingMode(true);
    options.setIsStitching(true);

    const win = getCurrentWindow();
    const monitorIndex = getCaptureMonitorIndex(options.label);
    try {
      const allWindows = await getAllWindows();
      for (const window of allWindows) {
        if (window.label.startsWith("capture_") && window.label !== win.label) await window.close();
      }
    } catch (error) {
      console.warn("Could not close other windows", error);
    }

    await win.hide();
    try {
      const stitchedBase64 = await invoke<string>("auto_scroll_capture_window", {
        monitorIndex,
        selectionX: options.cropRect.x,
        selectionY: options.cropRect.y,
        selectionWidth: options.cropRect.w,
        selectionHeight: options.cropRect.h,
      });
      const img = new Image();
      img.src = stitchedBase64;
      img.onload = async () => {
        options.imageRef.current = img;
        options.setImageLoaded(true);
        if (options.canvasRef.current) {
          options.canvasRef.current.width = img.width;
          options.canvasRef.current.height = img.height;
          options.setCropRect({ x: 0, y: 0, w: img.width, h: img.height });
        }
        options.setIsStitchedResult(true);
        options.setShapes([]);
        options.setIsStitching(false);
        options.isScrollingModeRef.current = false;
        options.setIsScrollingMode(false);
        await win.show();
        await win.setFocus();
        try {
          await invoke("copy_screenshot_to_clipboard", { base64Image: stitchedBase64 });
          options.showToast(options.scrollCancelRequestedRef.current
            ? "已中斷長截圖，已保留目前畫面並複製到剪貼簿"
            : "長截圖完成並已複製到剪貼簿！可直接貼上使用");
        } catch {
          options.showToast("長截圖完成！可直接複製或存檔");
        }
      };
      img.onerror = async () => {
        options.setIsStitching(false);
        options.isScrollingModeRef.current = false;
        options.setIsScrollingMode(false);
        await win.show();
        await win.setFocus();
        options.showToast("長截圖載入失敗");
      };
    } catch (error) {
      console.error("Auto scroll error:", error);
      options.setIsStitching(false);
      options.isScrollingModeRef.current = false;
      options.setIsScrollingMode(false);
      await win.show();
      await win.setFocus();
      options.showToast(`長截圖失敗：${String(error).slice(0, 60)}`);
    }
  }, [options]);
}
