# UniSnap-Windows 架構說明

## 設計原則

1. Windows 專案不引用 macOS 專案的原始碼、Xcode 設定或 ScreenCaptureKit bridge。
2. React UI 只依賴穩定的 Tauri command／事件契約，不直接呼叫 Windows API。
3. Windows 原生功能集中在 `src-tauri/src/platform/windows/`，以介面隔離平台差異。
4. 權限與能力採最小化設定；每一項新增 command 都必須同步檢查 capability allowlist。
5. 每個功能均保留「需求 → command 契約 → 原生實作 → 測試 → 驗收」的追溯鏈。
6. macOS 專案可以獨立重構；Windows 只同步經過影響分析確認的功能契約、共用邏輯與修補意義，不直接整批覆蓋原始碼。

## macOS 重構與 Windows 同步策略

macOS 版本是重要的產品行為參考，但不是 Windows 原始碼的自動同步來源。當 macOS 重新整理程式碼時，Windows 移植需依下列順序評估：

1. 記錄 macOS 變更所解決的問題、適用範圍與既有測試。
2. 將變更分類為共用邏輯、平台抽象、macOS 原生實作或歷史 workaround。
3. 比對 Windows 的 command 契約、資料格式、錯誤語意與 UI 行為是否受影響。
4. 對需要同步的內容，在 Windows 專案建立獨立實作與測試；不直接複製 macOS platform code。
5. 將「已同步」、「不適用」、「待 Windows 實機驗證」明確記錄，避免重構後產生無法追溯的功能落差。

## 分層

| 層級 | 責任 | Windows 方向 |
|---|---|---|
| UI | 主視窗、選取框、編輯器、設定與提示 | `src/` 下的 React／TypeScript；沿用產品互動概念，重新驗證 Windows 尺寸與字型 |
| Command | 穩定的前後端資料與錯誤契約 | Tauri 2 command，禁止把 Win32 型別外漏至 UI |
| Domain | 影像格式、裁切、標註、拼接、設定 | 與平台無關的 Rust 模組 |
| Platform | 螢幕、視窗、輸入、剪貼簿、錄影、音訊 | Windows Graphics Capture／DXGI、Win32、Windows Clipboard、必要時 Media Foundation |
| Packaging | 安裝、更新、簽章與診斷 | Windows installer、程式碼簽章、SmartScreen 與防毒相容性 |

## 建議的 Windows 原生能力

- 螢幕與視窗擷取：優先評估 Windows Graphics Capture；以 DXGI Desktop Duplication 作為相容性與效能備選。
- 多螢幕與 DPI：以 `GetDpiForMonitor`／Per-Monitor V2 DPI 與實際螢幕座標建立明確轉換層，避免混用邏輯像素與實體像素。
- 輸入與長截圖：以 SendInput／相關視窗訊息建立可取消、可恢復的捲動策略；不得假設所有應用程式都接受相同事件。
- 剪貼簿：使用 Windows bitmap/DIB 格式寫入圖片，並處理剪貼簿被其他程式占用的重試與錯誤。
- 錄影：區分畫面、麥克風與系統音訊三個來源；Windows 系統音訊以 WASAPI loopback 擷取並在 AAC 前統一混音，裝置、權限與同步仍需 Windows 實機驗證。
- OCR：維持 UI 契約，Windows 本機語言資料與效能另列測試項目。

## Command 契約草案

`list_monitors`、`trigger_screenshot`、`capture_screen_region`、`capture_full_screen`、`capture_work_area`、`auto_scroll_capture_window`、`cancel_scroll_capture`、`copy_screenshot_to_clipboard`、`start_recording`、`stop_recording`、`load_config`、`save_config`。

正式實作前需補上輸入結構、輸出結構、錯誤分類、取消語意與權限需求；不得直接照搬 macOS 實作中的未驗證假設。
