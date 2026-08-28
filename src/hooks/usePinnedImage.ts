import { useEffect, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";

interface ImageSize {
  width: number;
  height: number;
}

interface SelectionRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function usePinnedImage(
  label: string,
  mode: string,
  isMainEditor: boolean,
  imageRef: MutableRefObject<HTMLImageElement | null>,
  setScreenshotData: Dispatch<SetStateAction<string | null>>,
  setImageLoaded: Dispatch<SetStateAction<boolean>>,
  setEditorImageSize: Dispatch<SetStateAction<ImageSize | null>>,
  setCropRect: Dispatch<SetStateAction<SelectionRect | null>>,
) {
  useEffect(() => {
    async function fetchScreenshot() {
      try {
        console.log("[usePinnedImage] fetching label:", label);
        const data = await invoke<string>("get_pinned_image", { label });
        console.log("[usePinnedImage] got data length:", data?.length ?? 0);
        setScreenshotData(data);
        const img = new Image();
        img.onload = () => {
          imageRef.current = img;
          if (isMainEditor) {
            setEditorImageSize({ width: img.width, height: img.height });
          }
          setImageLoaded(true);
          if (mode === "edit" || mode === "edit-main") {
            setCropRect({ x: 0, y: 0, w: img.width, h: img.height });
          }
        };
        img.onerror = (e) => {
          console.error("[usePinnedImage] image failed to load from data URL:", e);
        };
        // Register before assigning src so cached local images still initialise
        // the editor selection and toolbar state correctly.
        img.src = data;
      } catch (err) {
        console.error("[usePinnedImage] get_pinned_image failed for label:", label, "error:", err);
      }
    }
    if (label) fetchScreenshot();
  }, [label]);
}
