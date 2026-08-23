import type { Shape } from "./types";

export interface ExpansionValues {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export interface SelectionBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ExpandedCanvasResult {
  dataUrl: string;
  width: number;
  height: number;
  selectionX: number;
  selectionY: number;
}

export function getSelectionBounds(
  selection: { x: number; y: number; w: number; h: number },
  source: { width: number; height: number },
): SelectionBounds {
  const x = Math.max(0, Math.round(selection.x));
  const y = Math.max(0, Math.round(selection.y));
  return {
    x,
    y,
    width: Math.min(source.width - x, Math.max(1, Math.round(selection.w))),
    height: Math.min(source.height - y, Math.max(1, Math.round(selection.h))),
  };
}

export function parseExpansionValues(values: Record<keyof ExpansionValues, string>): ExpansionValues {
  const parse = (value: string) => Math.max(0, Math.round(Number(value) || 0));
  return { top: parse(values.top), right: parse(values.right), bottom: parse(values.bottom), left: parse(values.left) };
}

/**
 * Expands the current selection onto a white canvas and returns an image data URL.
 * Keeping DOM canvas work here makes the editor component responsible only for
 * state transitions while preserving the selected-region semantics.
 */
export function expandSelectedCanvas(
  source: HTMLCanvasElement,
  selection: { x: number; y: number; w: number; h: number },
  values: ExpansionValues,
): ExpandedCanvasResult | null {
  const { top, right, bottom, left } = values;
  const { x: selectionX, y: selectionY, width: selectionWidth, height: selectionHeight } = getSelectionBounds(selection, source);
  if (selectionWidth <= 0 || selectionHeight <= 0) return null;

  const selected = document.createElement("canvas");
  selected.width = selectionWidth;
  selected.height = selectionHeight;
  const selectedCtx = selected.getContext("2d");
  if (!selectedCtx) return null;
  selectedCtx.drawImage(
    source,
    selectionX,
    selectionY,
    selectionWidth,
    selectionHeight,
    0,
    0,
    selectionWidth,
    selectionHeight,
  );

  const expanded = document.createElement("canvas");
  expanded.width = selectionWidth + left + right;
  expanded.height = selectionHeight + top + bottom;
  const ctx = expanded.getContext("2d");
  if (!ctx) return null;
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, expanded.width, expanded.height);
  ctx.drawImage(selected, left, top);

  return {
    dataUrl: expanded.toDataURL("image/png"),
    width: expanded.width,
    height: expanded.height,
    selectionX,
    selectionY,
  };
}

export function translateShapes(
  shapes: Shape[],
  selectionX: number,
  selectionY: number,
  offsetX: number,
  offsetY: number,
): Shape[] {
  return shapes.map((shape) => {
    if ("x" in shape && "y" in shape) {
      return { ...shape, x: shape.x - selectionX + offsetX, y: shape.y - selectionY + offsetY } as Shape;
    }
    if ("start" in shape && "end" in shape) {
      return {
        ...shape,
        start: { x: shape.start.x - selectionX + offsetX, y: shape.start.y - selectionY + offsetY },
        end: { x: shape.end.x - selectionX + offsetX, y: shape.end.y - selectionY + offsetY },
      } as Shape;
    }
    if ("points" in shape) {
      return { ...shape, points: shape.points.map((point) => ({ x: point.x - selectionX + offsetX, y: point.y - selectionY + offsetY })) } as Shape;
    }
    return shape;
  });
}
