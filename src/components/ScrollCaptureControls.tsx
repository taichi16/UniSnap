interface ScrollCaptureControlsProps {
  showGuidance: boolean;
  hasSelection: boolean;
  isComplete: boolean;
  onStart: () => void;
  onReset: () => void;
  onCancel: () => void;
}

export default function ScrollCaptureControls({ showGuidance, hasSelection, isComplete, onStart, onReset, onCancel }: ScrollCaptureControlsProps) {
  const stopCapturePointerEvent = (event: React.SyntheticEvent) => event.stopPropagation();
  const interactiveProps = {
    "data-capture-interactive": "true",
    onMouseDown: stopCapturePointerEvent,
    onMouseMove: stopCapturePointerEvent,
    onMouseUp: stopCapturePointerEvent,
    onPointerDown: stopCapturePointerEvent,
    onPointerMove: stopCapturePointerEvent,
    onPointerUp: stopCapturePointerEvent,
    onClick: stopCapturePointerEvent,
  } as const;
  // Once the stitched image is open, the regular editor toolbar owns the
  // screen. Keeping the selection toolbar mounted underneath it was confusing
  // and made the two controls overlap.
  if (isComplete) return null;
  if (showGuidance) {
    return (
      <div {...interactiveProps} style={{ position: "fixed", top: 24, left: "50%", transform: "translateX(-50%)", background: "rgba(18, 18, 24, 0.94)", border: "1px solid var(--accent-color)", borderRadius: 24, padding: "10px 24px", color: "#fff", fontSize: 13, fontWeight: 500, zIndex: 10000, boxShadow: "0 10px 35px rgba(0,0,0,0.6)", pointerEvents: "auto", display: "flex", alignItems: "center", gap: 8 }}>
        <span>長截圖：先框選單一可捲動內容區域，再開始擷取；不要包含固定側欄、浮動輸入框或捲軸</span>
        <button onClick={(event) => { event.stopPropagation(); onCancel(); }} onMouseDown={(event) => event.stopPropagation()} style={{ border: "1px solid rgba(255,255,255,.45)", borderRadius: 6, padding: "4px 9px", background: "transparent", color: "#fff", cursor: "pointer", whiteSpace: "nowrap" }}>取消</button>
      </div>
    );
  }

  if (!hasSelection) return null;
  return (
    <div {...interactiveProps} style={{ position: "fixed", top: 24, left: "50%", transform: "translateX(-50%)", zIndex: 10000, display: "flex", alignItems: "center", gap: 10, padding: "10px 14px", borderRadius: 10, background: "rgba(18,18,24,.96)", border: "1px solid var(--accent-color)", color: "#fff", boxShadow: "0 10px 35px rgba(0,0,0,.55)", pointerEvents: "auto" }}>
      <span style={{ fontSize: 12, whiteSpace: "nowrap" }}>已選取內容區域；請確認只包含一個捲動區</span>
      <button className="btn btn-primary" onClick={onStart}>開始長截圖</button>
      <button className="btn" onClick={onReset}>重選範圍</button>
      <button className="btn" onClick={onCancel}>取消</button>
    </div>
  );
}
