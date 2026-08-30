import { useCallback, type MutableRefObject } from "react";
import { drawMosaic as drawMosaicPixels } from "../editor/mosaic";

export function useMosaicRenderer(imageRef: MutableRefObject<HTMLImageElement | null>) {
  return useCallback((
    ctx: CanvasRenderingContext2D,
    rx: number,
    ry: number,
    rw: number,
    rh: number,
    size: number,
  ) => {
    if (!imageRef.current) return;
    drawMosaicPixels(
      ctx,
      imageRef.current,
      rx,
      ry,
      rw,
      rh,
      size,
      // Long screenshots use a canvas larger than the viewport. Sampling
      // against window dimensions shifts source pixels and can produce black
      // blocks when the requested region falls outside the temporary canvas.
      ctx.canvas.width,
      ctx.canvas.height,
    );
  }, [imageRef]);
}
