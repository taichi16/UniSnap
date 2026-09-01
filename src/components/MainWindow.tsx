import { useState, useEffect, useRef, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { emit, listen } from "@tauri-apps/api/event";
import { availableMonitors, getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Folder, Video, Keyboard, Settings, Scissors, Maximize, AppWindow, ScrollText, Monitor, Sun, Moon, CircleHelp, BarChart3, Image as ImageIcon, Mic, Power, X } from "lucide-react";
import type { AudioInputDeviceInfo, AudioInputTestResult } from "../audio";

interface AppConfig {
  shortcut_screenshot: string;
  shortcut_recording: string;
  save_directory: string;
  remember_save_directory: boolean;
  jpg_quality: number;
  auto_copy_to_clipboard: boolean;
  theme: string;
  close_to_tray: boolean;
  microphone_device_id: string | null;
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
    close_to_tray: true,
    microphone_device_id: null,
  });

  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
  const [showAbout, setShowAbout] = useState(false);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [selectedMonitor, setSelectedMonitor] = useState(0);
  const [monitorError, setMonitorError] = useState<string | null>(null);
  const [screenPermissionError, setScreenPermissionError] = useState(false);
  const [audioInputDevices, setAudioInputDevices] = useState<AudioInputDeviceInfo[]>([]);
  const [microphoneStatus, setMicrophoneStatus] = useState<string | null>(null);
  const [testingMicrophone, setTestingMicrophone] = useState(false);
  const shortcutSaveInFlight = useRef(false);
  const [shortcutSaving, setShortcutSaving] = useState(false);

  useEffect(() => {
    if (!showHelp && !showAbout) return;
    const closeModal = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        setShowHelp(false);
        setShowAbout(false);
      }
    };
    window.addEventListener("keydown", closeModal);
    return () => window.removeEventListener("keydown", closeModal);
  }, [showHelp, showAbout]);

  const fetchAudioInputDevices = async () => {
    try {
      const devices = await invoke<AudioInputDeviceInfo[]>("list_audio_input_devices");
      setAudioInputDevices(devices);
      setMicrophoneStatus(devices.length ? null : "沒有偵測到麥克風");
    } catch (error) {
      console.error("Failed to list microphones:", error);
      setAudioInputDevices([]);
      setMicrophoneStatus(`無法讀取麥克風：${String(error)}`);
    }
  };

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
    fetchAudioInputDevices();

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

  useEffect(() => {
    if (!showSettings) return;
    void fetchAudioInputDevices();
    const timer = window.setInterval(() => void fetchAudioInputDevices(), 3000);
    return () => window.clearInterval(timer);
  }, [showSettings]);


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

  const saveSettings = async (updatedConfig: AppConfig): Promise<boolean> => {
    try {
      await invoke("save_config", { config: updatedConfig });
      setConfig(updatedConfig);
      await emit("config-updated");
      showToast("設定已儲存！");
      return true;
    } catch (err) {
      console.error("Failed to save config:", err);
      showToast(`儲存設定失敗：${String(err)}`);
      return false;
    }
  };

  const saveShortcut = async (
    field: "shortcut_screenshot" | "shortcut_recording",
    shortcut: string,
  ) => {
    if (shortcutSaveInFlight.current) {
      showToast("快捷鍵正在套用，請稍候");
      return;
    }
    const other = field === "shortcut_screenshot"
      ? config.shortcut_recording
      : config.shortcut_screenshot;
    if (shortcut.toLocaleLowerCase() === other.trim().toLocaleLowerCase()) {
      showToast("截圖與錄影快捷鍵不可相同");
      return;
    }

    shortcutSaveInFlight.current = true;
    setShortcutSaving(true);
    try {
      await saveSettings({ ...config, [field]: shortcut });
    } finally {
      shortcutSaveInFlight.current = false;
      setShortcutSaving(false);
    }
  };

  const toggleTheme = async () => {
    const previous = config;
    const nextTheme = config.theme === "light" ? "dark" : "light";
    const updated = { ...config, theme: nextTheme };

    // Apply immediately so the UI is not blocked by disk I/O. If persistence
    // fails, restore the previous in-memory and DOM state together.
    setConfig(updated);
    document.documentElement.dataset.theme = nextTheme;
    try {
      await invoke("save_config", { config: updated });
      await emit("config-updated");
    } catch (err) {
      console.error("Failed to save theme:", err);
      setConfig(previous);
      document.documentElement.dataset.theme = previous.theme || "dark";
      showToast("主題設定儲存失敗");
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

  const testMicrophone = async () => {
    if (testingMicrophone) return;
    setTestingMicrophone(true);
    setMicrophoneStatus("正在測試，請對麥克風說話…");
    try {
      const result = await invoke<AudioInputTestResult>("test_audio_input_device", {
        deviceId: config.microphone_device_id,
      });
      const level = Math.round(result.peak * 100);
      setMicrophoneStatus(result.hasSignal
        ? `麥克風正常，偵測音量 ${level}%`
        : "裝置已開啟，但未偵測到聲音；請確認靜音鍵與輸入音量");
    } catch (error) {
      setMicrophoneStatus(`測試失敗：${String(error)}`);
    } finally {
      setTestingMicrophone(false);
    }
  };

  // Hide frontend window first before invoking backend
  const hideAndInvoke = async (command: string, args?: Record<string, any>) => {
    setScreenPermissionError(false);
    try {
      const currentWin = getCurrentWindow();
      await currentWin.hide();
      // Tauri confirms the hide request before DWM has published the updated
      // desktop.  Native capture also enforces this wait; keeping a frontend
      // frame boundary prevents fast machines from racing the command call.
      await new Promise((resolve) => window.setTimeout(resolve, 120));
      await invoke(command, args);
    } catch (err: any) {
      const currentWin = getCurrentWindow();
      await currentWin.show();
      await currentWin.setFocus();
      console.error(`Failed to invoke ${command}:`, err);
      const msg = String(err);
      if (msg.includes("permission") || msg.includes("denied") || msg.includes("access")) {
        setScreenPermissionError(true);
        showToast("⚠️ 需要螢幕擷取權限！請確認 Windows 已允許螢幕擷取");
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
        {/* Direct buttons are easier to target than a truncated native select on multi-monitor PCs. */}
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 4, flex: "0 0 150px", minWidth: 150 }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", gap: 4, width: "100%" }}>
            <Monitor size={15} color="#a5b4fc" />
            {monitors.map((m, i) => (
              <button
                key={`${m.name}-${m.x}-${m.y}`}
                type="button"
                onClick={() => setSelectedMonitor(i)}
                title={`螢幕 ${i + 1}：${m.name}，${m.width}×${m.height}，位置 (${m.x}, ${m.y})`}
                aria-label={`選擇螢幕 ${i + 1}`}
                style={{
                  width: 27,
                  height: 25,
                  padding: 0,
                  borderRadius: 5,
                  border: selectedMonitor === i ? "2px solid #818cf8" : "1px solid var(--panel-border)",
                  background: selectedMonitor === i ? "rgba(99,102,241,.28)" : "var(--panel-bg)",
                  color: "var(--text-primary)",
                  fontSize: 11,
                  fontWeight: 700,
                  cursor: "pointer",
                }}
              >
                {i + 1}
              </button>
            ))}
          </div>
          <span title={monitors[selectedMonitor]?.name} style={{ maxWidth: 148, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: "var(--text-secondary)", fontSize: 9 }}>
            {monitors.length ? `螢幕 ${selectedMonitor + 1} · ${monitors[selectedMonitor]?.width}×${monitors[selectedMonitor]?.height}` : monitorError ?? "正在讀取螢幕…"}
          </span>
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
          <section className="settings-section help-section">
              <button className="help-toggle" onClick={() => setShowHelp(true)}>
                <CircleHelp size={17} />
                <span>使用說明</span>
                <span className="help-chevron">查看</span>
              </button>
              <p className="help-summary">截圖、長截圖、錄影、圖片編輯與快捷鍵的操作方式</p>
          </section>
          <section className="settings-section help-section">
              <button className="help-toggle" onClick={() => setShowAbout(true)}>
                <BarChart3 size={17} />
                <span>關於 UniSnap</span>
                <span className="help-chevron">查看</span>
              </button>
              <p className="help-summary">版本、作者、技術架構與 UniSnap 開發歷程</p>
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
                <span className="toggle-label">關閉視窗時常駐系統匣</span>
                <span className="toggle-desc">關閉主視窗後保留快捷鍵與背景服務；從系統匣選單「結束」才會退出</span>
              </div>
              <label className="switch">
                <input type="checkbox" checked={config.close_to_tray}
                  onChange={(e) => saveSettings({ ...config, close_to_tray: e.target.checked })} />
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
            <div className="form-group" style={{ marginTop: 16 }}>
              <button
                type="button"
                className="btn"
                onClick={() => void invoke("quit_application")}
                style={{ display: "inline-flex", alignItems: "center", gap: 7 }}
              >
                <Power size={15} />
                完全結束 UniSnap
              </button>
            </div>
          </section>

          <section className="settings-section">
            <h2 className="section-title">
              <Mic size={16} style={{ verticalAlign: "middle", marginRight: 6 }} />
              錄音裝置
            </h2>
            <div className="form-group">
              <label>錄影使用的麥克風</label>
              <div className="input-row">
                <select
                  value={config.microphone_device_id ?? ""}
                  onChange={(event) => {
                    const updated = {
                      ...config,
                      microphone_device_id: event.target.value || null,
                    };
                    setConfig(updated);
                    void saveSettings(updated);
                  }}
                  style={{ flex: 1 }}
                >
                  <option value="">跟隨 Windows 預設裝置</option>
                  {audioInputDevices.map((device) => (
                    <option key={device.id} value={device.id}>
                      {device.name}{device.isDefault ? "（目前預設）" : ""} · {device.channels} 聲道 · {device.sampleRate} Hz
                    </option>
                  ))}
                </select>
                <button className="btn" onClick={() => void fetchAudioInputDevices()}>重新偵測</button>
                <button className="btn" onClick={() => void testMicrophone()} disabled={testingMicrophone}>
                  {testingMicrophone ? "測試中…" : "測試"}
                </button>
              </div>
              {microphoneStatus && (
                <span style={{ display: "block", marginTop: 8, fontSize: 12, color: "var(--text-secondary)" }}>
                  {microphoneStatus}
                </span>
              )}
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
                  <input type="text" readOnly disabled={shortcutSaving} value={config.shortcut_screenshot as string}
                    onKeyDown={(e) => {
                      if (e.repeat) return;
                      const shortcut = shortcutFromKeyEvent(e);
                      if (!shortcut) return;
                      e.preventDefault();
                      void saveShortcut("shortcut_screenshot", shortcut);
                    }}
                    placeholder="按下要使用的組合鍵"
                    style={{ marginTop: 4, width: "100%", cursor: "crosshair" }} />
                </div>
                <div style={{ flex: 1 }}>
                  <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>錄影</span>
                  <input type="text" readOnly disabled={shortcutSaving} value={config.shortcut_recording as string}
                    onKeyDown={(e) => {
                      if (e.repeat) return;
                      const shortcut = shortcutFromKeyEvent(e);
                      if (!shortcut) return;
                      e.preventDefault();
                      void saveShortcut("shortcut_recording", shortcut);
                    }}
                    placeholder="按下要使用的組合鍵"
                    style={{ marginTop: 4, width: "100%", cursor: "crosshair" }} />
                </div>
              </div>
            </div>
          </section>

        </main>
      )}

      {showHelp && (
        <div className="info-modal-backdrop" role="presentation" onMouseDown={() => setShowHelp(false)}>
          <section
            className="info-modal info-modal-wide"
            role="dialog"
            aria-modal="true"
            aria-labelledby="help-modal-title"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <header className="info-modal-header">
              <h2 id="help-modal-title">使用說明</h2>
              <button className="info-modal-close" type="button" aria-label="關閉使用說明" onClick={() => setShowHelp(false)}>
                <X size={20} />
              </button>
            </header>
            <div className="info-modal-body help-modal-content">
              <div><h3>一般截圖</h3><p>選擇矩形截圖、全螢幕或工作區。矩形截圖請拖曳選取範圍；完成後可使用編輯工具列標註、複製或儲存。</p></div>
              <div><h3>長截圖</h3><p>先框選同一個可捲動內容區，再按「開始長截圖」。框選時不要包含瀏覽器分頁列、網址列、捲軸、固定側欄、浮動輸入框或懸浮按鈕；開始後請不要操作其他視窗，等待捲動與拼接完成。</p></div>
              <div><h3>螢幕錄影</h3><p>框選錄影區域後，選擇 FPS、麥克風及系統聲音，再按「開始錄影」。完成時按浮動控制列的「停止並存檔」。</p></div>
              <div><h3>圖片編輯</h3><p>開啟圖片可載入 PNG、JPG 或 JPEG；可裁切、擴增四周空白，並使用畫筆、形狀、箭頭、馬賽克、文字與 OCR。</p></div>
              <div><h3>快捷鍵與取消</h3><p>一般截圖可按 Escape 取消；長截圖使用畫面上的「取消」按鈕。截圖與錄影快捷鍵可在設定中重新輸入。</p></div>
            </div>
          </section>
        </div>
      )}

      {showAbout && (
        <div className="info-modal-backdrop" role="presentation" onMouseDown={() => setShowAbout(false)}>
          <section
            className="info-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="about-modal-title"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <header className="info-modal-header">
              <h2 id="about-modal-title">關於 UniSnap</h2>
              <button className="info-modal-close" type="button" aria-label="關閉關於視窗" onClick={() => setShowAbout(false)}>
                <X size={20} />
              </button>
            </header>
            <div className="info-modal-body about-modal-content">
              <h3>UniSnap V 2.0</h3>
              <p>UniSnap 是 Windows 11 螢幕截圖、長截圖、圖片編輯與螢幕錄影工具，從需求規劃、介面設計到多螢幕與媒體處理逐步整合完成。</p>
              <p><strong>開發工具：</strong>Tauri、Rust、React、TypeScript、Vite。</p>
              <p><strong>Windows 整合：</strong>桌面擷取、剪貼簿、Media Foundation 與音訊裝置。</p>
              <p><strong>AI 協作工具：</strong>Antigravity IDE 與 ChatGPT。</p>
              <p><strong>作者：</strong>YuJhao Wang</p>
              <p><strong>版本：</strong>V 2.0</p>
            </div>
          </section>
        </div>
      )}

      {screenPermissionError && (
        <div className="permission-banner">
          ⚠️ 需要<strong>螢幕擷取權限</strong>。請確認 Windows 已允許本應用程式擷取螢幕，
          並關閉可能阻擋桌面擷取的安全性或隱私工具後重新啟動。
        </div>
      )}
      {statusMessage && <div className="toast">{statusMessage}</div>}
    </div>
  );
}
