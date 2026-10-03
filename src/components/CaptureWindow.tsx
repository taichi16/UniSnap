import { useState, useRef, useEffect } from "react";
import { Monitor } from "lucide-react";

import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { getCanvasOverlayPosition } from "../editor/geometry";
import { cropCanvasToBase64 } from "../editor/imageExport";
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
import { useCanvasCoordinates } from "../hooks/useCanvasCoordinates";
import { useCopyScreenshot } from "../hooks/useCopyScreenshot";
import { usePinScreenshot } from "../hooks/usePinScreenshot";
import { useSaveScreenshot } from "../hooks/useSaveScreenshot";
import { useConfirmScreenshot } from "../hooks/useConfirmScreenshot";
import { useMosaicRenderer } from "../hooks/useMosaicRenderer";
import { useCanvasExpansion } from "../hooks/useCanvasExpansion";
import { useScrollCapture } from "../hooks/useScrollCapture";
import { useAnnotationActions } from "../hooks/useAnnotationActions";
import { useEditorPointerHandlers } from "../hooks/useEditorPointerHandlers";
import { useEditorLayout } from "../hooks/useEditorLayout";
export default function CaptureWindow({ label, mode = "screenshot", monitorIndex }: CaptureWindowProps) {
  const parsedMonitorIndex = monitorIndex ?? (() => {
    const match = label.match(/capture_(\d+)_/);
    return match ? parseInt(match[1], 10) : 0;
  })();

  const [showMonitorBadge, setShowMonitorBadge] = useState(true);

  useEffect(() => {
    const timer = setTimeout(() => {
      setShowMonitorBadge(false);
    }, 2400);
    return () => clearTimeout(timer);
  }, []);

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

  // Toast status
  const { toastMessage: toastMsg, showToast } = useToastMessage();

  const { showExpandDialog, setShowExpandDialog, expandValues, setExpandValues, handleExpandCanvas, applyExpandCanvas } = useCanvasExpansion(
    canvasRef,
    imageRef,
    cropRect,
    setCropRect,
    setShapes,
    setScreenshotData,
    setImageLoaded,
    setEditorImageSize,
    showToast,
  );

  // Scrolling Capture State
  const [isScrollingMode, setIsScrollingMode] = useState(false);
  const isScrollingModeRef = useRef(false);
  const scrollCancelRequestedRef = useRef(false);
  const [isStitching, setIsStitching] = useState(false);

  // Screen Recording State
  const [recordAudio, setRecordAudio] = useState(false);
  const [recordSystemAudio, setRecordSystemAudio] = useState(false);
  const [recordingFps, setRecordingFps] = useState(30);
  const [isStartingRecording, setIsStartingRecording] = useState(false);
  const [saveFormat, setSaveFormat] = useState<"png" | "jpg">("png");
  const isRecordMode = mode.toLowerCase().includes("record");
  const [isStitchedResult, setIsStitchedResult] = useState(false);
  const handleWindowScrollCapture = useScrollCapture({
    label,
    cropRect,
    imageRef,
    canvasRef,
    isScrollingModeRef,
    scrollCancelRequestedRef,
    setIsScrollingMode,
    setIsStitching,
    setIsStitchedResult,
    setImageLoaded,
    setCropRect,
    setShapes,
    showToast,
  });
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

  const drawMosaic = useMosaicRenderer(imageRef);

  useCanvasRedraw(canvasRef, imageRef, imageLoaded, cropRect, shapes, currentShape, drawMosaic);
  useColorPicker(imageRef, imageLoaded, cropRect, mousePos, setHoverColor);

  // Keep pointer coordinates in the image's native pixel space after visual scaling.
  const toCanvasPoint = useCanvasCoordinates(canvasRef);

  const { handleMouseDown, handleMouseMove, handleMouseUp } = useEditorPointerHandlers({
    toCanvasPoint, canvasRef, cropRect, setCropRect, isSelecting, setIsSelecting,
    selectStart, setSelectStart, shapes, setShapes, activeTool, strokeColor, strokeWidth,
    fillShape, fillOpacity, mosaicIntensity, lineStyle, arrowStyle, setCurrentShape, currentShape,
    setTextInput, textInputRef, draggingTextIndex, setDraggingTextIndex, textDragOffset,
    setTextDragOffset, resizeHandle, setResizeHandle, isDraggingCrop, setIsDraggingCrop,
    dragOffset, setDragOffset, setMousePos, isRecordMode, mode,
  });


  const { handleTextInputBlur, handleUndo } = useAnnotationActions(
    textInput,
    shapes,
    strokeColor,
    strokeWidth,
    textFont,
    setShapes,
    setTextInput,
  );


  // Actions
  const getCroppedCanvasBase64 = () => {
    if (!canvasRef.current || !cropRect) return null;
    return cropCanvasToBase64(canvasRef.current, cropRect);
  };

  const handleCopyOnly = useCopyScreenshot(canvasRef, cropRect, imageLoaded, showToast);

  const handleSave = useSaveScreenshot(canvasRef, cropRect, saveFormat, closeEditor, showToast);

  const handleConfirm = useConfirmScreenshot(canvasRef, cropRect, closeEditor, showToast);

  const handlePin = usePinScreenshot(canvasRef, cropRect, closeEditor, showToast);

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

  const { toolbarStyle, canvasSize: editorCanvasSize } = useEditorLayout({
    canvasRef,
    isScrollableEditor,
    isStitchedResult,
    cropRect,
    editorImageSize,
    toolbarWidth,
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
        // Once stitching finishes the result is an editor surface.  The
        // result canvas itself has a full-size cropRect, so checking only
        // cropRect would incorrectly render the pre-capture controls again
        // on top of the editing toolbar.
        showGuidance={mode === "scroll" && !isStitchedResult && !isScrollingMode && !isStitching && (!cropRect || cropRect.w < 80 || cropRect.h < 80)}
        hasSelection={mode === "scroll" && !isStitchedResult && Boolean(cropRect) && !isScrollingMode && !isStitching}
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
        <div ref={toolbarRef} className="toolbar-floating" style={toolbarStyle} onClick={(e) => e.stopPropagation()} onMouseDown={(e) => e.stopPropagation()} onMouseUp={(e) => e.stopPropagation()}>
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





      {/* Visual screen identification badge in capture overlay */}
      {showMonitorBadge && !cropRect && !isSelecting && (
        <div className="capture-screen-indicator-badge">
          <Monitor size={15} style={{ verticalAlign: "middle", marginRight: 6 }} />
          <span>螢幕 {parsedMonitorIndex + 1}</span>
          {editorImageSize && (
            <span className="capture-screen-indicator-res">
              {editorImageSize.width} × {editorImageSize.height}
            </span>
          )}
        </div>
      )}

      {toastMsg && <ToastMessage message={toastMsg} />}
    </div>
  );
}
