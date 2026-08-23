export default function StitchingOverlay() {
  return (
    <div className="ocr-result-modal" style={{ textAlign: "center" }} onMouseDown={(event) => event.stopPropagation()}>
      <h3 style={{ marginBottom: 10 }}>正在拼接長截圖...</h3>
      <p style={{ color: "var(--text-secondary)", fontSize: 13 }}>請稍候，正在比對重疊像素進行垂直拼接</p>
    </div>
  );
}
