import { ChevronRight } from "lucide-react";
import type { ArrowStyle, Tool } from "../editor/types";

interface FontOption {
  label: string;
  value: string;
}

interface ToolOptionsPopoverProps {
  activeTool: Tool;
  showColorPalette: boolean;
  colors: string[];
  strokeColor: string;
  onChooseColor: (color: string) => void;
  textFont: string;
  fontOptions: FontOption[];
  onTextFontChange: (font: string) => void;
  strokeWidth: number;
  onStrokeWidthChange: (width: number) => void;
  arrowStyle: ArrowStyle;
  onArrowStyleChange: (style: ArrowStyle) => void;
  fillShape: boolean;
  onFillShapeChange: (fill: boolean) => void;
  fillOpacity: number;
  onFillOpacityChange: (opacity: number) => void;
  lineStyle: "solid" | "dashed";
  onLineStyleChange: (style: "solid" | "dashed") => void;
  mosaicIntensity: number;
  onMosaicIntensityChange: (intensity: number) => void;
}

export default function ToolOptionsPopover({
  activeTool,
  showColorPalette,
  colors,
  strokeColor,
  onChooseColor,
  textFont,
  fontOptions,
  onTextFontChange,
  strokeWidth,
  onStrokeWidthChange,
  arrowStyle,
  onArrowStyleChange,
  fillShape,
  onFillShapeChange,
  fillOpacity,
  onFillOpacityChange,
  lineStyle,
  onLineStyleChange,
  mosaicIntensity,
  onMosaicIntensityChange,
}: ToolOptionsPopoverProps) {
  return (
    <>
      {showColorPalette && (
        <div className="color-picker-popover">
          {colors.map((color) => (
            <div
              key={color}
              className={`color-dot ${strokeColor === color ? "selected" : ""}`}
              style={{ backgroundColor: color }}
              onClick={() => onChooseColor(color)}
            />
          ))}
        </div>
      )}

      {!showColorPalette && activeTool === "text" && (
        <div className="sub-toolbar">
          <label>字型:</label>
          <select value={textFont} onChange={(event) => onTextFontChange(event.target.value)} style={{ fontSize: 11, background: "rgba(30,30,40,0.8)", color: "#fff", border: "1px solid rgba(255,255,255,0.2)", borderRadius: 4, padding: "2px 4px", cursor: "pointer" }}>
            {fontOptions.map((font) => <option key={font.value} value={font.value}>{font.label}</option>)}
          </select>
          <label style={{ marginLeft: 8 }}>大小:</label>
          <input type="range" min="1" max="12" value={strokeWidth} style={{ width: 60, accentColor: "var(--accent-color)" }} onChange={(event) => onStrokeWidthChange(parseInt(event.target.value))} />
          <span style={{ fontSize: 11, minWidth: 28 }}>{strokeWidth * 6}px</span>
        </div>
      )}

      {!showColorPalette && activeTool !== "select" && activeTool !== "highlighter" && activeTool !== "text" && (
        <div className="sub-toolbar">
          {activeTool !== "mosaic" && <>
            <label>粗細:</label>
            <input type="range" min="1" max="15" value={strokeWidth} style={{ width: 60, accentColor: "var(--accent-color)" }} onChange={(event) => onStrokeWidthChange(parseInt(event.target.value))} />
          </>}

          {activeTool === "arrow" && (
            <label style={{ display: "flex", alignItems: "center", gap: 4 }}>
              <ChevronRight size={12} />
              <select value={arrowStyle} onChange={(event) => onArrowStyleChange(event.target.value as ArrowStyle)} style={{ fontSize: 11, background: "rgba(30,30,40,0.9)", color: "#fff", border: "1px solid rgba(255,255,255,0.2)", borderRadius: 4, padding: "2px 4px", cursor: "pointer" }}>
                <option value="single">單向箭頭 →</option><option value="double">雙向箭頭 ↔</option><option value="curve">曲線箭頭 ↝</option><option value="elbow">折線箭頭 ↳</option><option value="chevron">人字形 ›</option><option value="block">粗體實心 ▶</option>
              </select>
            </label>
          )}

          {(activeTool === "rect" || activeTool === "circle") && <label style={{ display: "flex", alignItems: "center", gap: 4, cursor: "pointer" }}><input type="checkbox" checked={fillShape} onChange={(event) => onFillShapeChange(event.target.checked)} />實心</label>}
          {(activeTool === "rect" || activeTool === "circle") && fillShape && <label style={{ display: "flex", alignItems: "center", gap: 4 }}>透明度:<input type="range" min="10" max="100" step="5" value={fillOpacity} style={{ width: 55, accentColor: "var(--accent-color)" }} onChange={(event) => onFillOpacityChange(parseInt(event.target.value))} /><span style={{ fontSize: 10, minWidth: 28 }}>{fillOpacity}%</span></label>}
          {(activeTool === "line" || activeTool === "rect" || activeTool === "circle") && <label style={{ display: "flex", alignItems: "center", gap: 4, cursor: "pointer", marginLeft: 4 }}><input type="checkbox" checked={lineStyle === "dashed"} onChange={(event) => onLineStyleChange(event.target.checked ? "dashed" : "solid")} />虛線</label>}
          {activeTool === "mosaic" && <label style={{ display: "flex", alignItems: "center", gap: 4 }}>強度:<input type="range" min="4" max="24" step="2" value={mosaicIntensity} style={{ width: 60, accentColor: "var(--accent-color)" }} onChange={(event) => onMosaicIntensityChange(parseInt(event.target.value))} /></label>}
        </div>
      )}
    </>
  );
}
