import type { Point, Shape } from "./types";

export interface EditorRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function createSelectionRect(start: Point, current: Point): EditorRect {
  return {
    x: Math.min(current.x, start.x),
    y: Math.min(current.y, start.y),
    w: Math.abs(current.x - start.x),
    h: Math.abs(current.y - start.y),
  };
}

export function resizeEditorRect(rect: EditorRect, handle: string, point: Point): EditorRect {
  let { x, y, w, h } = rect;
  const right = x + w;
  const bottom = y + h;
  if (handle.includes("L")) {
    x = Math.min(point.x, right - 10);
    w = right - x;
  }
  if (handle.includes("R")) w = Math.max(10, point.x - x);
  if (handle.includes("T")) {
    y = Math.min(point.y, bottom - 10);
    h = bottom - y;
  }
  if (handle.includes("B")) h = Math.max(10, point.y - y);
  return { x, y, w, h };
}

export function moveEditorRect(rect: EditorRect, point: Point, offset: Point, bounds: { width: number; height: number }): EditorRect {
  const x = Math.max(0, Math.min(bounds.width - rect.w, point.x - offset.x));
  const y = Math.max(0, Math.min(bounds.height - rect.h, point.y - offset.y));
  return { ...rect, x, y };
}

/** Returns the crop resize handle under a pointer, if any. */
export function getHandleAt(point: Point, rect: EditorRect): string | null {
  const hitSize = 12;
  const handles: Record<string, Point> = {
    TL: { x: rect.x, y: rect.y },
    TM: { x: rect.x + rect.w / 2, y: rect.y },
    TR: { x: rect.x + rect.w, y: rect.y },
    MR: { x: rect.x + rect.w, y: rect.y + rect.h / 2 },
    BR: { x: rect.x + rect.w, y: rect.y + rect.h },
    BM: { x: rect.x + rect.w / 2, y: rect.y + rect.h },
    BL: { x: rect.x, y: rect.y + rect.h },
    ML: { x: rect.x, y: rect.y + rect.h / 2 },
  };

  for (const [name, position] of Object.entries(handles)) {
    if (Math.abs(point.x - position.x) < hitSize && Math.abs(point.y - position.y) < hitSize) {
      return name;
    }
  }
  return null;
}

export function isPointInRect(point: Point, rect: EditorRect): boolean {
  return point.x >= rect.x
    && point.x <= rect.x + rect.w
    && point.y >= rect.y
    && point.y <= rect.y + rect.h;
}

/** Finds the topmost text annotation under a pointer. */
export function findTextShapeIndex(shapes: Shape[], point: Point): number | undefined {
  return [...shapes.keys()].reverse().find((index) => {
    const shape = shapes[index];
    if (shape.type !== "text") return false;
    const width = Math.max(32, shape.text.length * shape.size * 0.72);
    const height = Math.max(28, shape.size * 1.8);
    return point.x >= shape.x - 8
      && point.x <= shape.x + width + 8
      && point.y >= shape.y - height
      && point.y <= shape.y + height;
  });
}

/** Converts a browser pointer position into the canvas' native pixel space. */
export function clientToCanvasPoint(
  clientX: number,
  clientY: number,
  bounds: { left: number; top: number; width: number; height: number },
  canvasWidth: number,
  canvasHeight: number,
): Point {
  return {
    x: Math.max(0, Math.min(canvasWidth, (clientX - bounds.left) * canvasWidth / Math.max(1, bounds.width))),
    y: Math.max(0, Math.min(canvasHeight, (clientY - bounds.top) * canvasHeight / Math.max(1, bounds.height))),
  };
}
