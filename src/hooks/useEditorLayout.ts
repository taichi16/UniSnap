import { useMemo, type MutableRefObject } from "react";
import { getEditorCanvasSize } from "../editor/geometry";
import { getToolbarStyle as calculateToolbarStyle } from "../editor/toolbarStyle";

interface CropRect { x: number; y: number; w: number; h: number }
interface Viewport { width: number; height: number }

export function useEditorLayout(options: {
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  isScrollableEditor: boolean;
  isStitchedResult: boolean;
  cropRect: CropRect | null;
  editorImageSize: { width: number; height: number } | null;
  toolbarWidth: number;
  viewport: Viewport;
}) {
  const toolbarStyle = useMemo(() => {
    const canvas = options.canvasRef.current;
    const bounds = canvas?.getBoundingClientRect();
    return calculateToolbarStyle({
      isScrollableEditor: options.isScrollableEditor,
      cropRect: options.cropRect,
      canvasWidth: canvas?.width ?? window.innerWidth,
      canvasHeight: canvas?.height ?? window.innerHeight,
      displayWidth: bounds?.width ?? window.innerWidth,
      displayHeight: bounds?.height ?? window.innerHeight,
      toolbarWidth: options.toolbarWidth,
      viewport: options.viewport,
    });
  }, [options]);

  const canvasSize = useMemo(() => getEditorCanvasSize({
    scrollable: options.isScrollableEditor,
    stitchedResult: options.isStitchedResult,
    canvas: options.canvasRef.current,
    imageSize: options.editorImageSize,
    viewport: options.viewport,
  }), [options]);

  return { toolbarStyle, canvasSize };
}
