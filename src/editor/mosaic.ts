/** Draws a pixelated mosaic over a region using the current source image. */
export function drawMosaic(
  target: CanvasRenderingContext2D,
  sourceImage: CanvasImageSource,
  rx: number,
  ry: number,
  rw: number,
  rh: number,
  size: number,
  canvasWidth: number,
  canvasHeight: number,
): void {
  const xStart = Math.max(0, Math.floor(Math.min(rx, rx + rw)));
  const yStart = Math.max(0, Math.floor(Math.min(ry, ry + rh)));
  const xEnd = Math.min(canvasWidth, Math.ceil(Math.max(rx, rx + rw)));
  const yEnd = Math.min(canvasHeight, Math.ceil(Math.max(ry, ry + rh)));
  const width = xEnd - xStart;
  const height = yEnd - yStart;
  const blockSize = Math.max(1, Math.floor(size));
  if (width <= 0 || height <= 0) return;

  // Only allocate the down-sampled mosaic region. The previous implementation
  // allocated a viewport-sized canvas and then read native long-screenshot
  // coordinates from it; pixels below the viewport were transparent and were
  // consequently rendered as black blocks.
  const tempCanvas = document.createElement("canvas");
  tempCanvas.width = Math.max(1, Math.ceil(width / blockSize));
  tempCanvas.height = Math.max(1, Math.ceil(height / blockSize));
  const tempContext = tempCanvas.getContext("2d");
  if (!tempContext) return;

  const dimensions = sourceImage as CanvasImageSource & {
    naturalWidth?: number;
    naturalHeight?: number;
    videoWidth?: number;
    videoHeight?: number;
    width?: number;
    height?: number;
  };
  const sourceWidth = dimensions.naturalWidth || dimensions.videoWidth || dimensions.width || canvasWidth;
  const sourceHeight = dimensions.naturalHeight || dimensions.videoHeight || dimensions.height || canvasHeight;
  const scaleX = sourceWidth / canvasWidth;
  const scaleY = sourceHeight / canvasHeight;

  tempContext.imageSmoothingEnabled = true;
  tempContext.imageSmoothingQuality = "high";
  tempContext.drawImage(
    sourceImage,
    xStart * scaleX,
    yStart * scaleY,
    width * scaleX,
    height * scaleY,
    0,
    0,
    tempCanvas.width,
    tempCanvas.height,
  );

  target.save();
  target.imageSmoothingEnabled = false;
  target.drawImage(tempCanvas, 0, 0, tempCanvas.width, tempCanvas.height, xStart, yStart, width, height);
  target.restore();
}
