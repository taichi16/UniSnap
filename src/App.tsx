import { useState, useEffect } from "react";
import MainWindow from "./components/MainWindow";
import CaptureWindow from "./components/CaptureWindow";
import PinWindow from "./components/PinWindow";
import RecordingControl from "./components/RecordingControl";
import RecordingStartControl from "./components/RecordingStartControl";
import QuickAccessWindow from "./components/QuickAccessWindow";
import IdentifyMonitorWindow from "./components/IdentifyMonitorWindow";
import "./App.css";

export default function App() {
  const [route, setRoute] = useState(window.location.hash);

  // Hash Routing
  useEffect(() => {
    const handleHashChange = () => {
      setRoute(window.location.hash);
    };
    window.addEventListener("hashchange", handleHashChange);
    return () => window.removeEventListener("hashchange", handleHashChange);
  }, []);

  // Simple Hash Router parser
  // format: #/path?param=value
  const hash = route.replace("#", "") || "/";
  const [path, queryString] = hash.split("?");
  const params = new URLSearchParams(queryString);

  if (path === "/capture") {
    const label = params.get("label") || "";
    const mode = params.get("mode") || "screenshot";
    const monitorIndexStr = params.get("monitorIndex");
    const monitorIndex = monitorIndexStr !== null ? parseInt(monitorIndexStr, 10) : undefined;
    return <CaptureWindow label={label} mode={mode} monitorIndex={monitorIndex} />;
  }

  if (path === "/identify-monitor") {
    const index = parseInt(params.get("index") || "0", 10);
    const number = parseInt(params.get("number") || "1", 10);
    const width = parseInt(params.get("w") || "0", 10);
    const height = parseInt(params.get("h") || "0", 10);
    return <IdentifyMonitorWindow index={index} number={number} width={width} height={height} />;
  }

  if (path === "/pin") {
    const label = params.get("label") || "";
    return <PinWindow label={label} />;
  }

  if (path === "/recording-control") {
    return <RecordingControl />;
  }

  if (path === "/recording-start-control") {
    return <RecordingStartControl />;
  }

  if (path === "/quick-access") {
    return <QuickAccessWindow label={params.get("label") || ""} />;
  }

  return <MainWindow />;
}
