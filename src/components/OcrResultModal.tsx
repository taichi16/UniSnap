import { X } from "lucide-react";

interface OcrResultModalProps {
  text: string;
  onClose: () => void;
}

function enforceReadableOcrText(element: HTMLElement | null) {
  if (!element) return;
  element.style.setProperty("color", "#f8fafc", "important");
  element.style.setProperty("-webkit-text-fill-color", "#f8fafc", "important");
  element.style.setProperty("opacity", "1", "important");
}

export default function OcrResultModal({ text, onClose }: OcrResultModalProps) {
  return (
    <section
      className="ocr-result-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ocr-result-title"
      ref={enforceReadableOcrText}
      onMouseDown={(event) => event.stopPropagation()}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
        <div id="ocr-result-title" ref={enforceReadableOcrText} style={{ fontSize: 15, fontWeight: 700 }}>OCR 辨識結果（已複製）</div>
        <button onClick={onClose} style={{ background: "transparent", border: "none", cursor: "pointer", color: "#aaa" }}>
          <X size={16} />
        </button>
      </div>
      <pre className="ocr-text-output" ref={enforceReadableOcrText} tabIndex={0}>{text}</pre>
      <div style={{ display: "flex", justifyContent: "flex-end" }}>
        <button className="btn btn-primary" onClick={onClose}>確認</button>
      </div>
    </section>
  );
}
