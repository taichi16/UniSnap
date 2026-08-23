export interface CropArea {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Renders the selected canvas area as a PNG data URL for save/copy actions. */
export function cropCanvasToBase64(canvas: HTMLCanvasElement, crop: CropArea): string | null {
  const offscreen = document.createElement("canvas");
  offscreen.width = crop.w;
  offscreen.height = crop.h;
  const context = offscreen.getContext("2d");
  if (!context) return null;

  context.drawImage(
    canvas,
    crop.x, crop.y, crop.w, crop.h,
    0, 0, crop.w, crop.h,
  );
  return offscreen.toDataURL("image/png");
}
