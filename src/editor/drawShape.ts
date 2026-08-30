import { drawArrow } from "./drawingPrimitives";
import type { Shape } from "./types";

export type MosaicRenderer = (ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number, intensity: number) => void;

export function drawShape(ctx: CanvasRenderingContext2D, shape: Shape, drawMosaic: MosaicRenderer) {
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  switch (shape.type) {
    case "pen":
    case "highlighter":
      ctx.strokeStyle = shape.color;
      ctx.lineWidth = shape.width;
      if (shape.points.length < 2) return;
      ctx.save();
      if (shape.type === "highlighter") ctx.globalAlpha = 0.45;
      ctx.beginPath();
      ctx.moveTo(shape.points[0].x, shape.points[0].y);
      for (let i = 1; i < shape.points.length; i++) ctx.lineTo(shape.points[i].x, shape.points[i].y);
      ctx.stroke();
      ctx.restore();
      break;
    case "line":
      ctx.strokeStyle = shape.color;
      ctx.lineWidth = shape.width;
      ctx.setLineDash(shape.style === "dashed" ? [6, 6] : []);
      ctx.beginPath();
      ctx.moveTo(shape.start.x, shape.start.y);
      ctx.lineTo(shape.end.x, shape.end.y);
      ctx.stroke();
      ctx.setLineDash([]);
      break;
    case "arrow":
      ctx.strokeStyle = shape.color;
      ctx.fillStyle = shape.color;
      ctx.lineWidth = shape.width;
      drawArrow(ctx, shape.start, shape.end, shape.width, shape.arrowStyle || "single");
      break;
    case "rect":
      ctx.strokeStyle = shape.color;
      ctx.fillStyle = shape.color;
      ctx.lineWidth = shape.width;
      ctx.setLineDash(shape.style === "dashed" ? [6, 6] : []);
      if (shape.fill) {
        ctx.save();
        ctx.globalAlpha = (shape.opacity ?? 100) / 100;
        ctx.fillRect(shape.x, shape.y, shape.w, shape.h);
        ctx.restore();
      } else {
        ctx.strokeRect(shape.x, shape.y, shape.w, shape.h);
      }
      ctx.setLineDash([]);
      break;
    case "circle":
      ctx.strokeStyle = shape.color;
      ctx.fillStyle = shape.color;
      ctx.lineWidth = shape.width;
      ctx.setLineDash(shape.style === "dashed" ? [6, 6] : []);
      ctx.beginPath();
      ctx.ellipse(shape.x + shape.w / 2, shape.y + shape.h / 2, Math.abs(shape.w) / 2, Math.abs(shape.h) / 2, 0, 0, 2 * Math.PI);
      if (shape.fill) {
        ctx.save();
        ctx.globalAlpha = (shape.opacity ?? 100) / 100;
        ctx.fill();
        ctx.restore();
      } else {
        ctx.stroke();
      }
      ctx.setLineDash([]);
      break;
    case "text":
      ctx.fillStyle = shape.color;
      ctx.font = `bold ${shape.size}px ${shape.fontFamily || "Inter, sans-serif"}`;
      ctx.textBaseline = "top";
      ctx.fillText(shape.text, shape.x, shape.y);
      break;
    case "mosaic":
      drawMosaic(ctx, shape.x, shape.y, shape.w, shape.h, shape.intensity);
      break;
  }
}
