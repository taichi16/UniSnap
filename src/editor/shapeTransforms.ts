import type { Point, Shape } from "./types";

export function updateShapeEndpoint(shape: Shape, point: Point): Shape {
  if (shape.type === "pen" || shape.type === "highlighter") {
    return { ...shape, points: [...shape.points, point] };
  }
  if (shape.type === "line" || shape.type === "arrow") {
    return { ...shape, end: point };
  }
  if (shape.type === "rect" || shape.type === "circle" || shape.type === "mosaic") {
    return { ...shape, w: point.x - shape.x, h: point.y - shape.y };
  }
  return shape;
}
