import { useState, useEffect, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { emit, listen } from "@tauri-apps/api/event";
import { availableMonitors, getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Folder, Video, Keyboard, Settings, Scissors, Maximize, AppWindow, ScrollText, Monitor, Sun, Moon, CircleHelp, BarChart3, Image as ImageIcon, Eye } from "lucide-react";
import InfoDialog from "./InfoDialog";

interface AppConfig {
  shortcut_screenshot: String;
  shortcut_recording: String;
  save_directory: String;
  remember_save_directory: boolean;
  jpg_quality: number;
  auto_copy_to_clipboard: boolean;
  theme: string;
}

interface MonitorInfo {
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  scale_factor: number;
}

function shortcutFromKeyEvent(event: ReactKeyboardEvent<HTMLInputElement>): string | null {
  // Escape remains reserved for leaving/cancelling capture flows and is not
  // accepted as a configurable global shortcut.
  if (event.key === "Escape" || ["Control", "Alt", "Shift", "Meta", "OS"].includes(event.key)) {
    return null;
  }

  const parts: string[] = [];
  const isMac = navigator.platform.toLowerCase().includes("mac");
  if (isMac ? event.metaKey : event.ctrlKey) parts.push(isMac ? "Command" : "Control");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");

  let key = event.key;
  if (key.length === 1) key = key.toUpperCase();
  if (key === " ") key = "Space";
  if (key === "ArrowUp" || key === "ArrowDown" || key === "ArrowLeft" || key === "ArrowRight") {
    // These names are accepted by global-hotkey and are clearer in settings.
    key = key;
  }
  if (!parts.length || !key) return null;
  return [...parts, key].join("+");
}

export default function MainWindow() {
  const [config, setConfig] = useState<AppConfig>({
    shortcut_screenshot: "Alt+A",
    shortcut_recording: "Alt+R",
    save_directory: "",
    remember_save_directory: true,
    jpg_quality: 90,
    auto_copy_to_clipboard: true,
    theme: "dark",
  });

  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
  const [showAbout, setShowAbout] = useState(false);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [selectedMonitor, setSelectedMonitor] = useState(0);
  const [monitorError, setMonitorError] = useState<string | null>(null);
  const [screenPermissionError, setScreenPermissionError] = useState(false);

  const fetchMonitors = async () => {
    try {
      let infos: MonitorInfo[] = [];
      try {
        infos = await invoke<MonitorInfo[]>("list_monitors");
      } catch (backendError) {
        console.warn("Backend monitor listing failed, using native window API:", backendError);
      }
      if (!infos.length) {
        const nativeMonitors = await availableMonitors();
        infos = nativeMonitors.map((monitor) => ({
          name: monitor.name ?? "未命名螢幕",
          x: monitor.position.x,
          y: monitor.position.y,
          width: monitor.size.width,
          height: monitor.size.height,
          scale_factor: monitor.scaleFactor,
        }));
      }
      if (!infos.length) throw new Error("系統未回傳任何螢幕");
      setMonitors(infos);
      setSelectedMonitor((current) => Math.min(current, infos.length - 1));
      setMonitorError(null);
    } catch (err) {
      console.error("Failed to fetch monitors:", err);
      setMonitors([]);
      setMonitorError("無法讀取螢幕");
    }
  };

  // Load config and monitors on mount & on focus
  useEffect(() => {
    async function fetchConfig() {
      try {
        const loadedConfig = await invoke<AppConfig>("load_config");
        setConfig(loadedConfig);
      } catch (err) {
        console.error("Failed to load config:", err);
      }
    }
    fetchConfig();
    fetchMonitors();

    window.addEventListener("focus", fetchMonitors);
    return () => window.removeEventListener("focus", fetchMonitors);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<string>("main-editor-error", (event) => {
      showToast(`開啟圖片失敗：${event.payload}`);
    }).then((cleanup) => { unlisten = cleanup; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = config.theme || "dark";
  }, [config.theme]);


  // Fix 6: Use static import instead of dynamic import to avoid CSP issues
  // Resize window dynamically based on expanded state
  useEffect(() => {
    async function resizeWindow() {
      try {
        const currentWin = getCurrentWindow();
        if (showSettings) {
          const screens = await availableMonitors();
          const screen = screens[0];
          const availableHeight = screen
            ? Math.max(560, Math.floor(screen.size.height / screen.scaleFactor - 56))
            : 720;
          await currentWin.setSize(new LogicalSize(760, availableHeight));
        } else {
          await currentWin.setSize(new LogicalSize(760, 112));
        }
      } catch (err) {
        console.error("Failed to resize window:", err);
      }
    }
    resizeWindow();
  }, [showSettings]);

  const saveSettings = async (updatedConfig: AppConfig) => {
    try {
      await invoke("save_config", { config: updatedConfig });
      setConfig(updatedConfig);
      await emit("config-updated");
      showToast("設定已儲存！");
    } catch (err) {
      console.error("Failed to save config:", err);
      showToast("儲存設定失敗");
    }
  };

  const toggleTheme = async () => {
    const updated = { ...config, theme: config.theme === "light" ? "dark" : "light" };
    try {
      await invoke("save_config", { config: updated });
      setConfig(updated);
      await emit("config-updated");
    } catch (err) {
      console.error("Failed to save theme:", err);
    }
  };

  const showToast = (msg: string) => {
    setStatusMessage(msg);
    setTimeout(() => setStatusMessage(null), 2500);
  };

  const handleBrowseFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: config.save_directory as string || undefined,
      });
      if (selected && typeof selected === "string") {
        const updated = { ...config, save_directory: selected };
        saveSettings(updated);
      }
    } catch (err) {
      console.error("Failed to open dialog:", err);
    }
  };

  // Hide frontend window first before invoking backend
  const hideAndInvoke = async (command: string, args?: Record<string, any>) => {
    setScreenPermissionError(false);
    try {
      const currentWin = getCurrentWindow();
      await currentWin.hide();
      await invoke(command, args);
    } catch (err: any) {
      const currentWin = getCurrentWindow();
      await currentWin.show();
      await currentWin.setFocus();
      console.error(`Failed to invoke ${command}:`, err);
      const msg = String(err);
      if (msg.includes("permission") || msg.includes("denied") || msg.includes("access")) {
        setScreenPermissionError(true);
        showToast("⚠️ 需要螢幕錄製權限！請至系統設定 > 隱私權 > 螢幕錄製 開啟授權");
      } else {
        showToast(`操作失敗：${msg.slice(0, 60)}`);
      }
    }
  };

  const getTargetMonitorIndex = () => selectedMonitor;

  const startScreenshot = () => hideAndInvoke("trigger_screenshot", { mode: "screenshot", monitorIndex: getTargetMonitorIndex() });
  const startRecording = () => hideAndInvoke("trigger_screenshot", { mode: "record", monitorIndex: getTargetMonitorIndex() });
  const startScrollingScreenshot = () => hideAndInvoke("trigger_screenshot", { mode: "scroll", monitorIndex: getTargetMonitorIndex() });

  const openExistingImage = async () => {
    try {
      await invoke("open_image_in_main_editor");
    } catch (err) {
      console.error("Failed to open image:", err);
      showToast(`開啟圖片失敗：${String(err).slice(0, 80)}`);
    }
  };

  const startFullScreenScreenshot = async () => {
    try {
      const currentWin = getCurrentWindow();
      await currentWin.hide();
      showToast("正在擷取全螢幕...");
      const path = await invoke<string>("capture_full_screen", { monitorIndex: selectedMonitor });
      const filename = (path as string).split('/').pop()?.split('\\').pop() || "";
      await currentWin.show();
      await currentWin.setFocus();
      showToast(`已儲存：${filename}`);
    } catch (err) {
      const currentWin = getCurrentWindow();
      await currentWin.show();
      showToast("全螢幕截圖失敗");
    }
  };

  const startWorkAreaScreenshot = async () => {
    try {
      const currentWin = getCurrentWindow();
      await currentWin.hide();
      showToast("正在擷取工作區...");
      const path = await invoke<string>("capture_work_area", { monitorIndex: selectedMonitor });
      const filename = (path as string).split('/').pop()?.split('\\').pop() || "";
      await currentWin.show();
      await currentWin.setFocus();
      showToast(`已儲存：${filename}`);
    } catch (err) {
      const currentWin = getCurrentWindow();
      await currentWin.show();
      showToast("工作區截圖失敗");
    }
  };

  return (
    <div className={`main-container ${showSettings ? "expanded" : "compact"}`}>
      {/* Compact horizontal toolbar */}
      <div className="compact-toolbar">
        <div className="app-brand" aria-label="UniSnap">
          <span>UniSnap</span>
          <button
            className="theme-toggle"
            onClick={() => void toggleTheme()}
            title={config.theme === "light" ? "切換夜間模式" : "切換日間模式"}
            aria-label={config.theme === "light" ? "切換夜間模式" : "切換日間模式"}
          >
            {config.theme === "light" ? <Moon size={15} /> : <Sun size={15} />}
          </button>
        </div>
        <button className="toolbar-item-btn open-image-btn" onClick={openExistingImage} title="開啟舊檔進行編輯">
          <ImageIcon size={18} color="#0ea5e9" />
          <span>開啟舊檔</span>
        </button>
        <button className="toolbar-item-btn" onClick={startScreenshot} title="拖曳矩形截圖（按 Esc 取消）">
          <Scissors size={18} color="#6366f1" />
          <span>矩形截圖</span>
        </button>
        <button className="toolbar-item-btn" onClick={startFullScreenScreenshot} title="直接儲存全螢幕">
          <Maximize size={18} color="#10b981" />
          <span>全螢幕</span>
        </button>
        <button className="toolbar-item-btn" onClick={startWorkAreaScreenshot} title="全螢幕（不含 Dock / 工作列）">
          <AppWindow size={18} color="#f59e0b" />
          <span>工作區</span>
        </button>
        <button className="toolbar-item-btn" onClick={startRecording} title="選取區域後錄影">
          <Video size={18} color="#ef4444" />
          <span>螢幕錄影</span>
        </button>
        <button className="toolbar-item-btn" onClick={startScrollingScreenshot} title="長截圖：選取含捲軸的視窗（瀏覽器、Word 等），自動捲動擷取完整頁面">
          <ScrollText size={18} color="#8b5cf6" />
          <span>長截圖</span>
        </button>
        {/* Every capture mode targets the monitor explicitly selected here. */}
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 3, flex: "0 0 160px", minWidth: 160 }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", width: "100%", padding: "0 2px" }}>
            <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
              <Monitor size={14} color="#a5b4fc" />
              <span style={{ fontSize: 11, color: "var(--text-secondary)", fontWeight: 500 }}>目標螢幕</span>
            </div>
            <button
              type="button"
              className="identify-monitor-btn"
              onClick={() => void invoke("identify_monitors", { monitorIndex: null })}
              title="在所有螢幕中央顯示代號（識別實體螢幕）"
            >
              <Eye size={11} style={{ marginRight: 3 }} />
              識別
            </button>
          </div>
          <select
            value={monitors.length ? selectedMonitor : ""}
            onChange={(e) => {
              const idx = parseInt(e.target.value, 10);
              setSelectedMonitor(idx);
              void invoke("identify_monitors", { monitorIndex: idx });
            }}
            disabled={!monitors.length}
            style={{
              fontSize: 11,
              background: "var(--panel-bg)",
              color: "var(--text-primary)",
              border: "1px solid var(--panel-border)",
              borderRadius: 5,
              padding: "3px 6px",
              cursor: "pointer",
              width: "100%",
              textAlign: "left",
            }}
            title="選擇要截圖的螢幕（切換時會顯示螢幕代號）"
          >
            {!monitors.length && <option value="">{monitorError ?? "正在讀取螢幕…"}</option>}
            {monitors.map((m, i) => (
              <option key={i} value={i}>
                螢幕 {i + 1}：{m.name}（{m.width}×{m.height}）
              </option>
            ))}
          </select>
          {monitorError && (
            <button type="button" onClick={fetchMonitors} style={{ border: 0, background: "transparent", color: "#fca5a5", fontSize: 9, cursor: "pointer" }}>
              重新讀取
            </button>
          )}
        </div>
        <button
          className={`toolbar-item-btn ${showSettings ? "active" : ""}`}
          onClick={() => setShowSettings(!showSettings)}
          title="設定"
        >
          <Settings size={18} color="#94a3b8" />
          <span>設定</span>
        </button>
      </div>

      {showSettings && (
        <main className="main-content" style={{ marginTop: 12, flex: 1, overflowY: "auto" }}>
          <div className="utility-grid">
          <section className="settings-section help-section info-entry-card">
              <button className="help-toggle" onClick={() => setShowHelp(true)}>
                <CircleHelp size={17} />
                <span>使用說明</span>
                <span className="help-chevron">查看</span>
              </button>
              <p className="info-entry-summary">截圖、長截圖、錄影、圖片編輯與快捷鍵的操作方式</p>
          </section>
          <section className="settings-section help-section info-entry-card">
              <button className="help-toggle" onClick={() => setShowAbout(true)}>
                <BarChart3 size={17} />
                <span>關於 UniSnap</span>
                <span className="help-chevron">查看</span>
              </button>
              <p className="info-entry-summary">版本、作者、技術架構與 UniSnap 開發歷程</p>
          </section>
          </div>
          {/* Save Folder Settings */}
          <section className="settings-section">
            <h2 className="section-title">
              <Folder size={16} style={{ verticalAlign: "middle", marginRight: 6 }} />
              儲存與路徑設定
            </h2>
            <div className="form-group">
              <label>預設存檔資料夾</label>
              <div className="input-row">
                <input
                  type="text"
                  readOnly
                  value={config.save_directory as string}
                  placeholder="選擇截圖儲存路徑..."
                />
                <button className="btn" onClick={handleBrowseFolder}>瀏覽...</button>
              </div>
            </div>
            <div className="toggle-group">
              <div className="toggle-info">
                <span className="toggle-label">記住存取的資料夾</span>
                <span className="toggle-desc">另存新檔時自動記住上次目錄</span>
              </div>
              <label className="switch">
                <input type="checkbox" checked={config.remember_save_directory}
                  onChange={(e) => saveSettings({ ...config, remember_save_directory: e.target.checked })} />
                <span className="slider"></span>
              </label>
            </div>
            <div className="toggle-group">
              <div className="toggle-info">
                <span className="toggle-label">自動複製到剪貼簿</span>
                <span className="toggle-desc">截圖完成後自動複製到系統剪貼簿</span>
              </div>
              <label className="switch">
                <input type="checkbox" checked={config.auto_copy_to_clipboard}
                  onChange={(e) => saveSettings({ ...config, auto_copy_to_clipboard: e.target.checked })} />
                <span className="slider"></span>
              </label>
            </div>
          </section>

          <section className="settings-section">
            <h2 className="section-title">
              <Settings size={16} style={{ verticalAlign: "middle", marginRight: 6 }} />
              常規設定
            </h2>
            <div className="form-group">
              <label>JPG 壓縮品質 ({config.jpg_quality}%)</label>
              <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                <input type="range" min="10" max="100" value={config.jpg_quality}
                  style={{ flex: 1, accentColor: "var(--accent-color)" }}
                  onChange={(e) => setConfig({ ...config, jpg_quality: parseInt(e.target.value) })}
                  onMouseUp={() => saveSettings(config)} />
                <span style={{ fontSize: 14, width: 36, textAlign: "right" }}>{config.jpg_quality}%</span>
              </div>
            </div>
            <div className="form-group" style={{ marginTop: 12 }}>
              <label>
                <Keyboard size={14} style={{ verticalAlign: "middle", marginRight: 4 }} />
                全域快捷鍵
              </label>
              <div style={{ display: "flex", gap: 12, marginTop: 6 }}>
                <div style={{ flex: 1 }}>
                  <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>截圖</span>
                  <input type="text" readOnly value={config.shortcut_screenshot as string}
                    onKeyDown={(e) => {
                      const shortcut = shortcutFromKeyEvent(e);
                      if (!shortcut) return;
                      e.preventDefault();
                      const updated = { ...config, shortcut_screenshot: shortcut };
                      setConfig(updated);
                      void saveSettings(updated);
                    }}
                    placeholder="按下要使用的組合鍵"
                    style={{ marginTop: 4, width: "100%", cursor: "crosshair" }} />
                </div>
                <div style={{ flex: 1 }}>
                  <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>錄影</span>
                  <input type="text" readOnly value={config.shortcut_recording as string}
                    onKeyDown={(e) => {
                      const shortcut = shortcutFromKeyEvent(e);
                      if (!shortcut) return;
                      e.preventDefault();
                      const updated = { ...config, shortcut_recording: shortcut };
                      setConfig(updated);
                      void saveSettings(updated);
                    }}
                    placeholder="按下要使用的組合鍵"
                    style={{ marginTop: 4, width: "100%", cursor: "crosshair" }} />
                </div>
              </div>
            </div>
          </section>

        </main>
      )}

      {screenPermissionError && (
        <div className="permission-banner">
          ⚠️ 需要<strong>螢幕錄製權限</strong>。請前往
          <strong>系統設定 &gt; 隱私權與安全性 &gt; 螢幕錄製</strong>
          ，勾選本應用程式後重新啟動。
        </div>
      )}
      {statusMessage && <div className="toast">{statusMessage}</div>}
      {showHelp && (
        <InfoDialog title="使用說明" onClose={() => setShowHelp(false)}>
          <div className="info-dialog-section"><h3>一般截圖</h3><p>選擇矩形截圖、全螢幕或工作區。矩形截圖請拖曳框選範圍；完成後可使用編輯工具列標註、複製或儲存。</p></div>
          <div className="info-dialog-section"><h3>長截圖</h3><p>先框選同一個可捲動內容區，再按「開始長截圖」。框選時不要包含瀏覽器分頁列、網址列、捲軸、固定側欄、浮動輸入框或懸浮按鈕；若畫面有多個捲動區域，只選主要內容區。開始後請不要移動滑鼠或操作其他視窗，等待畫面完成捲動與拼接；截圖中若要中止請將滑鼠移開選取區域。</p></div>
          <div className="info-dialog-section"><h3>螢幕錄影</h3><p>框選錄影區域後，在浮動控制列選擇 FPS、麥克風與系統聲音，再按「開始錄影」。完成時按「停止並存檔」。</p></div>
          <div className="info-dialog-section"><h3>圖片編輯</h3><p>開啟舊檔可載入 PNG、JPG 或 JPEG。可裁切、擴增四周空白，並使用畫筆、形狀、箭頭、馬賽克、文字與 OCR。</p></div>
          <div className="info-dialog-section"><h3>快捷鍵與取消</h3><p>一般截圖可按 Escape 取消；長截圖請使用畫面上的「取消」按鈕。全域截圖與錄影快捷鍵可在設定中重新輸入。</p></div>
        </InfoDialog>
      )}
      {showAbout && (
        <InfoDialog title="關於 UniSnap" onClose={() => setShowAbout(false)}>
          <div className="about-panel"><h3>UniSnap V 2.0</h3><p>UniSnap 是 macOS 螢幕截圖、長截圖、圖片編輯與螢幕錄影工具，從需求規劃、介面設計到影像處理逐步整理成模組化架構。</p><p><strong>開發工具：</strong>Tauri、Rust、React、TypeScript、Vite。</p><p><strong>macOS 整合：</strong>ScreenCaptureKit、Vision、CoreGraphics。</p><p><strong>AI 協作工具：</strong>Antigravity IDE 與 ChatGPT。</p><p><strong>作者：</strong>YuJhao Wang</p><p><strong>版本：</strong>V 2.0</p></div>
        </InfoDialog>
      )}
    </div>
  );
}
