import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, getAllWindows, LogicalSize } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { save } from "@tauri-apps/plugin-dialog";
import Tesseract from "tesseract.js";
import { calculateToolbarPlacement } from "../editor/toolbarLayout";
import type { ArrowStyle, CaptureWindowProps, Point, Shape, Tool } from "../editor/types";
import EditorActions from "./EditorActions";
import EditorToolButtons from "./EditorToolButtons";
import ExpandCanvasDialog from "./ExpandCanvasDialog";
import ToolOptionsPopover from "./ToolOptionsPopover";
import {
  X,
} from "lucide-react";

export default function CaptureWindow({ label, mode = "screenshot" }: CaptureWindowProps) {
  const [screenshotData, setScreenshotData] = useState<string | null>(null);
  const [imageLoaded, setImageLoaded] = useState(false);
  const [editorImageSize, setEditorImageSize] = useState<{ width: number; height: number } | null>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  const toolbarRef = useRef<HTMLDivElement | null>(null);
  const [toolbarWidth, setToolbarWidth] = useState(700);
  const [viewport, setViewport] = useState(() => ({ width: window.innerWidth, height: window.innerHeight }));

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
  const fontOptions = [
    { label: "預設 (Inter)", value: "Inter, sans-serif" },
    { label: "Arial", value: "Arial, sans-serif" },
    { label: "Georgia", value: "Georgia, serif" },
    { label: "Courier", value: "Courier New, monospace" },
    { label: "黑體", value: "PingFang TC, Microsoft JhengHei, sans-serif" },
  ];
  const [shapes, setShapes] = useState<Shape[]>([]);
  const [currentShape, setCurrentShape] = useState<Shape | null>(null);

  // Color palette helpers
  const [showColorPalette, setShowColorPalette] = useState(false);
  const colorsList = ["#ff0000", "#00ff00", "#0000ff", "#ffff00", "#ff00ff", "#00ffff", "#ffffff", "#000000"];

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

  // OCR modal State
  const [ocrText, setOcrText] = useState<string | null>(null);
  const [ocrLoading, setOcrLoading] = useState(false);
  const [showExpandDialog, setShowExpandDialog] = useState(false);
  const [expandValues, setExpandValues] = useState({ top: "0", right: "0", bottom: "0", left: "0" });

  // Toast status
  const [toastMsg, setToastMsg] = useState<string | null>(null);

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
    const match = label.match(/capture_(\d+)_/);
    const monitorIndex = match ? parseInt(match[1]) : 0;

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

  // Keep toolbar placement responsive when the window, display scale, or
  // active tool changes the rendered toolbar width.
  useEffect(() => {
    const updateViewport = () => setViewport({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", updateViewport);

    const toolbar = toolbarRef.current;
    if (!toolbar) {
      return () => window.removeEventListener("resize", updateViewport);
    }

    const updateToolbarWidth = () => {
      const width = toolbar.getBoundingClientRect().width;
      if (width > 0) setToolbarWidth(Math.ceil(width));
    };
    updateToolbarWidth();
    const observer = typeof ResizeObserver !== "undefined" ? new ResizeObserver(updateToolbarWidth) : null;
    observer?.observe(toolbar);

    return () => {
      window.removeEventListener("resize", updateViewport);
      observer?.disconnect();
    };
  }, [isScrollableEditor, Boolean(cropRect), activeTool, showColorPalette]);

  const closeEditor = async () => {
    if (isMainEditor) {
      const current = getCurrentWindow();
      window.location.hash = "#/";
      await current.setSize(new LogicalSize(760, 112));
      return;
    }
    await invoke("close_capture_windows");
  };

  const cancelScrollFlow = async () => {
    if (isScrollingModeRef.current) {
      scrollCancelRequestedRef.current = true;
      await invoke("cancel_scroll_capture");
      return;
    }
    await invoke("cancel_scroll_capture");
    await closeEditor();
  };

  const startRecordingControl = async () => {
    if (!cropRect || isStartingRecording) return;
    const match = label.match(/capture_(\d+)_/);
    const monitorIndex = match ? parseInt(match[1], 10) : 0;
    const captureWindow = getCurrentWindow();
    try {
      setIsStartingRecording(true);
      // The start panel belongs to this capture window. Hide it before the
      // operating system starts collecting frames, so it is never recorded.
      await captureWindow.hide();
      await new Promise((resolve) => window.setTimeout(resolve, 120));
      await invoke("start_recording", {
        monitorIndex,
        x: Math.round(cropRect.x),
        y: Math.round(cropRect.y),
        width: Math.round(cropRect.w),
        height: Math.round(cropRect.h),
        canvasWidth: canvasRef.current?.width ?? window.innerWidth,
        canvasHeight: canvasRef.current?.height ?? window.innerHeight,
        recordAudio,
        recordSystemAudio,
        fps: recordingFps,
      });
      window.location.hash = "#/recording-control";
    } catch (error) {
      setIsStartingRecording(false);
      await captureWindow.show().catch(() => {});
      await captureWindow.setFocus().catch(() => {});
      showToast(`啟動錄影失敗：${String(error)}`);
    }
  };

  useEffect(() => {
    if (!isStartingRecording) return;
    const timeout = window.setTimeout(() => {
      setIsStartingRecording(false);
      showToast("錄影啟動逾時");
    }, 8000);
    return () => window.clearTimeout(timeout);
  }, [isStartingRecording]);

  // The capture window is hidden during scrolling, so Escape is routed
  // through the main window's global shortcut handler.
  useEffect(() => {
    if (mode === "record") return;
    let unlisten: (() => void) | undefined;
    listen("global-escape", async () => {
      if (isScrollingModeRef.current) {
        scrollCancelRequestedRef.current = true;
        await invoke("cancel_scroll_capture");
      } else {
        await cancelScrollFlow();
      }
    }).then((cleanup) => { unlisten = cleanup; });
    return () => { unlisten?.(); };
  }, [mode]);

  useEffect(() => {
    if (mode === "record") return;
    let unlisten: (() => void) | undefined;
    listen<{ strategy: string; message: string }>("scroll-capture-strategy", (event) => {
      if (event.payload?.message) showToast(event.payload.message);
    }).then((cleanup) => { unlisten = cleanup; });
    return () => { unlisten?.(); };
  }, [mode]);

  // Auto-focus window on mount so drag selection starts on the very first mouse down
  useEffect(() => {
    const win = getCurrentWindow();
    win.setFocus().catch(() => {});
    window.focus();
  }, []);

  // Keep the pre-recording toolbar above the captured desktop, including when
  // the selection touches the Dock/taskbar edge of a monitor.
  useEffect(() => {
    if (!cropRect) return;
    const win = getCurrentWindow();
    win.setAlwaysOnTop(true).catch(() => {});
  }, [cropRect]);

  // Fetch base64 image on mount
  useEffect(() => {
    async function fetchScreenshot() {
      try {
        const data = await invoke<string>("get_pinned_image", { label });
        setScreenshotData(data);
        const img = new Image();
        img.onload = () => {
          imageRef.current = img;
          if (isMainEditor) {
            setEditorImageSize({ width: img.width, height: img.height });
          }
          setImageLoaded(true);
          if (mode === "edit" || mode === "edit-main") {
            setCropRect({
              x: 0,
              y: 0,
              w: img.width,
              h: img.height,
            });
          }
        };
        // Register before assigning src.  Small local images may complete
        // immediately from cache; registering afterwards leaves the editor
        // with an image but without its editable crop and toolbar state.
        img.src = data;
      } catch (err) {
        console.error("Failed to load screenshot:", err);
      }
    }
    if (label) {
      fetchScreenshot();
    }
  }, [label]);

  // Keep the editing surface initialisation separate from image decoding.
  // This guarantees that an old file opened from the native file picker has
  // a selection and a visible editing toolbar even if the image load event
  // is delivered before React has committed the canvas.
  useEffect(() => {
    if (!isMainEditor || !imageLoaded || cropRect) return;
    const width = editorImageSize?.width ?? window.innerWidth;
    const height = editorImageSize?.height ?? window.innerHeight;
    setCropRect({ x: 0, y: 0, w: width, h: height });
  }, [isMainEditor, imageLoaded, cropRect, editorImageSize]);

  // Keyboard shortcut listener ('Cmd+C' / 'Ctrl+C' for copy, 'C' for color picker, 'Esc' to exit)
  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // Before scrolling begins, Escape cancels the whole long-screenshot
        // flow. During scrolling, let Rust finish the frames already captured.
        await cancelScrollFlow();
      } else if ((e.metaKey || e.ctrlKey) && (e.key === "c" || e.key === "C")) {
        // Cmd+C or Ctrl+C: Copy current cropped screenshot to clipboard
        e.preventDefault();
        await handleCopyOnly();
      } else if (!e.metaKey && !e.ctrlKey && (e.key === "c" || e.key === "C")) {
        if (!cropRect && hoverColor) {
          await writeText(hoverColor);
          showToast(`已複製顏色: ${hoverColor}`);
          setTimeout(async () => {
            await closeEditor();
          }, 600);
        }
      } else if (mode === "record" && cropRect && e.key === "Enter" && !isStartingRecording) {
        // Full-monitor selections can place the toolbar under the system
        // menu bar or Dock. Enter remains a reliable start shortcut.
        e.preventDefault();
        await startRecordingControl();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [cropRect, hoverColor, isScrollingMode, shapes, mode, isStartingRecording]);




  // Redraw loop
  useEffect(() => {
    if (!imageLoaded || !canvasRef.current || !imageRef.current) return;
    const canvas = canvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Clear canvas
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Draw background screenshot
    ctx.drawImage(imageRef.current, 0, 0, canvas.width, canvas.height);

    // Draw overlay dimming outside selection
    if (!cropRect) {
      ctx.fillStyle = "rgba(0, 0, 0, 0.4)";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
    } else {
      // Dim outside of cropRect
      ctx.fillStyle = "rgba(0, 0, 0, 0.4)";
      // Top
      ctx.fillRect(0, 0, canvas.width, cropRect.y);
      // Bottom
      ctx.fillRect(0, cropRect.y + cropRect.h, canvas.width, canvas.height - (cropRect.y + cropRect.h));
      // Left
      ctx.fillRect(0, cropRect.y, cropRect.x, cropRect.h);
      // Right
      ctx.fillRect(cropRect.x + cropRect.w, cropRect.y, canvas.width - (cropRect.x + cropRect.w), cropRect.h);

      // Draw bright border around selection
      ctx.strokeStyle = "rgba(99, 102, 241, 0.9)";
      ctx.lineWidth = 1;
      ctx.strokeRect(cropRect.x, cropRect.y, cropRect.w, cropRect.h);

      // Draw resize handles (small squares)
      drawHandles(ctx, cropRect);
    }

    // Draw all shapes in history
    ctx.save();
    // Clip drawing area to cropRect to prevent annotations spilling out of screenshot
    if (cropRect) {
      ctx.beginPath();
      ctx.rect(cropRect.x, cropRect.y, cropRect.w, cropRect.h);
      ctx.clip();
    }
    
    shapes.forEach((shape) => drawShape(ctx, shape));
    if (currentShape) {
      drawShape(ctx, currentShape);
    }
    ctx.restore();

  }, [imageLoaded, cropRect, shapes, currentShape]);

  // Color picker pixel reading
  useEffect(() => {
    if (!imageLoaded || !imageRef.current || cropRect) return;
    const canvas = document.createElement("canvas");
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.drawImage(imageRef.current, 0, 0, canvas.width, canvas.height);

    try {
      const pixel = ctx.getImageData(mousePos.x, mousePos.y, 1, 1).data;
      const hex = rgbToHex(pixel[0], pixel[1], pixel[2]);
      setHoverColor(hex);
    } catch (e) {
      // Ignore boundary errors
    }
  }, [mousePos, imageLoaded, cropRect]);

  const showToast = (msg: string) => {
    setToastMsg(msg);
    setTimeout(() => setToastMsg(null), 2000);
  };

  const rgbToHex = (r: number, g: number, b: number) => {
    const toHex = (c: number) => {
      const hex = c.toString(16);
      return hex.length === 1 ? "0" + hex : hex;
    };
    return "#" + toHex(r) + toHex(g) + toHex(b);
  };

  // Drawing Helper Functions
  const drawHandles = (ctx: CanvasRenderingContext2D, rect: { x: number; y: number; w: number; h: number }) => {
    const size = 6;
    ctx.fillStyle = "#ffffff";
    ctx.strokeStyle = "var(--accent-color)";
    ctx.lineWidth = 1.5;

    const points = [
      { x: rect.x, y: rect.y }, // TL
      { x: rect.x + rect.w / 2, y: rect.y }, // TM
      { x: rect.x + rect.w, y: rect.y }, // TR
      { x: rect.x + rect.w, y: rect.y + rect.h / 2 }, // MR
      { x: rect.x + rect.w, y: rect.y + rect.h }, // BR
      { x: rect.x + rect.w / 2, y: rect.y + rect.h }, // BM
      { x: rect.x, y: rect.y + rect.h }, // BL
      { x: rect.x, y: rect.y + rect.h / 2 }, // ML
    ];

    points.forEach((p) => {
      ctx.fillRect(p.x - size / 2, p.y - size / 2, size, size);
      ctx.strokeRect(p.x - size / 2, p.y - size / 2, size, size);
    });
  };

  const drawShape = (ctx: CanvasRenderingContext2D, shape: Shape) => {
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    switch (shape.type) {
      case "pen":
        ctx.strokeStyle = shape.color;
        ctx.lineWidth = shape.width;
        if (shape.points.length < 2) return;
        ctx.beginPath();
        ctx.moveTo(shape.points[0].x, shape.points[0].y);
        for (let i = 1; i < shape.points.length; i++) {
          ctx.lineTo(shape.points[i].x, shape.points[i].y);
        }
        ctx.stroke();
        break;
      case "highlighter":
        ctx.strokeStyle = shape.color;
        ctx.lineWidth = shape.width;
        if (shape.points.length < 2) return;
        ctx.save();
        ctx.globalAlpha = 0.45;
        ctx.beginPath();
        ctx.moveTo(shape.points[0].x, shape.points[0].y);
        for (let i = 1; i < shape.points.length; i++) {
          ctx.lineTo(shape.points[i].x, shape.points[i].y);
        }
        ctx.stroke();
        ctx.restore();
        break;
      case "line":
        ctx.strokeStyle = shape.color;
        ctx.lineWidth = shape.width;
        if (shape.style === "dashed") {
          ctx.setLineDash([6, 6]);
        } else {
          ctx.setLineDash([]);
        }
        ctx.beginPath();
        ctx.moveTo(shape.start.x, shape.start.y);
        ctx.lineTo(shape.end.x, shape.end.y);
        ctx.stroke();
        ctx.setLineDash([]);
        break;
      case "arrow":
        ctx.strokeStyle = shape.color;
        ctx.fillStyle = shape.color;
        ctx.lineWidth = shape.width;
        drawArrow(ctx, shape.start, shape.end, shape.width, shape.arrowStyle || "single");
        break;
      case "rect":
        ctx.strokeStyle = shape.color;
        ctx.fillStyle = shape.color;
        ctx.lineWidth = shape.width;
        if (shape.style === "dashed") {
          ctx.setLineDash([6, 6]);
        } else {
          ctx.setLineDash([]);
        }
        if (shape.fill) {
          ctx.save();
          ctx.globalAlpha = (shape.opacity ?? 100) / 100;
          ctx.fillRect(shape.x, shape.y, shape.w, shape.h);
          ctx.restore();
        } else {
          ctx.strokeRect(shape.x, shape.y, shape.w, shape.h);
        }
        ctx.setLineDash([]);
        break;
      case "circle":
        ctx.strokeStyle = shape.color;
        ctx.fillStyle = shape.color;
        ctx.lineWidth = shape.width;
        if (shape.style === "dashed") {
          ctx.setLineDash([6, 6]);
        } else {
          ctx.setLineDash([]);
        }
        ctx.beginPath();
        const rx = Math.abs(shape.w) / 2;
        const ry = Math.abs(shape.h) / 2;
        const cx = shape.x + shape.w / 2;
        const cy = shape.y + shape.h / 2;
        ctx.ellipse(cx, cy, rx, ry, 0, 0, 2 * Math.PI);
        if (shape.fill) {
          ctx.save();
          ctx.globalAlpha = (shape.opacity ?? 100) / 100;
          ctx.fill();
          ctx.restore();
        } else {
          ctx.stroke();
        }
        ctx.setLineDash([]);
        break;
      case "text":
        ctx.fillStyle = shape.color;
        ctx.font = `bold ${shape.size}px ${shape.fontFamily || "Inter, sans-serif"}`;
        ctx.textBaseline = "top";
        ctx.fillText(shape.text, shape.x, shape.y);
        break;
      case "mosaic":
        drawMosaic(ctx, shape.x, shape.y, shape.w, shape.h, shape.intensity);
        break;
    }
  };

  const drawArrowHead = (
    ctx: CanvasRenderingContext2D,
    tip: Point,
    angle: number,
    headLength: number,
    style: ArrowStyle
  ) => {
    if (style === "chevron") {
      // Hollow chevron / open arrowhead
      const a = Math.PI / 5;
      ctx.beginPath();
      ctx.moveTo(tip.x - headLength * Math.cos(angle - a), tip.y - headLength * Math.sin(angle - a));
      ctx.lineTo(tip.x, tip.y);
      ctx.lineTo(tip.x - headLength * Math.cos(angle + a), tip.y - headLength * Math.sin(angle + a));
      ctx.stroke();
    } else if (style === "block") {
      // Solid rectangular block arrowhead
      const bw = headLength * 0.6; // block width (perpendicular)
      const bh = headLength;       // block depth
      const perp = angle + Math.PI / 2;
      const baseX = tip.x - bh * Math.cos(angle);
      const baseY = tip.y - bh * Math.sin(angle);
      ctx.beginPath();
      ctx.moveTo(tip.x, tip.y);
      ctx.lineTo(baseX + bw * Math.cos(perp), baseY + bw * Math.sin(perp));
      ctx.lineTo(baseX - bw * Math.cos(perp), baseY - bw * Math.sin(perp));
      ctx.closePath();
      ctx.fill();
    } else {
      // Default solid triangle head
      const arrowAngle = Math.PI / 6;
      const x1 = tip.x - headLength * Math.cos(angle - arrowAngle);
      const y1 = tip.y - headLength * Math.sin(angle - arrowAngle);
      const x2 = tip.x - headLength * Math.cos(angle + arrowAngle);
      const y2 = tip.y - headLength * Math.sin(angle + arrowAngle);
      ctx.beginPath();
      ctx.moveTo(tip.x, tip.y);
      ctx.lineTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.closePath();
      ctx.fill();
    }
  };

  const drawArrow = (ctx: CanvasRenderingContext2D, start: Point, end: Point, width: number, style: ArrowStyle = "single") => {
    const angle = Math.atan2(end.y - start.y, end.x - start.x);
    const headLength = Math.max(14, width * 4);
    const isChevron = style === "chevron";

    if (style === "curve") {
      // Quadratic bezier curve arrow
      const mx = (start.x + end.x) / 2;
      const my = (start.y + end.y) / 2;
      const cpx = mx - (end.y - start.y) * 0.3;
      const cpy = my + (end.x - start.x) * 0.3;
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.quadraticCurveTo(cpx, cpy, end.x, end.y);
      ctx.stroke();
      // Arrow head at end tangent
      const tangentAngle = Math.atan2(end.y - cpy, end.x - cpx);
      drawArrowHead(ctx, end, tangentAngle, headLength, style);
      return;
    }

    if (style === "elbow") {
      // Right-angle elbow arrow: horizontal then vertical
      const midX = end.x;
      const midY = start.y;
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(midX, midY);
      ctx.lineTo(end.x, end.y - (end.y > start.y ? headLength / 2 : -headLength / 2));
      ctx.stroke();
      const elbowAngle = end.y >= start.y ? Math.PI / 2 : -Math.PI / 2;
      drawArrowHead(ctx, end, elbowAngle, headLength, "single");
      return;
    }

    // For single / double / chevron / block: draw shaft
    const shaftEndX = end.x - (isChevron ? 0 : headLength / 2) * Math.cos(angle);
    const shaftEndY = end.y - (isChevron ? 0 : headLength / 2) * Math.sin(angle);
    const shaftStartX = style === "double" ? start.x + headLength / 2 * Math.cos(angle) : start.x;
    const shaftStartY = style === "double" ? start.y + headLength / 2 * Math.sin(angle) : start.y;

    ctx.beginPath();
    ctx.moveTo(shaftStartX, shaftStartY);
    ctx.lineTo(shaftEndX, shaftEndY);
    ctx.stroke();

    // Draw end arrowhead
    drawArrowHead(ctx, end, angle, headLength, style);

    // Draw start arrowhead for double
    if (style === "double") {
      drawArrowHead(ctx, start, angle + Math.PI, headLength, "single");
    }
  };

  const drawMosaic = (ctx: CanvasRenderingContext2D, rx: number, ry: number, rw: number, rh: number, size: number) => {
    if (!imageRef.current) return;
    
    // Normalize coordinates to be positive and non-zero
    const xStart = Math.min(rx, rx + rw);
    const yStart = Math.min(ry, ry + rh);
    const width = Math.abs(rw);
    const height = Math.abs(rh);

    if (width <= 0 || height <= 0 || size <= 0) return;
    
    const tempCanvas = document.createElement("canvas");
    tempCanvas.width = window.innerWidth;
    tempCanvas.height = window.innerHeight;
    const tempCtx = tempCanvas.getContext("2d");
    if (!tempCtx) return;
    
    tempCtx.drawImage(imageRef.current, 0, 0, tempCanvas.width, tempCanvas.height);
    
    let imgData;
    try {
      imgData = tempCtx.getImageData(xStart, yStart, width, height);
    } catch (e) {
      console.error("getImageData failed:", e);
      return;
    }
    
    const data = imgData.data;

    for (let y = 0; y < height; y += size) {
      for (let x = 0; x < width; x += size) {
        let r = 0, g = 0, b = 0, count = 0;
        for (let dy = 0; dy < size && y + dy < height; dy++) {
          for (let dx = 0; dx < size && x + dx < width; dx++) {
            const index = ((y + dy) * width + (x + dx)) * 4;
            if (index + 2 < data.length) {
              r += data[index];
              g += data[index + 1];
              b += data[index + 2];
              count++;
            }
          }
        }
        
        if (count > 0) {
          const avgR = Math.round(r / count);
          const avgG = Math.round(g / count);
          const avgB = Math.round(b / count);

          ctx.fillStyle = `rgb(${avgR},${avgG},${avgB})`;
          ctx.fillRect(xStart + x, yStart + y, Math.min(size, width - x), Math.min(size, height - y));
        }
      }
    }
  };

  // Resize logic helpers
  const getHandleAt = (p: Point, rect: { x: number; y: number; w: number; h: number }) => {
    const size = 12; // Click target margin
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

    for (const [name, pos] of Object.entries(handles)) {
      if (Math.abs(p.x - pos.x) < size && Math.abs(p.y - pos.y) < size) {
        return name;
      }
    }
    return null;
  };

  const isPointInRect = (p: Point, rect: { x: number; y: number; w: number; h: number }) => {
    return p.x >= rect.x && p.x <= rect.x + rect.w && p.y >= rect.y && p.y <= rect.y + rect.h;
  };

  // The editor window may fit a large image inside the available screen. Keep
  // pointer coordinates in the image's native pixel space so crop and
  // annotations remain accurate after that visual scaling.
  const toCanvasPoint = (clientX: number, clientY: number): Point => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: clientX, y: clientY };
    const rect = canvas.getBoundingClientRect();
    return {
      x: Math.max(0, Math.min(canvas.width, (clientX - rect.left) * canvas.width / Math.max(1, rect.width))),
      y: Math.max(0, Math.min(canvas.height, (clientY - rect.top) * canvas.height / Math.max(1, rect.height))),
    };
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
    const textIndex = [...shapes.keys()].reverse().find((index) => {
      const shape = shapes[index];
      if (shape.type !== "text") return false;
      const width = Math.max(32, shape.text.length * shape.size * 0.72);
      const height = Math.max(28, shape.size * 1.8);
      return clientPos.x >= shape.x - 8 && clientPos.x <= shape.x + width + 8
        && clientPos.y >= shape.y - height && clientPos.y <= shape.y + height;
    });
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

      if (activeTool === "pen") {
        setCurrentShape({ type: "pen", points: [clientPos], color: strokeColor, width: strokeWidth });
      } else if (activeTool === "highlighter") {
        setCurrentShape({ type: "highlighter", points: [clientPos], color: strokeColor, width: 16 });
      } else if (activeTool === "line") {
        setCurrentShape({ type: "line", start: clientPos, end: clientPos, color: strokeColor, width: strokeWidth, style: lineStyle });
      } else if (activeTool === "arrow") {
        setCurrentShape({ type: "arrow", start: clientPos, end: clientPos, color: strokeColor, width: strokeWidth, arrowStyle });
      } else if (activeTool === "rect") {
        setCurrentShape({ type: "rect", x: clientPos.x, y: clientPos.y, w: 0, h: 0, color: strokeColor, width: strokeWidth, fill: fillShape, style: lineStyle, opacity: fillOpacity });
      } else if (activeTool === "circle") {
        setCurrentShape({ type: "circle", x: clientPos.x, y: clientPos.y, w: 0, h: 0, color: strokeColor, width: strokeWidth, fill: fillShape, style: lineStyle, opacity: fillOpacity });
      } else if (activeTool === "mosaic") {
        setCurrentShape({ type: "mosaic", x: clientPos.x, y: clientPos.y, w: 0, h: 0, intensity: mosaicIntensity });
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
      const x = Math.min(clientPos.x, selectStart.x);
      const y = Math.min(clientPos.y, selectStart.y);
      const w = Math.abs(clientPos.x - selectStart.x);
      const h = Math.abs(clientPos.y - selectStart.y);
      setCropRect({ x, y, w, h });
      return;
    }

    if (draggingTextIndex !== null) {
      const shape = shapes[draggingTextIndex];
      if (shape?.type === "text") {
        const nextX = Math.max(0, Math.min((canvasRef.current?.width ?? window.innerWidth) - 1, clientPos.x - textDragOffset.x));
        const nextY = Math.max(shape.size, Math.min((canvasRef.current?.height ?? window.innerHeight) - 1, clientPos.y - textDragOffset.y));
        setShapes((current) => current.map((item, index) => index === draggingTextIndex && item.type === "text"
          ? { ...item, x: nextX, y: nextY }
          : item));
      }
      return;
    }

    if (resizeHandle && cropRect) {
      // Adjusting borders
      let { x, y, w, h } = cropRect;
      const right = x + w;
      const bottom = y + h;

      if (resizeHandle.includes("L")) {
        x = Math.min(clientPos.x, right - 10);
        w = right - x;
      }
      if (resizeHandle.includes("R")) {
        w = Math.max(10, clientPos.x - x);
      }
      if (resizeHandle.includes("T")) {
        y = Math.min(clientPos.y, bottom - 10);
        h = bottom - y;
      }
      if (resizeHandle.includes("B")) {
        h = Math.max(10, clientPos.y - y);
      }

      setCropRect({ x, y, w, h });
      return;
    }

    if (isDraggingCrop && cropRect) {
      // Moving entire crop box
        const nx = Math.max(0, Math.min((canvasRef.current?.width ?? window.innerWidth) - cropRect.w, clientPos.x - dragOffset.x));
      const ny = Math.max(0, Math.min((canvasRef.current?.height ?? window.innerHeight) - cropRect.h, clientPos.y - dragOffset.y));
      setCropRect({ ...cropRect, x: nx, y: ny });
      return;
    }

    // Annotation drawing
    if (currentShape && cropRect) {
      // Lock coordinates inside cropRect
      const lockedPos = {
        x: Math.max(cropRect.x, Math.min(cropRect.x + cropRect.w, clientPos.x)),
        y: Math.max(cropRect.y, Math.min(cropRect.y + cropRect.h, clientPos.y)),
      };

      if (currentShape.type === "pen" || currentShape.type === "highlighter") {
        setCurrentShape({
          ...currentShape,
          points: [...currentShape.points, lockedPos],
        } as Shape);
      } else if (currentShape.type === "line" || currentShape.type === "arrow") {
        setCurrentShape({
          ...currentShape,
          end: lockedPos,
        } as Shape);
      } else if (currentShape.type === "rect" || currentShape.type === "circle" || currentShape.type === "mosaic") {
        const w = lockedPos.x - currentShape.x;
        const h = lockedPos.y - currentShape.y;
        setCurrentShape({
          ...currentShape,
          w,
          h,
        } as Shape);
      }
    }
  };

  const handleMouseUp = (e: React.MouseEvent) => {
    // Persist the exact last pointer position on selection completion.
    if ((isRecordMode || mode === "scroll") && isSelecting) {
      const point = toCanvasPoint(e.clientX, e.clientY);
      const rect = {
        x: Math.min(point.x, selectStart.x),
        y: Math.min(point.y, selectStart.y),
        w: Math.abs(point.x - selectStart.x),
        h: Math.abs(point.y - selectStart.y),
      };
      setCropRect(rect);
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
        {
          type: "text",
          x: textInput.x,
          y: textInput.y,
          text: textInput.text,
          color: strokeColor,
          size: strokeWidth * 6,
          fontFamily: textFont,
        },
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
    const top = Math.max(0, Math.round(Number(expandValues.top) || 0));
    const right = Math.max(0, Math.round(Number(expandValues.right) || 0));
    const bottom = Math.max(0, Math.round(Number(expandValues.bottom) || 0));
    const left = Math.max(0, Math.round(Number(expandValues.left) || 0));
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
    const selectionX = Math.max(0, Math.round(selection.x));
    const selectionY = Math.max(0, Math.round(selection.y));
    const selectionWidth = Math.min(source.width - selectionX, Math.max(1, Math.round(selection.w)));
    const selectionHeight = Math.min(source.height - selectionY, Math.max(1, Math.round(selection.h)));
    if (selectionWidth <= 0 || selectionHeight <= 0) {
      showToast("目前選取區域無法擴增");
      return;
    }

    const selected = document.createElement("canvas");
    selected.width = selectionWidth;
    selected.height = selectionHeight;
    const selectedCtx = selected.getContext("2d");
    if (!selectedCtx) return;
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
    if (!ctx) return;
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, expanded.width, expanded.height);
    ctx.drawImage(selected, left, top);
    const data = expanded.toDataURL("image/png");
    const img = new Image();
    img.onload = () => {
      imageRef.current = img;
      setScreenshotData(data);
      setImageLoaded(true);
      setShapes((items) => items.map((shape) => {
        if ("x" in shape && "y" in shape) return { ...shape, x: shape.x - selectionX + left, y: shape.y - selectionY + top } as Shape;
        if ("start" in shape && "end" in shape) return { ...shape, start: { x: shape.start.x - selectionX + left, y: shape.start.y - selectionY + top }, end: { x: shape.end.x - selectionX + left, y: shape.end.y - selectionY + top } } as Shape;
        if ("points" in shape) return { ...shape, points: shape.points.map((p) => ({ x: p.x - selectionX + left, y: p.y - selectionY + top })) } as Shape;
        return shape;
      }));
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
    const canvas = canvasRef.current;
    
    // Create offscreen canvas for the crop
    const offscreen = document.createElement("canvas");
    offscreen.width = cropRect.w;
    offscreen.height = cropRect.h;
    const ctx = offscreen.getContext("2d");
    if (!ctx) return null;

    // Draw the cropped portion from the main canvas
    ctx.drawImage(
      canvas,
      cropRect.x, cropRect.y, cropRect.w, cropRect.h, // Source
      0, 0, cropRect.w, cropRect.h // Target
    );

    return offscreen.toDataURL("image/png");
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

  const handleOCR = async () => {
    const base64 = getCroppedCanvasBase64();
    if (!base64) return;

    setOcrLoading(true);
    showToast("進行文字辨識中...");
    
    try {
      const result = await Tesseract.recognize(base64, "chi_tra+eng");
      setOcrText(result.data.text || "未辨識到任何文字。");
      await writeText(result.data.text || "");
      showToast("文字辨識完成，已複製到剪貼簿");
    } catch (err) {
      console.error("OCR error:", err);
      showToast("文字辨識失敗");
    } finally {
      setOcrLoading(false);
    }
  };



  // Fix 4: Positioning floating toolbar - stays fixed at bottom when fullscreen selected
  const getToolbarStyle = (): React.CSSProperties => {
    if (isScrollableEditor) {
      return {
        position: "sticky",
        top: 10,
        margin: "10px auto",
        transform: "none",
        zIndex: 100000,
        pointerEvents: "auto",
      };
    }

    if (!cropRect) return { display: "none" };

    const canvas = canvasRef.current;
    const bounds = canvas?.getBoundingClientRect();
    const sx = bounds && canvas ? bounds.width / Math.max(1, canvas.width) : 1;
    const sy = bounds && canvas ? bounds.height / Math.max(1, canvas.height) : 1;
    const displayRect = { x: cropRect.x * sx, y: cropRect.y * sy, w: cropRect.w * sx, h: cropRect.h * sy };

    return calculateToolbarPlacement(displayRect, toolbarWidth, viewport);
  };

  return (
    <div
      className={`capture-container${isScrollableEditor ? " editor-main" : ""}`}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
    >
      <canvas
        ref={canvasRef}
        width={isScrollableEditor ? (isStitchedResult ? (canvasRef.current?.width ?? window.innerWidth) : (editorImageSize?.width ?? window.innerWidth)) : window.innerWidth}
        height={isScrollableEditor ? (isStitchedResult ? (canvasRef.current?.height ?? window.innerHeight) : (editorImageSize?.height ?? window.innerHeight)) : window.innerHeight}
        className="capture-canvas"
        style={isScrollableEditor ? { width: `${isStitchedResult ? (canvasRef.current?.width ?? window.innerWidth) : (editorImageSize?.width ?? window.innerWidth)}px`, height: `${isStitchedResult ? (canvasRef.current?.height ?? window.innerHeight) : (editorImageSize?.height ?? window.innerHeight)}px` } : undefined}
      />

      {/* Long Screenshot Top Guidance Banner */}
      {mode === "scroll" && !isScrollingMode && !isStitching && (!cropRect || cropRect.w < 80 || cropRect.h < 80) && (
        <div
          style={{
            position: "fixed",
            top: 24,
            left: "50%",
            transform: "translateX(-50%)",
            background: "rgba(18, 18, 24, 0.94)",
            border: "1px solid var(--accent-color)",
            borderRadius: 24,
            padding: "10px 24px",
            color: "#fff",
            fontSize: 13,
            fontWeight: 500,
            zIndex: 10000,
            boxShadow: "0 10px 35px rgba(0,0,0,0.6)",
            pointerEvents: "auto",
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => e.stopPropagation()}
        >
          <span>長截圖：先框選單一可捲動內容區域，再開始擷取；不要包含固定側欄、浮動輸入框或捲軸</span>
          <button
            onClick={(e) => { e.stopPropagation(); void cancelScrollFlow(); }}
            onMouseDown={(e) => e.stopPropagation()}
            style={{ border: "1px solid rgba(255,255,255,.45)", borderRadius: 6, padding: "4px 9px", background: "transparent", color: "#fff", cursor: "pointer", whiteSpace: "nowrap" }}
          >
            取消
          </button>
        </div>
      )}

      {mode === "scroll" && cropRect && !isScrollingMode && !isStitching && (
        <div
          style={{ position: "fixed", top: 24, left: "50%", transform: "translateX(-50%)", zIndex: 10000, display: "flex", alignItems: "center", gap: 10, padding: "10px 14px", borderRadius: 10, background: "rgba(18,18,24,.96)", border: "1px solid var(--accent-color)", color: "#fff", boxShadow: "0 10px 35px rgba(0,0,0,.55)", pointerEvents: "auto" }}
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => e.stopPropagation()}
        >
          <span style={{ fontSize: 12, whiteSpace: "nowrap" }}>已選取內容區域；請確認只包含一個捲動區</span>
          <button className="btn btn-primary" onClick={() => void handleWindowScrollCapture()}>開始長截圖</button>
          <button className="btn" onClick={() => { setCropRect(null); setIsSelecting(false); }}>重選範圍</button>
          <button className="btn" onClick={() => void cancelScrollFlow()}>取消</button>
        </div>
      )}



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
              if (isScrollableEditor) {
                const sy = canvas.getBoundingClientRect().height / Math.max(1, canvas.height);
                return canvas.offsetTop + textInput.y * sy;
              }
              const canvasRect = canvas.getBoundingClientRect();
              const containerRect = container.getBoundingClientRect();
              const sy = canvasRect.height / Math.max(1, canvas.height);
              return canvasRect.top - containerRect.top + textInput.y * sy;
            })(),
            left: (() => {
              const canvas = canvasRef.current;
              const container = canvas?.parentElement;
              if (!canvas || !container) return textInput.x;
              if (isScrollableEditor) {
                const sx = canvas.getBoundingClientRect().width / Math.max(1, canvas.width);
                return canvas.offsetLeft + textInput.x * sx;
              }
              const canvasRect = canvas.getBoundingClientRect();
              const containerRect = container.getBoundingClientRect();
              const sx = canvasRect.width / Math.max(1, canvas.width);
              return canvasRect.left - containerRect.left + textInput.x * sx;
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
        <div
          style={{ position: "fixed", top: 58, right: 24, zIndex: 1_000_000, display: "flex", alignItems: "center", gap: 10, padding: "10px 12px", borderRadius: 10, background: "rgba(25,25,32,.98)", border: "1px solid rgba(99,102,241,.9)", boxShadow: "0 8px 24px rgba(0,0,0,.42)", color: "#fff", fontFamily: "system-ui,sans-serif", pointerEvents: "auto" }}
          onClick={(e) => e.stopPropagation()}
          onMouseDown={(e) => e.stopPropagation()}
          onMouseUp={(e) => e.stopPropagation()}
        >
          <span style={{ fontSize: 12, fontWeight: 600, whiteSpace: "nowrap" }}>錄影控制</span>
          <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, whiteSpace: "nowrap" }}>
            <input type="checkbox" checked={recordAudio} onChange={(e) => setRecordAudio(e.target.checked)} />麥克風
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, whiteSpace: "nowrap" }}>
            <input type="checkbox" checked={recordSystemAudio} onChange={(e) => setRecordSystemAudio(e.target.checked)} />系統聲音
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, whiteSpace: "nowrap" }}>
            FPS
            <select value={recordingFps} onChange={(e) => setRecordingFps(Number(e.target.value))} style={{ fontSize: 12 }}>
              <option value={15}>15</option><option value={24}>24</option><option value={30}>30</option><option value={60}>60</option>
            </select>
          </label>
          <button onClick={() => void startRecordingControl()} disabled={isStartingRecording} style={{ border: 0, borderRadius: 6, padding: "7px 12px", background: "#6366f1", color: "#fff", cursor: isStartingRecording ? "default" : "pointer", fontSize: 13, fontWeight: 600, opacity: isStartingRecording ? .65 : 1 }}>
            {isStartingRecording ? "正在啟動…" : "開始錄影"}
          </button>
          <button
            onClick={() => void closeEditor()}
            disabled={isStartingRecording}
            style={{ border: "1px solid rgba(255,255,255,.35)", borderRadius: 6, padding: "7px 10px", background: "transparent", color: "#fff", cursor: isStartingRecording ? "default" : "pointer", fontSize: 13, opacity: isStartingRecording ? .5 : 1 }}
            title="取消錄影選取"
          >
            取消
          </button>
        </div>
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
                colors={colorsList}
                strokeColor={strokeColor}
                onChooseColor={(color) => { setStrokeColor(color); setShowColorPalette(false); }}
                textFont={textFont}
                fontOptions={fontOptions}
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
        <div className="ocr-result-modal" onMouseDown={(e) => e.stopPropagation()}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
            <h3 style={{ fontSize: 15, fontWeight: 600 }}>OCR 辨識結果 (已複製)</h3>
            <button
              onClick={() => setOcrText(null)}
              style={{ background: "transparent", border: "none", cursor: "pointer", color: "#aaa" }}
            >
              <X size={16} />
            </button>
          </div>
          <textarea
            className="ocr-textarea"
            value={ocrText}
            readOnly
            onClick={(e) => (e.target as HTMLTextAreaElement).select()}
          />
          <div style={{ display: "flex", justifyContent: "flex-end" }}>
            <button className="btn btn-primary" onClick={() => setOcrText(null)}>
              確認
            </button>
          </div>
        </div>
      )}



      {/* Stitching Loading Overlay */}
      {isStitching && (
        <div className="ocr-result-modal" style={{ textAlign: "center" }} onMouseDown={(e) => e.stopPropagation()}>
          <h3 style={{ marginBottom: 10 }}>正在拼接長截圖...</h3>
          <p style={{ color: "var(--text-secondary)", fontSize: 13 }}>請稍候，正在比對重疊像素進行垂直拼接</p>
        </div>
      )}





      {toastMsg && <div className="toast">{toastMsg}</div>}
    </div>
  );
}
