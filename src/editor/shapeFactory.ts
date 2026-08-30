import type { ArrowStyle, Point, Shape, Tool } from "./types";

export interface ShapeFactoryOptions {
  color: string;
  width: number;
  fill: boolean;
  opacity: number;
  mosaicIntensity: number;
  lineStyle: "solid" | "dashed";
  arrowStyle: ArrowStyle;
}

/** Creates the initial annotation shape for a pointer-down event. */
export function createShapeForTool(tool: Tool, point: Point, options: ShapeFactoryOptions): Shape | null {
  switch (tool) {
    case "pen":
      return { type: "pen", points: [point], color: options.color, width: options.width };
    case "highlighter":
      return { type: "highlighter", points: [point], color: options.color, width: 16 };
    case "line":
      return { type: "line", start: point, end: point, color: options.color, width: options.width, style: options.lineStyle };
    case "arrow":
      return { type: "arrow", start: point, end: point, color: options.color, width: options.width, arrowStyle: options.arrowStyle };
    case "rect":
      return { type: "rect", x: point.x, y: point.y, w: 0, h: 0, color: options.color, width: options.width, fill: options.fill, style: options.lineStyle, opacity: options.opacity };
    case "circle":
      return { type: "circle", x: point.x, y: point.y, w: 0, h: 0, color: options.color, width: options.width, fill: options.fill, style: options.lineStyle, opacity: options.opacity };
    case "mosaic":
      return { type: "mosaic", x: point.x, y: point.y, w: 0, h: 0, intensity: options.mosaicIntensity };
    default:
      return null;
  }
}

export function createTextShape(
  input: { x: number; y: number; text: string },
  color: string,
  size: number,
  fontFamily: string,
): Shape {
  return {
    type: "text",
    x: input.x,
    y: input.y,
    text: input.text,
    color,
    size,
    fontFamily,
  };
}
