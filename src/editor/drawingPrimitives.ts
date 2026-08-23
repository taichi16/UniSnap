import type { ArrowStyle, Point } from "./types";

export interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function drawHandles(ctx: CanvasRenderingContext2D, rect: CropRect) {
  const size = 6;
  ctx.fillStyle = "#ffffff";
  ctx.strokeStyle = "var(--accent-color)";
  ctx.lineWidth = 1.5;

  const points = [
    { x: rect.x, y: rect.y },
    { x: rect.x + rect.w / 2, y: rect.y },
    { x: rect.x + rect.w, y: rect.y },
    { x: rect.x + rect.w, y: rect.y + rect.h / 2 },
    { x: rect.x + rect.w, y: rect.y + rect.h },
    { x: rect.x + rect.w / 2, y: rect.y + rect.h },
    { x: rect.x, y: rect.y + rect.h },
    { x: rect.x, y: rect.y + rect.h / 2 },
  ];

  points.forEach((point) => {
    ctx.fillRect(point.x - size / 2, point.y - size / 2, size, size);
    ctx.strokeRect(point.x - size / 2, point.y - size / 2, size, size);
  });
}

function drawArrowHead(ctx: CanvasRenderingContext2D, tip: Point, angle: number, headLength: number, style: ArrowStyle) {
  if (style === "chevron") {
    const angleOffset = Math.PI / 5;
    ctx.beginPath();
    ctx.moveTo(tip.x - headLength * Math.cos(angle - angleOffset), tip.y - headLength * Math.sin(angle - angleOffset));
    ctx.lineTo(tip.x, tip.y);
    ctx.lineTo(tip.x - headLength * Math.cos(angle + angleOffset), tip.y - headLength * Math.sin(angle + angleOffset));
    ctx.stroke();
    return;
  }

  if (style === "block") {
    const blockWidth = headLength * 0.6;
    const perpendicular = angle + Math.PI / 2;
    const baseX = tip.x - headLength * Math.cos(angle);
    const baseY = tip.y - headLength * Math.sin(angle);
    ctx.beginPath();
    ctx.moveTo(tip.x, tip.y);
    ctx.lineTo(baseX + blockWidth * Math.cos(perpendicular), baseY + blockWidth * Math.sin(perpendicular));
    ctx.lineTo(baseX - blockWidth * Math.cos(perpendicular), baseY - blockWidth * Math.sin(perpendicular));
    ctx.closePath();
    ctx.fill();
    return;
  }

  const arrowAngle = Math.PI / 6;
  const x1 = tip.x - headLength * Math.cos(angle - arrowAngle);
  const y1 = tip.y - headLength * Math.sin(angle - arrowAngle);
  const x2 = tip.x - headLength * Math.cos(angle + arrowAngle);
  const y2 = tip.y - headLength * Math.sin(angle + arrowAngle);
  ctx.beginPath();
  ctx.moveTo(tip.x, tip.y);
  ctx.lineTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.closePath();
  ctx.fill();
}

export function drawArrow(ctx: CanvasRenderingContext2D, start: Point, end: Point, width: number, style: ArrowStyle = "single") {
  const angle = Math.atan2(end.y - start.y, end.x - start.x);
  const headLength = Math.max(14, width * 4);
  const isChevron = style === "chevron";

  if (style === "curve") {
    const middleX = (start.x + end.x) / 2;
    const middleY = (start.y + end.y) / 2;
    const controlX = middleX - (end.y - start.y) * 0.3;
    const controlY = middleY + (end.x - start.x) * 0.3;
    ctx.beginPath();
    ctx.moveTo(start.x, start.y);
    ctx.quadraticCurveTo(controlX, controlY, end.x, end.y);
    ctx.stroke();
    drawArrowHead(ctx, end, Math.atan2(end.y - controlY, end.x - controlX), headLength, style);
    return;
  }

  if (style === "elbow") {
    const midX = end.x;
    const midY = start.y;
    ctx.beginPath();
    ctx.moveTo(start.x, start.y);
    ctx.lineTo(midX, midY);
    ctx.lineTo(end.x, end.y - (end.y > start.y ? headLength / 2 : -headLength / 2));
    ctx.stroke();
    drawArrowHead(ctx, end, end.y >= start.y ? Math.PI / 2 : -Math.PI / 2, headLength, "single");
    return;
  }

  const shaftEndX = end.x - (isChevron ? 0 : headLength / 2) * Math.cos(angle);
  const shaftEndY = end.y - (isChevron ? 0 : headLength / 2) * Math.sin(angle);
  const shaftStartX = style === "double" ? start.x + headLength / 2 * Math.cos(angle) : start.x;
  const shaftStartY = style === "double" ? start.y + headLength / 2 * Math.sin(angle) : start.y;
  ctx.beginPath();
  ctx.moveTo(shaftStartX, shaftStartY);
  ctx.lineTo(shaftEndX, shaftEndY);
  ctx.stroke();
  drawArrowHead(ctx, end, angle, headLength, style);
  if (style === "double") drawArrowHead(ctx, start, angle + Math.PI, headLength, "single");
}
