import React, { useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, getAllWindows } from "@tauri-apps/api/window";

import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { save } from "@tauri-apps/plugin-dialog";
import { clampPointToRect, clientToCanvasPoint, createSelectionRect, findTextShapeIndex, getCanvasOverlayPosition, getCanvasPixelSize, getEditorCanvasSize, getHandleAt, isPointInRect, moveEditorRect, resizeEditorRect } from "../editor/geometry";
import { drawMosaic as drawMosaicPixels } from "../editor/mosaic";
import { cropCanvasToBase64 } from "../editor/imageExport";
import { getToolbarStyle as calculateToolbarStyle } from "../editor/toolbarStyle";
import { createShapeForTool, createTextShape } from "../editor/shapeFactory";
import { expandSelectedCanvas, parseExpansionValues, translateShapes } from "../editor/canvasExpansion";
import { updateShapeEndpoint } from "../editor/shapeTransforms";
import { getCaptureMonitorIndex } from "../editor/windowIdentity";
import { EDITOR_COLORS, EDITOR_FONT_OPTIONS } from "../editor/constants";
import type { ArrowStyle, CaptureWindowProps, Point, Shape, Tool } from "../editor/types";
import EditorActions from "./EditorActions";
import EditorToolButtons from "./EditorToolButtons";
import ExpandCanvasDialog from "./ExpandCanvasDialog";
import OcrResultModal from "./OcrResultModal";
import RecordingSelectionControls from "./RecordingSelectionControls";
import ScrollCaptureControls from "./ScrollCaptureControls";
import StitchingOverlay from "./StitchingOverlay";
import ToastMessage from "./ToastMessage";
import ToolOptionsPopover from "./ToolOptionsPopover";
import { useResponsiveLayout } from "../hooks/useResponsiveLayout";
import { useToastMessage } from "../hooks/useToastMessage";
import { useEditorSelectionInitialization } from "../hooks/useEditorSelectionInitialization";
import { useColorPicker } from "../hooks/useColorPicker";
import { useCanvasRedraw } from "../hooks/useCanvasRedraw";
import { useEditorKeyboardShortcuts } from "../hooks/useEditorKeyboardShortcuts";
import { usePinnedImage } from "../hooks/usePinnedImage";
import { useCaptureWindowLifecycle } from "../hooks/useCaptureWindowLifecycle";
import { useScrollCaptureEvents } from "../hooks/useScrollCaptureEvents";
import { useRecordingStart } from "../hooks/useRecordingStart";
import { useEditorWindowActions } from "../hooks/useEditorWindowActions";
import { useOcr } from "../hooks/useOcr";
export default function CaptureWindow({ label, mode = "screenshot" }: CaptureWindowProps) {
  const [screenshotData, setScreenshotData] = useState<string | null>(null);
  const [imageLoaded, setImageLoaded] = useState(false);
  const [editorImageSize, setEditorImageSize] = useState<{ width: number; height: number } | null>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  const toolbarRef = useRef<HTMLDivElement | null>(null);

  // Canvas Refs
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  // Crop Box Selection State
  const [cropRect, setCropRect] = useState<{ x: number; y: number; w: number; h: number } | null>(null);
  const [isSelecting, setIsSelecting] = useState(false);
  const [selectStart, setSelectStart] = useState<Point>({ x: 0, y: 0 });
  const [resizeHandle, setResizeHandle] = useState<string | null>(null);
  const [isDraggingCrop, setIsDraggingCrop] = useState(false);
  const [dragOffset, setDragOffset] = useState<Point>({ x: 0, y: 0 });

  // Magnifier (Color Picker) State
  const [mousePos, setMousePos] = useState<Point>({ x: 0, y: 0 });
  const [hoverColor, setHoverColor] = useState("#000000");

  // Drawing Tools State
  const [activeTool, setActiveTool] = useState<Tool>("select");
  const [strokeColor, setStrokeColor] = useState("#ff0000");
  const [strokeWidth, setStrokeWidth] = useState(3);
  const [fillShape, setFillShape] = useState(false);
  const [fillOpacity, setFillOpacity] = useState(100); // 0-100%
  const [mosaicIntensity, setMosaicIntensity] = useState(10);
  const [lineStyle, setLineStyle] = useState<"solid" | "dashed">("solid");
  const [arrowStyle, setArrowStyle] = useState<ArrowStyle>("single");
  // Fix 1: Font selection for text tool
  const [textFont, setTextFont] = useState("Inter, sans-serif");
  const [shapes, setShapes] = useState<Shape[]>([]);
  const [currentShape, setCurrentShape] = useState<Shape | null>(null);

  // Color palette helpers
  const [showColorPalette, setShowColorPalette] = useState(false);

  const chooseTool = (tool: Tool) => {
    setShowColorPalette(false);
    setActiveTool(tool);
  };

  const toggleColorPalette = () => {
    setShowColorPalette((visible) => !visible);
  };

  // Text Tool Editing Overlay State
  const [textInput, setTextInput] = useState<{ x: number; y: number; text: string } | null>(null);
  const textInputRef = useRef<HTMLTextAreaElement | null>(null);
  const [draggingTextIndex, setDraggingTextIndex] = useState<number | null>(null);
  const [textDragOffset, setTextDragOffset] = useState<Point>({ x: 0, y: 0 });

  const [showExpandDialog, setShowExpandDialog] = useState(false);
  const [expandValues, setExpandValues] = useState({ top: "0", right: "0", bottom: "0", left: "0" });

  // Toast status
  const { toastMessage: toastMsg, showToast } = useToastMessage();

  // Scrolling Capture State
  const [isScrollingMode, setIsScrollingMode] = useState(false);
  const isScrollingModeRef = useRef(false);
  const scrollCancelRequestedRef = useRef(false);
  const [isStitching, setIsStitching] = useState(false);

  const handleWindowScrollCapture = async () => {
    if (isScrollingModeRef.current) return;
    if (!cropRect || cropRect.w < 80 || cropRect.h < 80) {
      showToast("請先框選單一可捲動內容區域");
      return;
    }
    isScrollingModeRef.current = true;
    scrollCancelRequestedRef.current = false;
    setIsScrollingMode(true);
    setIsStitching(true);

    const win = getCurrentWindow();
    const monitorIndex = getCaptureMonitorIndex(label);

    // Close other capture windows on other monitors immediately so only one window remains
    try {
      const allWindows = await getAllWindows();
      for (const w of allWindows) {
        if (w.label.startsWith("capture_") && w.label !== win.label) {
          await w.close();
        }
      }
    } catch (e) {
      console.warn("Could not close other windows", e);
    }

    // Hide overlay so background window receives events
    await win.hide();


    try {
      const stitchedBase64 = await invoke<string>("auto_scroll_capture_window", {
        monitorIndex,
        selectionX: cropRect.x,
        selectionY: cropRect.y,
        selectionWidth: cropRect.w,
        selectionHeight: cropRect.h,
      });

      const img = new Image();
      img.src = stitchedBase64;
      img.onload = async () => {
        imageRef.current = img;
        setImageLoaded(true);
        if (canvasRef.current) {
          canvasRef.current.width = img.width;
          canvasRef.current.height = img.height;
          setCropRect({ x: 0, y: 0, w: img.width, h: img.height });
        }
        setIsStitchedResult(true);
        setShapes([]);
        setIsStitching(false);
        isScrollingModeRef.current = false;
        setIsScrollingMode(false);
        await win.show();
        await win.setFocus();

        // Auto-copy long screenshot to clipboard
        try {
          await invoke("copy_screenshot_to_clipboard", {
            base64Image: stitchedBase64,
          });
          showToast(scrollCancelRequestedRef.current
            ? "已中斷長截圖，已保留目前畫面並複製到剪貼簿"
            : "長截圖完成並已複製到剪貼簿！可直接貼上使用");
        } catch (e) {
          showToast("長截圖完成！可直接複製或存檔");
        }
      };
      img.onerror = async () => {
        setIsStitching(false);
        isScrollingModeRef.current = false;
        setIsScrollingMode(false);
        await win.show();
        await win.setFocus();
        showToast("長截圖載入失敗");
      };
    } catch (err: any) {
      console.error("Auto scroll error:", err);
      setIsStitching(false);
      isScrollingModeRef.current = false;
      setIsScrollingMode(false);
      await win.show();
      await win.setFocus();
      showToast(`長截圖失敗：${String(err).slice(0, 60)}`);
    }
  };





  // Screen Recording State
  const [recordAudio, setRecordAudio] = useState(false);
  const [recordSystemAudio, setRecordSystemAudio] = useState(false);
  const [recordingFps, setRecordingFps] = useState(30);
  const [isStartingRecording, setIsStartingRecording] = useState(false);
  const [saveFormat, setSaveFormat] = useState<"png" | "jpg">("png");
  const isRecordMode = mode.toLowerCase().includes("record");
  const [isStitchedResult, setIsStitchedResult] = useState(false);
  // `main_editor_` is the Rust-side route contract for files opened from the
  // main window.  Keep it as a fallback in case a WebView restores a route
  // before its query parameters have been parsed.
  const isMainEditor = mode === "edit-main" || label.startsWith("main_editor_");
  const isScrollableEditor = isMainEditor || isStitchedResult;
  const { toolbarWidth, viewport } = useResponsiveLayout(toolbarRef, [isScrollableEditor, Boolean(cropRect), activeTool, showColorPalette]);

  const { closeEditor, cancelScrollFlow } = useEditorWindowActions(
    isMainEditor,
    isScrollingModeRef,
    scrollCancelRequestedRef,
  );

  const startRecordingControl = useRecordingStart({
    label,
    cropRect,
    isStartingRecording,
    setIsStartingRecording,
    canvasRef,
    recordAudio,
    recordSystemAudio,
    recordingFps,
    showToast,
  });

  // The capture window is hidden during scrolling, so Escape is routed
  // through the main window's global shortcut handler.
  useScrollCaptureEvents(
    mode,
    isScrollingModeRef,
    scrollCancelRequestedRef,
    cancelScrollFlow,
    showToast,
  );

  useCaptureWindowLifecycle(cropRect);

  usePinnedImage(
    label,
    mode,
    isMainEditor,
    imageRef,
    setScreenshotData,
    setImageLoaded,
    setEditorImageSize,
    setCropRect,
  );

  // Keep the editing surface initialisation separate from image decoding.
  // This guarantees that an old file opened from the native file picker has
  // a selection and a visible editing toolbar even if the image load event
  // is delivered before React has committed the canvas.
  useEditorSelectionInitialization(isMainEditor, imageLoaded, editorImageSize, cropRect, setCropRect);

  const drawMosaic = (ctx: CanvasRenderingContext2D, rx: number, ry: number, rw: number, rh: number, size: number) => {
    if (!imageRef.current) return;
    // Keep the established editor sampling dimensions while moving the pixel
    // processing implementation out of the window component.
    drawMosaicPixels(ctx, imageRef.current, rx, ry, rw, rh, size, window.innerWidth, window.innerHeight);
  };

  useCanvasRedraw(canvasRef, imageRef, imageLoaded, cropRect, shapes, currentShape, drawMosaic);
  useColorPicker(imageRef, imageLoaded, cropRect, mousePos, setHoverColor);

  // The editor window may fit a large image inside the available screen. Keep
  // pointer coordinates in the image's native pixel space so crop and
  // annotations remain accurate after that visual scaling.
  const toCanvasPoint = (clientX: number, clientY: number): Point => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: clientX, y: clientY };
    const rect = canvas.getBoundingClientRect();
    return clientToCanvasPoint(clientX, clientY, rect, canvas.width, canvas.height);
  };

  // Mouse Handlers
  const handleMouseDown = (e: React.MouseEvent) => {
    const clientPos = toCanvasPoint(e.clientX, e.clientY);

    if (!cropRect) {
      // First crop selection
      setIsSelecting(true);
      setSelectStart(clientPos);
      setCropRect({ x: clientPos.x, y: clientPos.y, w: 0, h: 0 });
      return;
    }


    // Text annotations remain movable until the image is saved. Handle this
    // before the active-tool branch so the user can drag a label immediately
    // after typing, without having to reselect the pointer tool first.
    const textIndex = findTextShapeIndex(shapes, clientPos);
    if (textIndex !== undefined) {
      const shape = shapes[textIndex];
      if (shape.type === "text") {
        setDraggingTextIndex(textIndex);
        setTextDragOffset({ x: clientPos.x - shape.x, y: clientPos.y - shape.y });
        return;
      }
    }

    if (activeTool === "select") {

      // Check handles first
      const handle = getHandleAt(clientPos, cropRect);
      if (handle) {
        setResizeHandle(handle);
        return;
      }

      // Check if dragging inside crop box
      if (isPointInRect(clientPos, cropRect)) {
        setIsDraggingCrop(true);
        setDragOffset({ x: clientPos.x - cropRect.x, y: clientPos.y - cropRect.y });
        return;
      }

      // If clicked outside selection, start a new crop box
      setIsSelecting(true);
      setSelectStart(clientPos);
      setCropRect({ x: clientPos.x, y: clientPos.y, w: 0, h: 0 });
    } else {
      // Draw annotations inside crop selection
      if (!isPointInRect(clientPos, cropRect)) return;

      const shape = createShapeForTool(activeTool, clientPos, {
        color: strokeColor,
        width: strokeWidth,
        fill: fillShape,
        opacity: fillOpacity,
        mosaicIntensity,
        lineStyle,
        arrowStyle,
      });
      if (shape) {
        setCurrentShape(shape);
      } else if (activeTool === "text") {
        setTextInput({ x: clientPos.x, y: clientPos.y, text: "" });
        setTimeout(() => textInputRef.current?.focus(), 50);
      }
    }
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    const clientPos = toCanvasPoint(e.clientX, e.clientY);
    setMousePos({ x: e.clientX, y: e.clientY });

    if (isSelecting && cropRect) {
      // Selection dragging
      setCropRect(createSelectionRect(selectStart, clientPos));
      return;
    }

    if (draggingTextIndex !== null) {
      const shape = shapes[draggingTextIndex];
      if (shape?.type === "text") {
        const canvasSize = getCanvasPixelSize(canvasRef.current, { width: window.innerWidth, height: window.innerHeight });
        const nextX = Math.max(0, Math.min(canvasSize.width - 1, clientPos.x - textDragOffset.x));
        const nextY = Math.max(shape.size, Math.min(canvasSize.height - 1, clientPos.y - textDragOffset.y));
        setShapes((current) => current.map((item, index) => index === draggingTextIndex && item.type === "text"
          ? { ...item, x: nextX, y: nextY }
          : item));
      }
      return;
    }

    if (resizeHandle && cropRect) {
      // Adjusting borders
      setCropRect(resizeEditorRect(cropRect, resizeHandle, clientPos));
      return;
    }

    if (isDraggingCrop && cropRect) {
      // Moving entire crop box
      setCropRect(moveEditorRect(cropRect, clientPos, dragOffset, getCanvasPixelSize(canvasRef.current, { width: window.innerWidth, height: window.innerHeight })));
      return;
    }

    // Annotation drawing
    if (currentShape && cropRect) {
      // Lock coordinates inside cropRect
      const lockedPos = clampPointToRect(clientPos, cropRect);

      setCurrentShape(updateShapeEndpoint(currentShape, lockedPos));
    }
  };

  const handleMouseUp = (e: React.MouseEvent) => {
    // Persist the exact last pointer position on selection completion.
    if ((isRecordMode || mode === "scroll") && isSelecting) {
      const point = toCanvasPoint(e.clientX, e.clientY);
      setCropRect(createSelectionRect(selectStart, point));
    }
    setIsSelecting(false);
    setResizeHandle(null);
    setIsDraggingCrop(false);
    setDraggingTextIndex(null);

    if (currentShape) {
      // Save shape
      setShapes([...shapes, currentShape]);
      setCurrentShape(null);
    }
  };


  const handleTextInputBlur = () => {
    if (textInput && textInput.text.trim()) {
      setShapes([
        ...shapes,
        createTextShape(textInput, strokeColor, strokeWidth * 6, textFont),
      ]);
    }
    setTextInput(null);
  };

  const handleUndo = () => {
    if (shapes.length > 0) {
      setShapes(shapes.slice(0, -1));
    }
  };

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
    const { top, right, bottom, left } = parseExpansionValues(expandValues);
    if (top + right + bottom + left === 0) {
      setShowExpandDialog(false);
      showToast("未輸入擴增像素");
      return;
    }

    const source = canvasRef.current;
    // Expansion applies to the current selection. When the editor has not
    // created a selection yet, fall back to the whole image for compatibility
    // with opening an image directly in the editor.
    const selection = cropRect ?? { x: 0, y: 0, w: source.width, h: source.height };
    const expanded = expandSelectedCanvas(source, selection, { top, right, bottom, left });
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
      setShapes((items) => translateShapes(items, expanded.selectionX, expanded.selectionY, left, top));
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

  // Actions
  const getCroppedCanvasBase64 = () => {
    if (!canvasRef.current || !cropRect) return null;
    return cropCanvasToBase64(canvasRef.current, cropRect);
  };

  const handleCopyOnly = async () => {
    console.debug("[clipboard-ui] copy start", {
      hasCanvas: Boolean(canvasRef.current),
      hasCropRect: Boolean(cropRect),
      cropRect,
      imageLoaded,
    });
    const base64 = getCroppedCanvasBase64();
    if (!base64) {
      const reason = !canvasRef.current
        ? "編輯畫布尚未建立"
        : !cropRect
          ? "尚未建立可複製的選取範圍"
          : "目前畫面尚未完成載入";
      console.error("[clipboard-ui] no image data", reason);
      showToast(`複製失敗：${reason}`);
      return;
    }

    try {
      console.debug("[clipboard-ui] invoking copy_screenshot_to_clipboard", {
        base64Chars: base64.length,
      });
      await invoke("copy_screenshot_to_clipboard", {
        base64Image: base64,
      });
      console.debug("[clipboard-ui] copy command succeeded");
      showToast("已複製截圖到剪貼簿！可直接貼上使用 (Cmd+V)");
    } catch (err) {
      console.error("Copy error:", err);
      showToast(`複製到剪貼簿失敗：${String(err)}`);
    }
  };

  const handleSave = async () => {
    const base64 = getCroppedCanvasBase64();
    if (!base64) return;

    try {
      // Prompt user to save as a file
      const filepath = await save({
        filters: saveFormat === "png"
          ? [{ name: "PNG 圖片", extensions: ["png"] }]
          : [{ name: "JPG 圖片", extensions: ["jpg", "jpeg"] }],
        defaultPath: saveFormat === "png" ? "Screenshot.png" : "Screenshot.jpg",
      });

      if (filepath) {
        await invoke("save_and_copy_screenshot", {
          base64Image: base64,
          savePath: filepath,
          autoCopy: false,
        });
        showToast("圖片已儲存");
        setTimeout(async () => {
          await closeEditor();
        }, 600);
      }
    } catch (err) {
      console.error("Save error:", err);
      showToast("儲存失敗");
    }
  };

  const handleConfirm = async () => {
    const base64 = getCroppedCanvasBase64();
    if (!base64) return;

    try {
      // Decode default options
      const savedPath = await invoke<string>("save_and_copy_screenshot", {
        base64Image: base64,
        savePath: null,
        autoCopy: true,
      });
      showToast(`已存檔且複製到剪貼簿\n路徑: ${savedPath}`);
      setTimeout(async () => {
        await closeEditor();
      }, 800);
    } catch (err) {
      console.error("Confirm error:", err);
      showToast("存檔失敗");
    }
  };

  const handlePin = async () => {
    const base64 = getCroppedCanvasBase64();
    if (!base64 || !cropRect) return;

    try {
      // Calculate coordinates relative to screen
      await invoke("pin_screenshot", {
        imageBase64: base64,
        x: cropRect.x,
        y: cropRect.y,
        width: cropRect.w,
        height: cropRect.h,
      });
      
      // Close capture windows
      await closeEditor();
    } catch (err) {
      console.error("Pin error:", err);
      showToast("貼圖失敗");
    }
  };

  const { ocrText, setOcrText, ocrLoading, handleOCR } = useOcr(
    getCroppedCanvasBase64,
    showToast,
  );



  // Fix 4: Positioning floating toolbar - stays fixed at bottom when fullscreen selected
  useEditorKeyboardShortcuts({
    cropRect,
    hoverColor,
    isScrollingMode,
    shapes,
    mode,
    isStartingRecording,
    cancelScrollFlow,
    handleCopyOnly,
    startRecordingControl,
    writeColor: writeText,
    showToast,
    closeEditor,
  });

  const getToolbarStyle = (): React.CSSProperties => {
    const canvas = canvasRef.current;
    const bounds = canvas?.getBoundingClientRect();
    return calculateToolbarStyle({
      isScrollableEditor,
      cropRect,
      canvasWidth: canvas?.width ?? window.innerWidth,
      canvasHeight: canvas?.height ?? window.innerHeight,
      displayWidth: bounds?.width ?? window.innerWidth,
      displayHeight: bounds?.height ?? window.innerHeight,
      toolbarWidth,
      viewport,
    });
  };

  const editorCanvasSize = getEditorCanvasSize({
    scrollable: isScrollableEditor,
    stitchedResult: isStitchedResult,
    canvas: canvasRef.current,
    imageSize: editorImageSize,
    viewport,
  });

  return (
    <div
      className={`capture-container${isScrollableEditor ? " editor-main" : ""}`}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
    >
      <canvas
        ref={canvasRef}
        width={editorCanvasSize.width}
        height={editorCanvasSize.height}
        className="capture-canvas"
        style={isScrollableEditor ? { width: `${editorCanvasSize.width}px`, height: `${editorCanvasSize.height}px` } : undefined}
      />

      <ScrollCaptureControls
        showGuidance={mode === "scroll" && !isScrollingMode && !isStitching && (!cropRect || cropRect.w < 80 || cropRect.h < 80)}
        hasSelection={mode === "scroll" && Boolean(cropRect) && !isScrollingMode && !isStitching}
        onStart={() => void handleWindowScrollCapture()}
        onReset={() => { setCropRect(null); setIsSelecting(false); }}
        onCancel={() => void cancelScrollFlow()}
      />



      {/* Magnifier / Color Picker (Shows before any selection) */}
      {mode === "screenshot" && !cropRect && (
        <div
          className="magnifier-overlay"
          style={{
            top: mousePos.y - 130, // Position slightly offset from cursor
            left: mousePos.x - 60,
          }}
        >
          {/* We render a scaled circle using CSS border, drawing is simulated */}
          <div
            style={{
              width: "100%",
              height: "100%",
              background: `radial-gradient(circle, transparent 20%, rgba(0,0,0,0.1) 80%), url(${screenshotData})`,
              backgroundSize: `${window.innerWidth * 7}px ${window.innerHeight * 7}px`, // Zoom scale
              backgroundPosition: `${-mousePos.x * 7 + 60}px ${-mousePos.y * 7 + 60}px`,
              backgroundRepeat: "no-repeat",
              imageRendering: "pixelated"
            }}
          />
          {/* Target pointer lines */}
          <div style={{ position: "absolute", top: "50%", left: 0, right: 0, height: 1, backgroundColor: "rgba(255,0,0,0.5)" }}></div>
          <div style={{ position: "absolute", left: "50%", top: 0, bottom: 0, width: 1, backgroundColor: "rgba(255,0,0,0.5)" }}></div>
          <div className="magnifier-info">
            {hoverColor}
            <span style={{ fontSize: 7, display: "block", color: "#888", textAlign: "center" }}>按 C 複製</span>
          </div>
        </div>
      )}

      {/* HTML text input overlay on canvas */}
      {textInput && (
        <textarea
          ref={textInputRef}
          className="canvas-text-input"
          value={textInput.text}
          onChange={(e) => setTextInput({ ...textInput, text: e.target.value })}
          onBlur={handleTextInputBlur}
          style={{
            top: (() => {
              const canvas = canvasRef.current;
              const container = canvas?.parentElement;
              if (!canvas || !container) return textInput.y;
              const canvasRect = canvas.getBoundingClientRect();
              const containerRect = container.getBoundingClientRect();
              return getCanvasOverlayPosition(textInput, canvas, canvasRect, containerRect, isScrollableEditor).y;
            })(),
            left: (() => {
              const canvas = canvasRef.current;
              const container = canvas?.parentElement;
              if (!canvas || !container) return textInput.x;
              const canvasRect = canvas.getBoundingClientRect();
              const containerRect = container.getBoundingClientRect();
              return getCanvasOverlayPosition(textInput, canvas, canvasRect, containerRect, isScrollableEditor).x;
            })(),
            color: strokeColor,
            fontSize: `${strokeWidth * 6}px`,
            fontFamily: textFont,
            fontWeight: "bold",
            minWidth: 100,
            minHeight: 24,
          }}
        />
      )}

      {showExpandDialog && (
        <ExpandCanvasDialog
          values={expandValues}
          onChange={(side, value) => setExpandValues((current) => ({ ...current, [side]: value }))}
          onCancel={() => setShowExpandDialog(false)}
          onApply={applyExpandCanvas}
        />
      )}

      {/* Recording start control: rendered inside the selected monitor's
          overlay, not in a second native window. This keeps it visible on
          every monitor and at every macOS display scale. */}
      {cropRect && isRecordMode && (
        <RecordingSelectionControls
          recordAudio={recordAudio}
          onRecordAudioChange={setRecordAudio}
          recordSystemAudio={recordSystemAudio}
          onRecordSystemAudioChange={setRecordSystemAudio}
          fps={recordingFps}
          onFpsChange={setRecordingFps}
          isStarting={isStartingRecording}
          onStart={() => void startRecordingControl()}
          onCancel={() => void closeEditor()}
        />
      )}

      {/* Floating Editing Toolbar */}
      {(cropRect || isMainEditor) && !isRecordMode && (
        <div ref={toolbarRef} className="toolbar-floating" style={getToolbarStyle()} onClick={(e) => e.stopPropagation()} onMouseDown={(e) => e.stopPropagation()} onMouseUp={(e) => e.stopPropagation()}>
          <>
              <EditorToolButtons
                activeTool={activeTool}
                onChooseTool={chooseTool}
                onExpandCanvas={handleExpandCanvas}
              />

              {/* Color Selector */}
              <button
                className="toolbar-btn"
                onClick={toggleColorPalette}
                style={{ color: strokeColor }}
                data-tooltip="標記顏色"
              >
                <div style={{ width: 14, height: 14, borderRadius: "50%", background: strokeColor, border: "1px solid #fff" }}></div>
              </button>
              
              <ToolOptionsPopover
                activeTool={activeTool}
                showColorPalette={showColorPalette}
                colors={EDITOR_COLORS}
                strokeColor={strokeColor}
                onChooseColor={(color) => { setStrokeColor(color); setShowColorPalette(false); }}
                textFont={textFont}
                fontOptions={EDITOR_FONT_OPTIONS}
                onTextFontChange={setTextFont}
                strokeWidth={strokeWidth}
                onStrokeWidthChange={setStrokeWidth}
                arrowStyle={arrowStyle}
                onArrowStyleChange={setArrowStyle}
                fillShape={fillShape}
                onFillShapeChange={setFillShape}
                fillOpacity={fillOpacity}
                onFillOpacityChange={setFillOpacity}
                lineStyle={lineStyle}
                onLineStyleChange={setLineStyle}
                mosaicIntensity={mosaicIntensity}
                onMosaicIntensityChange={setMosaicIntensity}
              />

              <EditorActions
                canUndo={shapes.length > 0}
                onUndo={handleUndo}
                onPin={handlePin}
                onOCR={handleOCR}
                ocrLoading={ocrLoading}
                onCopy={handleCopyOnly}
                saveFormat={saveFormat}
                onSaveFormatChange={setSaveFormat}
                onSave={handleSave}
                onCancel={() => void closeEditor()}
                onConfirm={handleConfirm}
              />
          </>
        </div>
      )}

      {/* OCR Result Overlay Modal */}
      {ocrText && (
        <OcrResultModal text={ocrText} onClose={() => setOcrText(null)} />
      )}



      {/* Stitching Loading Overlay */}
      {isStitching && (
        <StitchingOverlay />
      )}





      {toastMsg && <ToastMessage message={toastMsg} />}
    </div>
  );
}
