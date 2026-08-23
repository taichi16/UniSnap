import { useCallback, useEffect, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCaptureMonitorIndex } from "../editor/windowIdentity";

interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface RecordingStartOptions {
  label: string;
  cropRect: CropRect | null;
  isStartingRecording: boolean;
  setIsStartingRecording: Dispatch<SetStateAction<boolean>>;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  recordAudio: boolean;
  recordSystemAudio: boolean;
  recordingFps: number;
  showToast: (message: string) => void;
}

export function useRecordingStart(options: RecordingStartOptions) {
  const startRecordingControl = useCallback(async () => {
    if (!options.cropRect || options.isStartingRecording) return;
    const monitorIndex = getCaptureMonitorIndex(options.label);
    const captureWindow = getCurrentWindow();
    try {
      options.setIsStartingRecording(true);
      // Hide the selection panel before the OS starts collecting frames.
      await captureWindow.hide();
      await new Promise((resolve) => window.setTimeout(resolve, 120));
      await invoke("start_recording", {
        monitorIndex,
        x: Math.round(options.cropRect.x),
        y: Math.round(options.cropRect.y),
        width: Math.round(options.cropRect.w),
        height: Math.round(options.cropRect.h),
        canvasWidth: options.canvasRef.current?.width ?? window.innerWidth,
        canvasHeight: options.canvasRef.current?.height ?? window.innerHeight,
        recordAudio: options.recordAudio,
        recordSystemAudio: options.recordSystemAudio,
        fps: options.recordingFps,
      });
      window.location.hash = "#/recording-control";
    } catch (error) {
      options.setIsStartingRecording(false);
      await captureWindow.show().catch(() => {});
      await captureWindow.setFocus().catch(() => {});
      options.showToast(`啟動錄影失敗：${String(error)}`);
    }
  }, [options]);

  useEffect(() => {
    if (!options.isStartingRecording) return;
    const timeout = window.setTimeout(() => {
      options.setIsStartingRecording(false);
      options.showToast("錄影啟動逾時");
    }, 8000);
    return () => window.clearTimeout(timeout);
  }, [options.isStartingRecording, options.setIsStartingRecording, options.showToast]);

  return startRecordingControl;
}
