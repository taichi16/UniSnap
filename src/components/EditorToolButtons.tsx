import { Circle as CircleIcon, Crop, Grid, Highlighter, Maximize2, Minus, MoveRight, Pencil, Square, Type } from "lucide-react";
import type { ReactNode } from "react";
import type { Tool } from "../editor/types";

interface EditorToolButtonsProps {
  activeTool: Tool;
  onChooseTool: (tool: Tool) => void;
  onExpandCanvas: () => void;
}

export default function EditorToolButtons({ activeTool, onChooseTool, onExpandCanvas }: EditorToolButtonsProps) {
  const button = (tool: Tool, label: string, icon: ReactNode) => (
    <button className={`toolbar-btn ${activeTool === tool ? "active" : ""}`} onClick={() => onChooseTool(tool)} data-tooltip={label}>
      {icon}
    </button>
  );

  return (
    <>
      {button("select", "裁切／框選調整", <Crop size={16} />)}
      <button className="toolbar-btn" onClick={onExpandCanvas} data-tooltip="擴增空白畫布"><Maximize2 size={16} /></button>
      {button("pen", "筆型標記", <Pencil size={16} />)}
      {button("highlighter", "螢光標示", <Highlighter size={16} />)}
      {button("arrow", "指示箭頭", <MoveRight size={16} />)}
      {button("line", "直線/虛線", <Minus size={16} />)}
      {button("rect", "畫矩形", <Square size={16} />)}
      {button("circle", "畫橢圓/圓形", <CircleIcon size={16} />)}
      {button("text", "文字打字", <Type size={16} />)}
      {button("mosaic", "馬賽克", <Grid size={16} />)}
      <div className="toolbar-divider" />
    </>
  );
}
