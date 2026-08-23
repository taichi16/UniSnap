interface ExpandValues {
  top: string;
  right: string;
  bottom: string;
  left: string;
}

interface ExpandCanvasDialogProps {
  values: ExpandValues;
  onChange: (side: keyof ExpandValues, value: string) => void;
  onCancel: () => void;
  onApply: () => void;
}

const sideLabels: Record<keyof ExpandValues, string> = {
  top: "上",
  right: "右",
  bottom: "下",
  left: "左",
};

export default function ExpandCanvasDialog({ values, onChange, onCancel, onApply }: ExpandCanvasDialogProps) {
  return (
    <div
      style={{ position: "fixed", inset: 0, zIndex: 1_000_010, display: "flex", alignItems: "center", justifyContent: "center", background: "rgba(0,0,0,.35)", pointerEvents: "auto" }}
      onMouseDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
    >
      <div style={{ width: 300, padding: 18, borderRadius: 12, background: "var(--panel-bg)", border: "1px solid var(--panel-border)", boxShadow: "0 16px 40px rgba(0,0,0,.45)", color: "var(--text-primary)", fontFamily: "system-ui,sans-serif" }}>
        <h3 style={{ marginBottom: 6, fontSize: 15 }}>擴增空白畫布</h3>
        <p style={{ marginBottom: 12, fontSize: 12, color: "var(--text-secondary)" }}>請輸入各方向要增加的空白像素。</p>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
          {(Object.keys(sideLabels) as Array<keyof ExpandValues>).map((side) => (
            <label key={side} style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 12 }}>
              {sideLabels[side]}
              <input type="number" min="0" step="1" value={values[side]} onChange={(event) => onChange(side, event.target.value)} style={{ width: "100%", padding: "6px 8px" }} />
            </label>
          ))}
        </div>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 16 }}>
          <button className="btn" onClick={onCancel}>取消</button>
          <button className="btn btn-primary" onClick={onApply}>套用</button>
        </div>
      </div>
    </div>
  );
}
