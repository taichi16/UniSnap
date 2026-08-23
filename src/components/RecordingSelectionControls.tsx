interface RecordingSelectionControlsProps {
  recordAudio: boolean;
  onRecordAudioChange: (enabled: boolean) => void;
  recordSystemAudio: boolean;
  onRecordSystemAudioChange: (enabled: boolean) => void;
  fps: number;
  onFpsChange: (fps: number) => void;
  isStarting: boolean;
  onStart: () => void;
  onCancel: () => void;
}

export default function RecordingSelectionControls({
  recordAudio,
  onRecordAudioChange,
  recordSystemAudio,
  onRecordSystemAudioChange,
  fps,
  onFpsChange,
  isStarting,
  onStart,
  onCancel,
}: RecordingSelectionControlsProps) {
  return (
    <div
      style={{ position: "fixed", top: 58, right: 24, zIndex: 1_000_000, display: "flex", alignItems: "center", gap: 10, padding: "10px 12px", borderRadius: 10, background: "rgba(25,25,32,.98)", border: "1px solid rgba(99,102,241,.9)", boxShadow: "0 8px 24px rgba(0,0,0,.42)", color: "#fff", fontFamily: "system-ui,sans-serif", pointerEvents: "auto" }}
      onClick={(event) => event.stopPropagation()}
      onMouseDown={(event) => event.stopPropagation()}
      onMouseUp={(event) => event.stopPropagation()}
    >
      <span style={{ fontSize: 12, fontWeight: 600, whiteSpace: "nowrap" }}>錄影控制</span>
      <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, whiteSpace: "nowrap" }}>
        <input type="checkbox" checked={recordAudio} onChange={(event) => onRecordAudioChange(event.target.checked)} />麥克風
      </label>
      <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, whiteSpace: "nowrap" }}>
        <input type="checkbox" checked={recordSystemAudio} onChange={(event) => onRecordSystemAudioChange(event.target.checked)} />系統聲音
      </label>
      <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, whiteSpace: "nowrap" }}>
        FPS
        <select value={fps} onChange={(event) => onFpsChange(Number(event.target.value))} style={{ fontSize: 12 }}>
          <option value={15}>15</option><option value={24}>24</option><option value={30}>30</option><option value={60}>60</option>
        </select>
      </label>
      <button onClick={onStart} disabled={isStarting} style={{ border: 0, borderRadius: 6, padding: "7px 12px", background: "#6366f1", color: "#fff", cursor: isStarting ? "default" : "pointer", fontSize: 13, fontWeight: 600, opacity: isStarting ? .65 : 1 }}>
        {isStarting ? "正在啟動…" : "開始錄影"}
      </button>
      <button onClick={onCancel} disabled={isStarting} style={{ border: "1px solid rgba(255,255,255,.35)", borderRadius: 6, padding: "7px 10px", background: "transparent", color: "#fff", cursor: isStarting ? "default" : "pointer", fontSize: 13, opacity: isStarting ? .5 : 1 }} title="取消錄影選取">
        取消
      </button>
    </div>
  );
}
