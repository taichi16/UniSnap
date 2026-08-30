import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Keeps capture-window focus and the pre-recording toolbar above the desktop. */
export function useCaptureWindowLifecycle(cropRect: CropRect | null) {
  useEffect(() => {
    const win = getCurrentWindow();
    win.setFocus().catch(() => {});
    window.focus();
  }, []);

  useEffect(() => {
    if (!cropRect) return;
    getCurrentWindow().setAlwaysOnTop(true).catch(() => {});
  }, [cropRect]);
}
