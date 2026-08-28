# macOS 基線與 Windows 移植分析

## 分析範圍

本文件比較：

- macOS 參考專案：`/Users/taichi/AI/screenshot`
- macOS 重構工作區：`/Users/taichi/AI/UniSnap-refactor`
- Windows 移植專案：`/Users/taichi/AI/UniSnap-Windows`

目前以 macOS 重構版作為主要程式碼、架構與錯誤修正參考；macOS 原始版本仍保留作為功能回歸與歷史修補基線。分析只讀取兩個 macOS 工作區；後續 Windows 重構與實作只寫入 Windows 專案。

## 來源優先級

1. **目前主要程式碼參考：** `/Users/taichi/AI/UniSnap-refactor`
2. **功能回歸／歷史基線：** `/Users/taichi/AI/screenshot`
3. **Windows 實作來源：** `/Users/taichi/AI/UniSnap-Windows` 自己的 command、domain、services 與 Windows platform adapter

重構版雖已成為主要程式碼參考，且已修正部分未整理版本的錯誤，仍不能只因為程式碼較新就推定所有功能與原始版本完全相同；每個修正都需記錄問題、修正方式、回歸結果與 Windows 移植決策。

## 重構版錯誤修正的移植規則

重構版新增或修正的錯誤處理，分為以下四類：

- **跨平台邏輯修正：** Windows 應保留相同的正確行為與錯誤語意。
- **UI／互動修正：** Windows 應納入相同驗收情境，但依 Windows viewport、DPI 與視窗行為重新實作。
- **macOS 平台修正：** 不直接複製 API；需轉換為 Windows 對應的 platform adapter 行為。
- **歷史 workaround 移除：** 先確認原問題與回歸測試，再決定 Windows 是否需要同一 workaround。

## 已移植內容

截至重構版 `451e3f2`，Windows 已先移植前端重構分層，包括編輯器 geometry、toolbar layout、繪圖／輸出模組、hooks 與錄影／長截圖控制元件。Windows 文案與系統音訊限制已依 Windows 契約調整；Rust 原生層仍採 Windows 專案既有實作，未整批帶入 macOS system recording adapter。

Windows 目前也已整合重構版的 Rust 純邏輯模組：`image_data`、`capture_geometry`、`scroll_matching`、`scroll_masks`、`scroll_composite`、`recording_crop`、`recording_output`、`h264_sample`、`recording_timing`、`recording_mp4`、`recording_audio`、`frame_source` 與 `recording_session`。這些模組不依賴 macOS API；平台擷取、輸入與錄影 backend 仍由 Windows 專案自行維護。錄影裁切、輸出路徑、H.264 sample 轉換、時序計算、MP4 軌道／sample 寫入、AAC 錄音流程、影格來源與 session 狀態已從 `record.rs` 抽離，`record.rs` 目前主要負責錄影協調。

`recording_session` 也負責重複啟動競態的收尾：第二個 session 無法安裝時會先停止並等待自己的 worker，避免背景錄影執行緒脫離狀態容器管理。系統音訊則由 Windows-only `recording_system_audio` 管理 WASAPI loopback 生命週期，再交由 `recording_audio` 統一混音與 AAC 編碼。

## 主要觀察

| 區域 | macOS 基線現況 | Windows 移植決策 |
|---|---|---|
| 前端 | `CaptureWindow.tsx` 集中選取、標註、長截圖與錄影互動 | 先保留 UI 行為，逐步建立 command 型別與 Windows 文案／權限狀態 |
| 截圖 command | `capture.rs` 同時處理螢幕列舉、視窗、存檔、剪貼簿與長截圖 | 拆成 command facade、capture service、image service、stitch service、Windows adapter |
| 長截圖 | 已有穩定取樣、位移偵測、固定列／欄遮罩與取消流程 | 保留演算法；擷取影格、游標定位、捲動事件改由 Windows adapter 提供 |
| 錄影 | `record.rs` 同時包含 ScreenCaptureKit、xcap、H.264、AAC 與 session state | Windows 以 xcap／OpenH264／cpal／WASAPI 分別處理畫面、編碼、麥克風與系統音訊；Windows 裝置行為仍待實機驗證 |
| DPI | macOS Retina 修補與邏輯／實體像素轉換散落在流程中 | 集中至 `platform/windows`，只在邊界轉換一次 |
| 快捷鍵 | 預設值依 target 判斷，command 由 `lib.rs` 註冊 | Windows 專用預設值與 shortcut policy，不依賴 macOS 條件 |
| 原生 bridge | ScreenCaptureKit、Objective-C／Swift、CoreGraphics、AppleScript | 不帶入 Windows；Windows 專案 build 不包含 Apple bridge |
| 權限 | Tauri command registration 與 capability allowlist 必須同步 | 保留最小 capability，所有新增 command 需同步測試 allowlist |
| 編輯工具列邊界 | macOS 已修正右側邊界工具列被遮蔽問題（`v1.0-toolbar-fixed`） | 視為跨平台 UI 行為需求；Windows 必須以實際 toolbar 寬度與 viewport 寬度計算並驗收 |

## 歷史修補的移植分類

下列修補具有跨平台價值，但實作不能直接複製：

- Retina／DPI 導致的選取範圍偏移：保留問題定義，改用 Windows Per-Monitor DPI 邊界。
- 長截圖等待畫面穩定：保留演算法，重新驗證 Windows 擷取延遲與捲動幅度。
- 影像位移偵測與固定元件遮罩：屬於 domain logic，可在平台擷取穩定後共用。
- 存檔與複製分離：保留 command 語意，分別測試檔案輸出與 Windows bitmap clipboard。
- command permission 修補：保留追蹤要求，重新檢查 Windows capability。
- macOS ScreenCaptureKit／CGEvent／AppleScript：Windows 不適用，必須改寫或明確排除。
- 右側邊界工具列修正：保留「工具列不可超出 viewport」的行為；不可只複製 macOS 的 CSS 或固定寬度數值。

## 目前高耦合點

1. `capture.rs` 約同時扮演 Tauri command facade、視窗協調器、影像編碼器、剪貼簿服務與長截圖引擎。
2. `record.rs` 把錄影生命週期、影格來源、H.264/AAC 封裝與平台原生錄影混在同一模組。
3. 前端以字串直接呼叫 command，輸入輸出契約沒有集中型別定義。
4. 目前 macOS 參考程式的 platform conditional 不能作為 Windows 架構；只能作為行為與修補來源。

## 重規劃後的責任邊界

```text
src-tauri/src/
├── commands/       # Tauri IPC facade；只做輸入驗證、錯誤映射、服務呼叫
├── domain/         # Image、Stitch、Recording session 與純 Rust 規則
├── services/       # capture、clipboard、file、recording orchestration
├── platform/
│   └── windows/    # DPI、螢幕／視窗擷取、輸入捲動、clipboard、audio backend
└── state/          # pinned image、recording、scroll cancellation
```

重構期間允許保留既有 facade，但新功能不得再直接把 Windows API 或大型演算法塞入 `lib.rs`、`capture.rs` 或 `record.rs`。
