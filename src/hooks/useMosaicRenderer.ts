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
      // The editor canvas can be taller than the viewport (long screenshots).
      // Using window dimensions made the temporary source canvas too short;
      // getImageData then returned transparent pixels rendered as black blocks.
      ctx.canvas.width,
      ctx.canvas.height,
    );
  }, [imageRef]);
}
