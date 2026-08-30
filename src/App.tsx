import { useState, useEffect } from "react";
import MainWindow from "./components/MainWindow";
import CaptureWindow from "./components/CaptureWindow";
import PinWindow from "./components/PinWindow";
import RecordingControl from "./components/RecordingControl";
import RecordingStartControl from "./components/RecordingStartControl";
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
    return <CaptureWindow label={label} mode={mode} />;
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

  return <MainWindow />;
}
