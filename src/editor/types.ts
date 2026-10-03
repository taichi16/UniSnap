export interface CaptureWindowProps {
  label: string;
  mode?: string;
  monitorIndex?: number;
}

export type Tool =
  | "select"
  | "pen"
  | "highlighter"
  | "line"
  | "arrow"
  | "rect"
  | "circle"
  | "text"
  | "mosaic";

export type ArrowStyle = "single" | "double" | "curve" | "elbow" | "chevron" | "block";

export interface Point {
  x: number;
  y: number;
}

export type Shape =
  | { type: "pen"; points: Point[]; color: string; width: number }
  | { type: "highlighter"; points: Point[]; color: string; width: number }
  | { type: "line"; start: Point; end: Point; color: string; width: number; style?: "solid" | "dashed" }
  | { type: "arrow"; start: Point; end: Point; color: string; width: number; arrowStyle?: ArrowStyle }
  | { type: "rect"; x: number; y: number; w: number; h: number; color: string; width: number; fill: boolean; style?: "solid" | "dashed"; opacity?: number }
  | { type: "circle"; x: number; y: number; w: number; h: number; color: string; width: number; fill: boolean; style?: "solid" | "dashed"; opacity?: number }
  | { type: "text"; x: number; y: number; text: string; color: string; size: number; fontFamily: string }
  | { type: "mosaic"; x: number; y: number; w: number; h: number; intensity: number };
