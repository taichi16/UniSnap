import { useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { expandSelectedCanvas, parseExpansionValues, translateShapes } from "../editor/canvasExpansion";
import type { Shape } from "../editor/types";

interface CropRect { x: number; y: number; w: number; h: number }
interface ExpansionInput { top: string; right: string; bottom: string; left: string }

export function useCanvasExpansion(
  canvasRef: MutableRefObject<HTMLCanvasElement | null>,
  imageRef: MutableRefObject<HTMLImageElement | null>,
  cropRect: CropRect | null,
  setCropRect: Dispatch<SetStateAction<CropRect | null>>,
  setShapes: Dispatch<SetStateAction<Shape[]>>,
  setScreenshotData: Dispatch<SetStateAction<string | null>>,
  setImageLoaded: Dispatch<SetStateAction<boolean>>,
  setEditorImageSize: Dispatch<SetStateAction<{ width: number; height: number } | null>>,
  showToast: (message: string) => void,
) {
  const [showExpandDialog, setShowExpandDialog] = useState(false);
  const [expandValues, setExpandValues] = useState<ExpansionInput>({ top: "0", right: "0", bottom: "0", left: "0" });

  const handleExpandCanvas = () => {
    if (!canvasRef.current || !imageRef.current) {
      showToast("圖片尚未載入完成，無法擴增畫布");
      return;
    }
    setExpandValues({ top: "0", right: "0", bottom: "0", left: "0" });
    setShowExpandDialog(true);
  };

  const applyExpandCanvas = () => {
    if (!canvasRef.current || !imageRef.current) return;
    const values = parseExpansionValues(expandValues);
    if (values.top + values.right + values.bottom + values.left === 0) {
      setShowExpandDialog(false);
      showToast("未輸入擴增像素");
      return;
    }
    const selection = cropRect ?? { x: 0, y: 0, w: canvasRef.current.width, h: canvasRef.current.height };
    const expanded = expandSelectedCanvas(canvasRef.current, selection, values);
    if (!expanded) {
      showToast("目前選取區域無法擴增");
      return;
    }
    const data = expanded.dataUrl;
    const img = new Image();
    img.onload = () => {
      imageRef.current = img;
      setScreenshotData(data);
      setImageLoaded(true);
      setShapes((items) => translateShapes(items, expanded.selectionX, expanded.selectionY, values.left, values.top));
      if (canvasRef.current) {
        canvasRef.current.width = expanded.width;
        canvasRef.current.height = expanded.height;
      }
      setEditorImageSize({ width: expanded.width, height: expanded.height });
      setCropRect({ x: 0, y: 0, w: expanded.width, h: expanded.height });
      showToast(`畫布已擴增至 ${expanded.width} × ${expanded.height}`);
    };
    img.src = data;
    setShowExpandDialog(false);
  };

  return { showExpandDialog, setShowExpandDialog, expandValues, setExpandValues, handleExpandCanvas, applyExpandCanvas };
}
