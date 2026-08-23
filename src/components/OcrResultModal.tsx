import { X } from "lucide-react";

interface OcrResultModalProps {
  text: string;
  onClose: () => void;
}

export default function OcrResultModal({ text, onClose }: OcrResultModalProps) {
  return (
    <div className="ocr-result-modal" onMouseDown={(event) => event.stopPropagation()}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
        <h3 style={{ fontSize: 15, fontWeight: 600 }}>OCR 辨識結果 (已複製)</h3>
        <button onClick={onClose} style={{ background: "transparent", border: "none", cursor: "pointer", color: "#aaa" }}>
          <X size={16} />
        </button>
      </div>
      <textarea className="ocr-textarea" value={text} readOnly onClick={(event) => event.currentTarget.select()} />
      <div style={{ display: "flex", justifyContent: "flex-end" }}>
        <button className="btn btn-primary" onClick={onClose}>確認</button>
      </div>
    </div>
  );
}
