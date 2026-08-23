import { Copy, Check, Download, Languages, Pin, RotateCcw, X } from "lucide-react";

interface EditorActionsProps {
  canUndo: boolean;
  onUndo: () => void;
  onPin: () => void;
  onOCR: () => void;
  ocrLoading: boolean;
  onCopy: () => void;
  saveFormat: "png" | "jpg";
  onSaveFormatChange: (format: "png" | "jpg") => void;
  onSave: () => void;
  onCancel: () => void;
  onConfirm: () => void;
}

export default function EditorActions({
  canUndo,
  onUndo,
  onPin,
  onOCR,
  ocrLoading,
  onCopy,
  saveFormat,
  onSaveFormatChange,
  onSave,
  onCancel,
  onConfirm,
}: EditorActionsProps) {
  return (
    <>
      <button className="toolbar-btn" onClick={onUndo} disabled={!canUndo} data-tooltip="復原">
        <RotateCcw size={16} />
      </button>
      <div className="toolbar-divider" />
      <button className="toolbar-btn" onClick={onPin} data-tooltip="置頂貼圖"><Pin size={16} color="#3b82f6" /></button>
      <button className="toolbar-btn" onClick={onOCR} disabled={ocrLoading} data-tooltip="文字辨識 (OCR)"><Languages size={16} color="#10b981" /></button>
      <button className="toolbar-btn" onClick={onCopy} data-tooltip="複製到剪貼簿 (Cmd+C)"><Copy size={16} color="#38bdf8" /></button>
      <select value={saveFormat} onChange={(event) => onSaveFormatChange(event.target.value as "png" | "jpg")} title="選擇圖片格式" aria-label="選擇圖片格式" style={{ height: 28, fontSize: 11, borderRadius: 5, padding: "0 4px", background: "var(--panel-bg)", color: "var(--text-primary)", border: "1px solid var(--panel-border)" }}>
        <option value="png">PNG</option>
        <option value="jpg">JPG</option>
      </select>
      <button className="toolbar-btn" onClick={onSave} data-tooltip="另存新檔..."><Download size={16} /></button>
      <button className="toolbar-btn" onClick={onCancel} data-tooltip="取消 (Esc)"><X size={16} color="#ef4444" /></button>
      <button className="toolbar-btn" onClick={onConfirm} data-tooltip="完成存檔並關閉"><Check size={16} color="#10b981" /></button>
    </>
  );
}
