import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export default function RecordingStartControl() {
  const params = new URLSearchParams(window.location.hash.split("?")[1] || "");
  const [busy, setBusy] = useState(false);
  const [recordAudio, setRecordAudio] = useState(params.get("recordAudio") === "true");
  const [recordSystemAudio, setRecordSystemAudio] = useState(params.get("recordSystemAudio") === "true");
  const [fps, setFps] = useState(Number(params.get("fps") || 30));
  const start = async () => {
    if (busy) return;
    setBusy(true);
    const win = getCurrentWindow();
    // Remove this control from the desktop before the first captured frame.
    await win.hide();
    await new Promise((resolve) => window.setTimeout(resolve, 120));
    try {
      await invoke("start_recording", {
        monitorIndex: Number(params.get("monitorIndex") || 0),
        x: Number(params.get("x") || 0),
        y: Number(params.get("y") || 0),
        width: Number(params.get("width") || 0),
        height: Number(params.get("height") || 0),
        fps,
        recordAudio,
        recordSystemAudio,
      });
      await emit("recording-started");
    } catch (error) {
      console.error("Failed to start recording:", error);
      await emit("recording-start-failed", String(error));
    } finally {
      await win.close().catch(() => {});
    }
  };
  const cancel = async () => {
    if (busy) return;
    await getCurrentWindow().close().catch(() => {});
  };
  return <div style={{ width: "100vw", height: "100vh", display: "flex", alignItems: "center", justifyContent: "space-between", gap: 10, padding: "8px 12px", boxSizing: "border-box", background: "rgba(25,25,32,.98)", color: "#fff", border: "1px solid rgba(99,102,241,.8)", borderRadius: 10, fontFamily: "system-ui,sans-serif" }}>
    <span style={{ fontSize: 12, whiteSpace: "nowrap" }}>錄影範圍已選取</span>
    <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 11, whiteSpace: "nowrap" }}><input type="checkbox" checked={recordAudio} onChange={(e) => setRecordAudio(e.target.checked)} />麥克風</label>
    <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 11, whiteSpace: "nowrap" }}><input type="checkbox" checked={recordSystemAudio} onChange={(e) => setRecordSystemAudio(e.target.checked)} />系統聲音</label>
    <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 11 }}>FPS<select value={fps} onChange={(e) => setFps(Number(e.target.value))} style={{ fontSize: 11 }}><option value={15}>15</option><option value={24}>24</option><option value={30}>30</option><option value={60}>60</option></select></label>
    <button onClick={() => void start()} disabled={busy} style={{ border: 0, borderRadius: 6, padding: "6px 12px", background: "#6366f1", color: "#fff", cursor: busy ? "default" : "pointer", fontSize: 12 }}>{busy ? "正在啟動…" : "開始錄影"}</button>
    <button onClick={() => void cancel()} disabled={busy} style={{ border: "1px solid rgba(255,255,255,.35)", borderRadius: 6, padding: "6px 10px", background: "transparent", color: "#fff", cursor: busy ? "default" : "pointer", fontSize: 12 }}>取消</button>
  </div>;
}
