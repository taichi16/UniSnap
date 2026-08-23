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
  const xStart = Math.min(rx, rx + rw);
  const yStart = Math.min(ry, ry + rh);
  const width = Math.abs(rw);
  const height = Math.abs(rh);
  if (width <= 0 || height <= 0 || size <= 0) return;

  const tempCanvas = document.createElement("canvas");
  tempCanvas.width = canvasWidth;
  tempCanvas.height = canvasHeight;
  const tempContext = tempCanvas.getContext("2d");
  if (!tempContext) return;
  tempContext.drawImage(sourceImage, 0, 0, tempCanvas.width, tempCanvas.height);

  let imageData: ImageData;
  try {
    imageData = tempContext.getImageData(xStart, yStart, width, height);
  } catch (error) {
    console.error("getImageData failed:", error);
    return;
  }

  const data = imageData.data;
  for (let y = 0; y < height; y += size) {
    for (let x = 0; x < width; x += size) {
      let red = 0;
      let green = 0;
      let blue = 0;
      let count = 0;
      for (let dy = 0; dy < size && y + dy < height; dy++) {
        for (let dx = 0; dx < size && x + dx < width; dx++) {
          const index = ((y + dy) * width + (x + dx)) * 4;
          if (index + 2 < data.length) {
            red += data[index];
            green += data[index + 1];
            blue += data[index + 2];
            count++;
          }
        }
      }
      if (count > 0) {
        target.fillStyle = `rgb(${Math.round(red / count)},${Math.round(green / count)},${Math.round(blue / count)})`;
        target.fillRect(xStart + x, yStart + y, Math.min(size, width - x), Math.min(size, height - y));
      }
    }
  }
}
