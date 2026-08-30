import { useCallback, type MutableRefObject } from "react";
import { clientToCanvasPoint } from "../editor/geometry";
import type { Point } from "../editor/types";

export function useCanvasCoordinates(canvasRef: MutableRefObject<HTMLCanvasElement | null>) {
  return useCallback((clientX: number, clientY: number): Point => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: clientX, y: clientY };
    const rect = canvas.getBoundingClientRect();
    return clientToCanvasPoint(clientX, clientY, rect, canvas.width, canvas.height);
  }, [canvasRef]);
}
