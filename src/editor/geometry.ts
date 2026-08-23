import type { Point } from "./types";

export interface EditorRect {
  x: number;
  y: number;
  w: number;
  h: number;
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
