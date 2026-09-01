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
  microphoneDeviceId: string;
  recordingFps: number;
  showToast: (message: string) => void;
}

export function useRecordingStart(options: RecordingStartOptions) {
  const startRecordingControl = useCallback(async () => {
    if (!options.cropRect || options.isStartingRecording) return;
    // Snapshot every value before the first await. Pointer-up, window hiding,
    // and React re-renders must not alter the rectangle sent to the backend.
    const cropRect = { ...options.cropRect };
    const canvasWidth = options.canvasRef.current?.width ?? window.innerWidth;
    const canvasHeight = options.canvasRef.current?.height ?? window.innerHeight;
    const monitorIndex = getCaptureMonitorIndex(options.label);
    const captureWindow = getCurrentWindow();
    let backendStarted = false;
    try {
      options.setIsStartingRecording(true);
      // Hide the selection panel before the OS starts collecting frames.
      await captureWindow.hide();
      await new Promise((resolve) => window.setTimeout(resolve, 120));
      await invoke("start_recording", {
        monitorIndex,
        x: Math.round(cropRect.x),
        y: Math.round(cropRect.y),
        width: Math.round(cropRect.w),
        height: Math.round(cropRect.h),
        canvasWidth,
        canvasHeight,
        recordAudio: options.recordAudio,
        recordSystemAudio: options.recordSystemAudio,
        microphoneDeviceId: options.microphoneDeviceId || null,
        fps: options.recordingFps,
      });
      backendStarted = true;
      await invoke("open_recording_control", {
        monitorIndex,
        x: cropRect.x,
        y: cropRect.y,
        width: cropRect.w,
        height: cropRect.h,
        fps: options.recordingFps,
        recordAudio: options.recordAudio,
      });
      window.location.hash = "#/recording-control";
    } catch (error) {
      if (backendStarted) await invoke("stop_recording").catch(() => {});
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
