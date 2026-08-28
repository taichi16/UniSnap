import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import MainWindow from "./components/MainWindow";
import CaptureWindow from "./components/CaptureWindow";
import PinWindow from "./components/PinWindow";
import RecordingControl from "./components/RecordingControl";
import RecordingStartControl from "./components/RecordingStartControl";
import "./App.css";


function getInitialWindowLabel(): string {
  try {
    const win = getCurrentWindow();
    return win?.label ?? "";
  } catch {
    return "";
  }
}

export default function App() {
  const [route, setRoute] = useState(window.location.hash);
  const [windowLabel, setWindowLabel] = useState(getInitialWindowLabel);
  const [isReady, setIsReady] = useState(false);

  useEffect(() => {
    try {
      const win = getCurrentWindow();
      if (win && win.label) {
        setWindowLabel(win.label);
      }
    } catch {
      // Browser preview mode fallback
    }
    setIsReady(true);
  }, []);

  useEffect(() => {
    const handleHashChange = () => {
      setRoute(window.location.hash);
    };
    window.addEventListener("hashchange", handleHashChange);
    return () => window.removeEventListener("hashchange", handleHashChange);
  }, []);

  // Hash Routing parser fallback
  const hash = route.replace("#", "") || "/";
  const [path, queryString] = hash.split("?");
  const params = new URLSearchParams(queryString);

  const hashLabel = params.get("label") || "";
  const hashMode = params.get("mode") || "";

  const activeLabel = hashLabel || windowLabel;

  const isCaptureWindow =
    path !== "/recording-control" &&
    path !== "/recording-start-control" &&
    (activeLabel.startsWith("capture_") ||
      activeLabel.startsWith("editor_") ||
      activeLabel.startsWith("main_editor_") ||
      path === "/capture");

  // For capture windows: immediately set WebView2 background to transparent
  // to prevent white flash before the screenshot canvas is ready.
  useEffect(() => {
    if (!isCaptureWindow) return;
    try {
      const win = getCurrentWindow();
      // setBackgroundColor expects red, green, blue, alpha properties (not r, g, b, a)
      void (win as any).setBackgroundColor({ red: 0, green: 0, blue: 0, alpha: 0 }).catch(() => {});
    } catch {
      // API may not be available on all platforms
    }
  }, [isCaptureWindow]);

  if (!isReady) {
    return <div style={{ backgroundColor: "#000000", width: "100vw", height: "100vh" }} />;
  }

  if (isCaptureWindow) {
    let mode = hashMode;
    if (!mode) {
      if (activeLabel.includes("_record_") || activeLabel.includes("_record")) mode = "record";
      else if (activeLabel.includes("_scroll_") || activeLabel.includes("_scroll")) mode = "scroll";
      else if (activeLabel.startsWith("editor_") || activeLabel.startsWith("main_editor_") || activeLabel.includes("_edit")) mode = "edit";
      else mode = "screenshot";
    }
    return <CaptureWindow label={activeLabel} mode={mode} />;
  }

  if (activeLabel.startsWith("pin_") || path === "/pin") {
    return <PinWindow label={activeLabel} />;
  }

  if (activeLabel.startsWith("recording_control") || path === "/recording-control") {
    return <RecordingControl />;
  }

  if (activeLabel.startsWith("recording_start") || path === "/recording-start-control") {
    return <RecordingStartControl />;
  }

  return <MainWindow />;
}
