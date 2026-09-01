import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleStop } from "lucide-react";

type RecordingResult = { path: string; frameCount: number; width: number; height: number };
type RecordingBackendStatus = {
  phase: "idle" | "recording" | "finalizing" | "completed" | "failed";
  result: RecordingResult | null;
  error: string | null;
};

export default function RecordingControl() {
  const startedAtRef = useRef<number | null>(Date.now());
  const stoppingRef = useRef(false);
  const [seconds, setSeconds] = useState(0);
  const [status, setStatus] = useState<"recording" | "saving" | "saved" | "error">("recording");
  const [message, setMessage] = useState("");

  const applyBackendStatus = useCallback((backendStatus: RecordingBackendStatus) => {
    if (backendStatus.phase === "completed" && backendStatus.result) {
      setStatus("saved");
      setMessage(`已儲存：${backendStatus.result.path}`);
      window.setTimeout(() => void invoke("close_capture_windows"), 1400);
      return true;
    }
    if (backendStatus.phase === "failed") {
      setStatus("error");
      setMessage(`存檔失敗：${backendStatus.error ?? "未知錯誤"}`);
      return true;
    }
    if (backendStatus.phase === "recording") {
      setStatus("recording");
    } else if (backendStatus.phase === "finalizing") {
      setStatus("saving");
    }
    return false;
  }, []);

  const stopRecording = useCallback(async () => {
    if (stoppingRef.current || startedAtRef.current === null) return;
    stoppingRef.current = true;
    setStatus("saving");
    try {
      const backendStatus = await invoke<RecordingBackendStatus>("stop_recording");
      applyBackendStatus(backendStatus);
    } catch (error) {
      setStatus("error");
      setMessage(`存檔失敗：${String(error)}`);
    }
  }, [applyBackendStatus]);

  useEffect(() => {
    const clock = window.setInterval(() => {
      if (startedAtRef.current !== null && !stoppingRef.current) setSeconds(Math.floor((Date.now() - startedAtRef.current) / 1000));
    }, 250);
    return () => { window.clearInterval(clock); };
  }, [stopRecording]);

  useEffect(() => {
    if (status !== "recording" && status !== "saving") return;
    let disposed = false;
    let polling = false;
    const poll = async () => {
      if (disposed || polling) return;
      polling = true;
      try {
        const backendStatus = await invoke<RecordingBackendStatus>("get_recording_status");
        if (!disposed) applyBackendStatus(backendStatus);
      } catch (error) {
        if (!disposed) {
          setStatus("error");
          setMessage(`無法取得存檔進度：${String(error)}`);
        }
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = window.setInterval(() => void poll(), 500);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [status, applyBackendStatus]);

  const elapsed = new Date(seconds * 1000).toISOString().substring(14, 19);
  const label = status === "saving" ? "正在完成 MP4 存檔…" : status === "saved" || status === "error" ? message : `錄影中 ${elapsed}`;
  return <div style={{ width: "100vw", height: "100vh", display: "flex", alignItems: "center", justifyContent: "space-between", gap: 10, padding: "8px 12px", boxSizing: "border-box", background: "rgba(25,25,32,.96)", color: "#fff", fontFamily: "system-ui,sans-serif", border: `1px solid ${status === "error" ? "#f59e0b" : "rgba(239,68,68,.75)"}`, borderRadius: 10 }}>
    <span title={label} style={{ display: "flex", alignItems: "center", gap: 7, fontSize: 12, minWidth: 0, overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" }}><span style={{ flex: "0 0 auto", width: 8, height: 8, borderRadius: "50%", background: status === "error" ? "#f59e0b" : "#ef4444" }} />{label}</span>
    <button onClick={() => void stopRecording()} disabled={status !== "recording"} title="停止錄影並存成 MP4" style={{ display: "flex", flex: "0 0 auto", alignItems: "center", gap: 5, border: "none", borderRadius: 6, padding: "5px 8px", background: "#ef4444", color: "#fff", cursor: status === "recording" ? "pointer" : "default", fontSize: 12, opacity: status === "recording" ? 1 : .6 }}><CircleStop size={16} />停止並存檔</button>
  </div>;
}
