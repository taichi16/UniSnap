import { useEffect, type RefObject } from "react";
import { drawHandles } from "../editor/drawingPrimitives";
import { drawShape, type MosaicRenderer } from "../editor/drawShape";
import type { Shape } from "../editor/types";

export function useCanvasRedraw(
  canvasRef: RefObject<HTMLCanvasElement | null>,
  imageRef: RefObject<HTMLImageElement | null>,
  imageLoaded: boolean,
  cropRect: { x: number; y: number; w: number; h: number } | null,
  shapes: Shape[],
  currentShape: Shape | null,
  drawMosaic: MosaicRenderer,
) {
  useEffect(() => {
    if (!imageLoaded || !canvasRef.current || !imageRef.current) return;
    const canvas = canvasRef.current;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.clearRect(0, 0, canvas.width, canvas.height);
    context.drawImage(imageRef.current, 0, 0, canvas.width, canvas.height);

    context.fillStyle = "rgba(0, 0, 0, 0.4)";
    if (!cropRect) {
      context.fillRect(0, 0, canvas.width, canvas.height);
    } else {
      context.fillRect(0, 0, canvas.width, cropRect.y);
      context.fillRect(0, cropRect.y + cropRect.h, canvas.width, canvas.height - (cropRect.y + cropRect.h));
      context.fillRect(0, cropRect.y, cropRect.x, cropRect.h);
      context.fillRect(cropRect.x + cropRect.w, cropRect.y, canvas.width - (cropRect.x + cropRect.w), cropRect.h);
      context.strokeStyle = "rgba(99, 102, 241, 0.9)";
      context.lineWidth = 1;
      context.strokeRect(cropRect.x, cropRect.y, cropRect.w, cropRect.h);
      drawHandles(context, cropRect);
    }

    context.save();
    if (cropRect) {
      context.beginPath();
      context.rect(cropRect.x, cropRect.y, cropRect.w, cropRect.h);
      context.clip();
    }
    shapes.forEach((shape) => drawShape(context, shape, drawMosaic));
    if (currentShape) drawShape(context, currentShape, drawMosaic);
    context.restore();
  // Preserve the original redraw triggers; the renderer is a stable-purpose
  // callback whose captured image ref is read at execution time.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [imageLoaded, cropRect, shapes, currentShape]);
}
