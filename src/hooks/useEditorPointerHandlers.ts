import { useCallback, type Dispatch, type MutableRefObject, type SetStateAction, type MouseEvent } from "react";
import { clampPointToRect, createSelectionRect, findTextShapeIndex, getCanvasPixelSize, getHandleAt, isPointInRect, moveEditorRect, resizeEditorRect } from "../editor/geometry";
import { createShapeForTool } from "../editor/shapeFactory";
import { updateShapeEndpoint } from "../editor/shapeTransforms";
import type { Point, Shape, Tool, ArrowStyle } from "../editor/types";

interface CropRect { x: number; y: number; w: number; h: number }
interface TextInput { x: number; y: number; text: string }

export function useEditorPointerHandlers(options: {
  toCanvasPoint: (x: number, y: number) => Point;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  cropRect: CropRect | null;
  setCropRect: Dispatch<SetStateAction<CropRect | null>>;
  isSelecting: boolean;
  setIsSelecting: Dispatch<SetStateAction<boolean>>;
  selectStart: Point;
  setSelectStart: Dispatch<SetStateAction<Point>>;
  shapes: Shape[];
  setShapes: Dispatch<SetStateAction<Shape[]>>;
  activeTool: Tool;
  strokeColor: string;
  strokeWidth: number;
  fillShape: boolean;
  fillOpacity: number;
  mosaicIntensity: number;
  lineStyle: "solid" | "dashed";
  arrowStyle: ArrowStyle;
  setCurrentShape: Dispatch<SetStateAction<Shape | null>>;
  currentShape: Shape | null;
  setTextInput: Dispatch<SetStateAction<TextInput | null>>;
  textInputRef: MutableRefObject<HTMLTextAreaElement | null>;
  draggingTextIndex: number | null;
  setDraggingTextIndex: Dispatch<SetStateAction<number | null>>;
  textDragOffset: Point;
  setTextDragOffset: Dispatch<SetStateAction<Point>>;
  resizeHandle: string | null;
  setResizeHandle: Dispatch<SetStateAction<string | null>>;
  isDraggingCrop: boolean;
  setIsDraggingCrop: Dispatch<SetStateAction<boolean>>;
  dragOffset: Point;
  setDragOffset: Dispatch<SetStateAction<Point>>;
  setMousePos: Dispatch<SetStateAction<Point>>;
  isRecordMode: boolean;
  mode: string;
}) {
  const handleMouseDown = useCallback((event: MouseEvent) => {
    const clientPos = options.toCanvasPoint(event.clientX, event.clientY);
    if (!options.cropRect) {
      options.setIsSelecting(true);
      options.setSelectStart(clientPos);
      options.setCropRect({ x: clientPos.x, y: clientPos.y, w: 0, h: 0 });
      return;
    }
    const textIndex = findTextShapeIndex(options.shapes, clientPos);
    if (textIndex !== undefined && options.shapes[textIndex].type === "text") {
      const shape = options.shapes[textIndex];
      if (shape.type === "text") {
        options.setDraggingTextIndex(textIndex);
        options.setTextDragOffset({ x: clientPos.x - shape.x, y: clientPos.y - shape.y });
        return;
      }
    }
    if (options.activeTool === "select") {
      const handle = getHandleAt(clientPos, options.cropRect);
      if (handle) { options.setResizeHandle(handle); return; }
      if (isPointInRect(clientPos, options.cropRect)) {
        options.setIsDraggingCrop(true);
        options.setDragOffset({ x: clientPos.x - options.cropRect.x, y: clientPos.y - options.cropRect.y });
        return;
      }
      options.setIsSelecting(true);
      options.setSelectStart(clientPos);
      options.setCropRect({ x: clientPos.x, y: clientPos.y, w: 0, h: 0 });
    } else {
      if (!isPointInRect(clientPos, options.cropRect)) return;
      const shape = createShapeForTool(options.activeTool, clientPos, {
        color: options.strokeColor,
        width: options.strokeWidth,
        fill: options.fillShape,
        opacity: options.fillOpacity,
        mosaicIntensity: options.mosaicIntensity,
        lineStyle: options.lineStyle,
        arrowStyle: options.arrowStyle,
      });
      if (shape) options.setCurrentShape(shape);
      else if (options.activeTool === "text") {
        options.setTextInput({ x: clientPos.x, y: clientPos.y, text: "" });
        setTimeout(() => options.textInputRef.current?.focus(), 50);
      }
    }
  }, [options]);

  const handleMouseMove = useCallback((event: MouseEvent) => {
    const clientPos = options.toCanvasPoint(event.clientX, event.clientY);
    options.setMousePos({ x: event.clientX, y: event.clientY });
    if (options.isSelecting && options.cropRect) {
      options.setCropRect(createSelectionRect(options.selectStart, clientPos));
      return;
    }
    if (options.draggingTextIndex !== null) {
      const shape = options.shapes[options.draggingTextIndex];
      if (shape?.type === "text") {
        const canvasSize = getCanvasPixelSize(options.canvasRef.current, { width: window.innerWidth, height: window.innerHeight });
        const nextX = Math.max(0, Math.min(canvasSize.width - 1, clientPos.x - options.textDragOffset.x));
        const nextY = Math.max(shape.size, Math.min(canvasSize.height - 1, clientPos.y - options.textDragOffset.y));
        options.setShapes((current) => current.map((item, index) => index === options.draggingTextIndex && item.type === "text" ? { ...item, x: nextX, y: nextY } : item));
      }
      return;
    }
    if (options.resizeHandle && options.cropRect) {
      options.setCropRect(resizeEditorRect(options.cropRect, options.resizeHandle, clientPos));
      return;
    }
    if (options.isDraggingCrop && options.cropRect) {
      options.setCropRect(moveEditorRect(options.cropRect, clientPos, options.dragOffset, getCanvasPixelSize(options.canvasRef.current, { width: window.innerWidth, height: window.innerHeight })));
      return;
    }
    if (options.currentShape && options.cropRect) {
      options.setCurrentShape(updateShapeEndpoint(options.currentShape, clampPointToRect(clientPos, options.cropRect)));
    }
  }, [options]);

  const handleMouseUp = useCallback((event: MouseEvent) => {
    if ((options.isRecordMode || options.mode === "scroll") && options.isSelecting) {
      options.setCropRect(createSelectionRect(options.selectStart, options.toCanvasPoint(event.clientX, event.clientY)));
    }
    options.setIsSelecting(false);
    options.setResizeHandle(null);
    options.setIsDraggingCrop(false);
    options.setDraggingTextIndex(null);
    if (options.currentShape) {
      options.setShapes([...options.shapes, options.currentShape]);
      options.setCurrentShape(null);
    }
  }, [options]);

  return { handleMouseDown, handleMouseMove, handleMouseUp };
}
