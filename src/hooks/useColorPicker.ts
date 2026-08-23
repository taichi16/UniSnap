import { useEffect, type Dispatch, type RefObject, type SetStateAction } from "react";
import { rgbToHex } from "../editor/color";
import type { Point } from "../editor/types";

export function useColorPicker(
  imageRef: RefObject<HTMLImageElement | null>,
  imageLoaded: boolean,
  cropRect: { x: number; y: number; w: number; h: number } | null,
  mousePos: Point,
  setHoverColor: Dispatch<SetStateAction<string>>,
) {
  useEffect(() => {
    if (!imageLoaded || !imageRef.current || cropRect) return;
    const canvas = document.createElement("canvas");
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.drawImage(imageRef.current, 0, 0, canvas.width, canvas.height);
    try {
      const pixel = context.getImageData(mousePos.x, mousePos.y, 1, 1).data;
      setHoverColor(rgbToHex(pixel[0], pixel[1], pixel[2]));
    } catch {
      // Ignore boundary errors.
    }
  }, [mousePos, imageLoaded, cropRect, imageRef, setHoverColor]);
}
