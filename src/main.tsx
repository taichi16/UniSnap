import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";

// Setup log forwarder to Rust backend
try {
  const originalLog = console.log;
  const originalWarn = console.warn;
  const originalError = console.error;

  const forward = (level: string, args: any[]) => {
    const message = args.map(arg => typeof arg === "object" ? JSON.stringify(arg) : String(arg)).join(" ");
    invoke("log_from_frontend", { level, message }).catch(() => {});
  };

  console.log = (...args) => {
    originalLog.apply(console, args);
    forward("INFO", args);
  };
  console.warn = (...args) => {
    originalWarn.apply(console, args);
    forward("WARN", args);
  };
  console.error = (...args) => {
    originalError.apply(console, args);
    forward("ERROR", args);
  };

  window.addEventListener("error", (e) => {
    const errorMsg = e.error ? `${e.error.message}\n${e.error.stack}` : e.message;
    forward("FATAL", [`Unhandled exception: ${errorMsg} at ${e.filename}:${e.lineno}:${e.colno}`]);
  });

  window.addEventListener("unhandledrejection", (e) => {
    const reason = e.reason instanceof Error ? `${e.reason.message}\n${e.reason.stack}` : String(e.reason);
    forward("FATAL", [`Unhandled promise rejection: ${reason}`]);
  });
} catch (e) {
  // Safe fallback if invoke/Tauri is not available
}

import App from "./App";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
