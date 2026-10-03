import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Monitor } from "lucide-react";

interface IdentifyMonitorProps {
  index: number;
  number: number;
  width: number;
  height: number;
}

export default function IdentifyMonitorWindow({
  number,
  width,
  height,
}: IdentifyMonitorProps) {
  const [fading, setFading] = useState(false);

  useEffect(() => {
    // Set document body to transparent
    document.body.style.backgroundColor = "transparent";
    document.documentElement.style.backgroundColor = "transparent";

    const fadeTimer = setTimeout(() => {
      setFading(true);
    }, 1400);

    const closeTimer = setTimeout(() => {
      void getCurrentWindow().destroy();
    }, 1750);

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        void getCurrentWindow().destroy();
      }
    };
    window.addEventListener("keydown", handleKeyDown);

    return () => {
      clearTimeout(fadeTimer);
      clearTimeout(closeTimer);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);

  const handleClick = () => {
    void getCurrentWindow().destroy();
  };

  return (
    <div
      className={`identify-monitor-container ${fading ? "fading-out" : ""}`}
      onClick={handleClick}
      title="點擊或按 Esc 關閉提示"
    >
      <div className="identify-monitor-card">
        <div className="identify-monitor-number">{number}</div>
        <div className="identify-monitor-info">
          <div className="identify-monitor-title">
            <Monitor size={14} style={{ verticalAlign: "middle", marginRight: 5 }} />
            螢幕 {number}
          </div>
          {width > 0 && height > 0 && (
            <div className="identify-monitor-res">
              {width} × {height}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
