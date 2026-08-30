import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, PhysicalSize } from "@tauri-apps/api/window";
import { X } from "lucide-react";

interface PinWindowProps {
  label: string;
}

export default function PinWindow({ label }: PinWindowProps) {
  const [imageBase64, setImageBase64] = useState<string | null>(null);
  const [opacity, setOpacity] = useState(1.0);
  const [showMenu, setShowMenu] = useState(false);
  const [menuPos, setMenuPos] = useState({ x: 0, y: 0 });

  useEffect(() => {
    async function fetchImage() {
      try {
        const image = await invoke<string>("get_pinned_image", { label });
        setImageBase64(image);
      } catch (err) {
        console.error("Failed to load pinned image:", err);
      }
    }
    if (label) {
      fetchImage();
    }
  }, [label]);

  const handleMouseDown = async (e: React.MouseEvent) => {
    // Hide menu on click
    setShowMenu(false);
    
    // Only drag on left click
    if (e.button === 0) {
      try {
        await getCurrentWindow().startDragging();
      } catch (err) {
        console.error("Failed to start dragging:", err);
      }
    }
  };

  const handleDoubleClick = async () => {
    try {
      await invoke("unpin_screenshot", { label });
    } catch (err) {
      console.error("Failed to close pinned window:", err);
    }
  };

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setMenuPos({ x: e.clientX, y: e.clientY });
    setShowMenu(true);
  };

  const handleOpacityChange = (val: number) => {
    setOpacity(val);
  };

  const handleWheel = async (e: React.WheelEvent) => {
    try {
      const windowInstance = getCurrentWindow();
      const size = await windowInstance.innerSize();
      const factor = e.deltaY < 0 ? 1.05 : 0.95;
      
      const newWidth = Math.max(100, Math.round(size.width * factor));
      const newHeight = Math.max(100, Math.round(size.height * factor));
      
      await windowInstance.setSize(new PhysicalSize(newWidth, newHeight));
    } catch (err) {
      console.error("Failed to set size on wheel scroll:", err);
    }
  };

  if (!imageBase64) {
    return <div style={{ color: "#fff", padding: 10 }}>載入中...</div>;
  }

  return (
    <div
      className="pin-container"
      onMouseDown={handleMouseDown}
      onDoubleClick={handleDoubleClick}
      onContextMenu={handleContextMenu}
      onWheel={handleWheel}
      style={{ opacity }}
    >
      <img
        src={imageBase64}
        alt="Pinned Screen"
        className="pin-image"
        draggable={false}
      />

      {/* Floating Instruction Overlay (shows briefly on hover) */}
      <div
        style={{
          position: "absolute",
          top: 8,
          left: 8,
          background: "rgba(0,0,0,0.6)",
          padding: "4px 8px",
          borderRadius: 4,
          fontSize: 10,
          color: "#aaa",
          pointerEvents: "none",
          display: "flex",
          gap: 6,
        }}
      >
        <span>雙擊關閉</span>
        <span>•</span>
        <span>滾輪縮放</span>
        <span>•</span>
        <span>右鍵選單</span>
      </div>

      {/* Right Click Context Menu */}
      {showMenu && (
        <div
          style={{
            position: "absolute",
            top: menuPos.y,
            left: menuPos.x,
            background: "rgba(20,20,25,0.95)",
            border: "1px solid rgba(255,255,255,0.1)",
            padding: "8px 12px",
            borderRadius: 8,
            boxShadow: "0 4px 12px rgba(0,0,0,0.5)",
            display: "flex",
            flexDirection: "column",
            gap: 8,
            zIndex: 99999,
          }}
          onMouseDown={(e) => e.stopPropagation()} // Prevent dragging when interacting with menu
        >
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: 11, color: "#aaa" }}>不透明度 ({Math.round(opacity * 100)}%)</span>
            <input
              type="range"
              min="0.1"
              max="1.0"
              step="0.05"
              value={opacity}
              style={{ width: 120, accentColor: "var(--accent-color)" }}
              onChange={(e) => handleOpacityChange(parseFloat(e.target.value))}
            />
          </div>
          
          <div style={{ height: 1, background: "rgba(255,255,255,0.08)" }}></div>
          
          <button
            onClick={handleDoubleClick}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              background: "transparent",
              border: "none",
              color: "#ef4444",
              fontSize: 12,
              cursor: "pointer",
              padding: "4px 0",
              textAlign: "left",
            }}
          >
            <X size={14} />
            關閉貼圖
          </button>
        </div>
      )}
    </div>
  );
}
