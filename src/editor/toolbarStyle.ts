import type { CSSProperties } from "react";
import { calculateToolbarPlacement } from "./toolbarLayout";
import type { EditorRect } from "./geometry";

export function getToolbarStyle(options: {
  isScrollableEditor: boolean;
  cropRect: EditorRect | null;
  canvasWidth: number;
  canvasHeight: number;
  displayWidth: number;
  displayHeight: number;
  toolbarWidth: number;
  viewport: { width: number; height: number };
}): CSSProperties {
  if (options.isScrollableEditor) {
    return {
      position: "sticky",
      top: 10,
      margin: "10px auto",
      transform: "none",
      zIndex: 100000,
      pointerEvents: "auto",
    };
  }

  if (!options.cropRect) return { display: "none" };

  const sx = options.displayWidth / Math.max(1, options.canvasWidth);
  const sy = options.displayHeight / Math.max(1, options.canvasHeight);
  const displayRect = {
    x: options.cropRect.x * sx,
    y: options.cropRect.y * sy,
    w: options.cropRect.w * sx,
    h: options.cropRect.h * sy,
  };
  return calculateToolbarPlacement(displayRect, options.toolbarWidth, options.viewport);
}
