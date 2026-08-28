# UniSnap-Windows 移植計畫

## 範圍與邊界

本計畫只針對 `/Users/taichi/AI/UniSnap-Windows`。macOS 專案 `/Users/taichi/AI/screenshot` 是唯讀參考來源，不列入本次修改範圍。

macOS 專案未來可以重新整理或重構。這不會自動覆蓋 Windows 專案；每次重構都必須透過「變更影響分析」判斷是否同步，並保留原修補所解決問題的追蹤資料。

目前 macOS 新整理後的程式碼位於 `/Users/taichi/AI/UniSnap-refactor`，Windows 移植以它作為主要架構、程式碼與錯誤修正參考；`/Users/taichi/AI/screenshot` 保留作為功能回歸與歷史修補基線：

- 前端與 Tauri／Rust 建置成功。
- 截圖、編輯、長截圖、錄影、剪貼簿與設定功能完成 parity 檢查。
- Command 名稱、輸入輸出、事件與 permission 差異已盤點。
- 重構版的未提交變更已被明確記錄，不直接當作已驗證需求。

重構版已處理部分未整理版本的錯誤；這些修正必須建立「問題 → 修正 → macOS 驗證 → Windows 對應實作 → Windows 驗收」追蹤，不得因為未整理版本仍可執行，就將舊行為視為正確基準。

目前重構版 HEAD：`451e3f2 fix: map macOS recording to physical display bounds`。若 HEAD 持續變動，Windows 移植紀錄需同步記錄新的參考版本。

## 階段

## 目前進度

- 階段 0：完成基線與移植邊界。
- 階段 1：完成 React／Tauri／Rust 可編譯基線。
- 階段 2：完成第一版 Windows 座標／DPI 與捲動輸入抽象，並補齊負座標、非有限數值與整數溢位邊界測試。
- 階段 2 尚未完成 Windows 11 實機驗收；混合 DPI、多螢幕、負座標與實際滾輪事件仍待 Windows 環境驗證。
- 階段 6 已建立 Windows WASAPI loopback backend，並接入錄影生命週期與 AAC 輸入；目前仍未完成 Windows 實機裝置、權限、同步與 MP4 播放驗證。
- 架構分析：已完成 macOS 基線、歷史修補與 command 契約盤點；下一步是依分析結果拆出 Windows command facade 與 domain/services 模組。
- UI 回歸項目：已納入 macOS `v1.0-toolbar-fixed` 的右側邊界工具列修正，Windows 尚待實機驗收。
- Command／capability 契約：已完成 `generate_handler!` 與 Windows `allow-app-commands` allowlist 對照，補齊螢幕列舉、編輯器、區域擷取、檔案檢查與錄影控制列 command；尚待 Windows 實機執行時權限驗證。
- 驗收準備：已新增 `npm run verify:contracts` 自動檢查後端註冊、capability allowlist 與前端字面值 `invoke` 一致性，並建立 `docs/WINDOWS_ACCEPTANCE_MATRIX.md`；實機欄位目前尚未填寫。
- Windows 環境準備：已新增 `scripts/windows-preflight.ps1`，供 Windows 實機建立 OS、GPU、音訊裝置與工具鏈摘要；尚未在 Windows 主機執行。
- Command 輸入驗證：`trigger_scroll_capture` 已在 command 邊界拒絕未知 `mode`，避免錯誤模式被當成成功執行。
- 錄影輸入驗證：`start_recording` 已在 command 邊界拒絕不在 1 到 60 範圍內的 FPS，不再靜默修正輸入值。
- 前端重構移植：已帶入重構版 `451e3f2` 的 editor／hooks 分層；下一步比對並選擇性移植 Rust 的純邏輯模組，平台原生模組則重新實作。
- Rust 純邏輯移植：已整合影像資料、截圖幾何、長截圖匹配、固定遮罩、拼接、錄影裁切、輸出路徑、H.264 sample 轉換、錄影時序、MP4 封裝、AAC 錄音、WASAPI 系統音訊、音訊混音、影格來源與 session 狀態模組；本機 Rust 測試目前 40 passed、1 ignored（需實機螢幕權限）。

### 0. 基線與需求凍結

- 建立 Windows 專案與獨立版本識別。
- 將功能拆成截圖、編輯、長截圖、錄影、剪貼簿、快捷鍵、設定、釘選與發行。
- 為每項功能建立正常、異常、邊界、權限與取消情境。

### 1. Tauri／React 最小骨架

- 建立 Windows 專用 Tauri 2 啟動設定、capability 與 command 模組。
- 建立平台抽象：`ScreenCapture`、`WindowControl`、`InputScroll`、`ClipboardImage`、`Recorder`。
- 先以 mock 或明確的「尚未支援」錯誤讓 UI 可開發，避免假成功。

### 2. 螢幕與座標系統

- 實作螢幕列舉、目標螢幕選擇、全螢幕與工作區擷取。
- 驗證混合 DPI、多螢幕排列、負座標、直向螢幕、縮放 100／125／150／200%。
- 確認 overlay、擷取影像與輸出檔案的尺寸一致。

### 3. 區域截圖與編輯器

- 移植選取框、裁切、擴增畫布、標註、馬賽克、文字、PNG/JPG 輸出。
- 驗證高 DPI、IME／繁體中文輸入、字型 fallback、透明度與大量標註效能。

### 4. 剪貼簿、快捷鍵與釘選視窗

- 實作 Windows 圖片剪貼簿、全域快捷鍵、Escape 取消與 always-on-top 釘選視窗。
- 驗證剪貼簿占用、權限不足、快捷鍵衝突、工作站鎖定與多螢幕釘選位置。

### 5. 長截圖

- 以可取消狀態機實作「擷取 → 等待穩定 → 捲動 → 比對 → 拼接」。
- 針對 Chrome、Edge、Firefox、Word、PDF 檢視器與一般 Win32/WPF/Qt 應用程式建立驗收矩陣。
- 明確記錄不支援或結果不可靠的應用程式，不以單一成功案例推論普遍支援。

### 6. 錄影與音訊

- 先完成無音訊區域錄影，再加入麥克風，最後驗證系統音訊。
- 確認 MP4 編碼、影格時間戳、停止／取消、休眠／鎖定與長時間錄影資源使用。
- 已將錄影裁切與輸出檔案路徑抽離至 `recording_crop.rs` 與 `recording_output.rs`；目前僅完成純邏輯整合，尚未完成 Windows 11 實機錄影驗收。
- 已將 OpenH264 Annex-B NAL 轉換與 SPS／PPS 提取抽離至 `h264_sample.rs`；MP4 軌道建立與時間戳流程仍保留在 `record.rs`，後續再拆分錄影時序與封裝責任。
- 已將影格取樣節奏、經過時間、sample 最小長度與時序邊界處理抽離至 `recording_timing.rs`；MP4 軌道管理與 AAC 封裝仍待後續拆分。
- 已將 MP4 視訊／音訊軌道建立、sample 寫入與 AAC packet 時間換算抽離至 `recording_mp4.rs`；目前仍由 `record.rs` 協調影格來源與 AAC 編碼生命週期。
- 已將麥克風串流建立、樣本收集、停止後樣本快照與 AAC 編碼抽離至 `recording_audio.rs`；目前仍待 Windows 11 實機驗證裝置、權限與長時間錄音穩定性。
- 已將 `FrameSource`、原生錄影器停止清理、receiver 取樣與區域擷取抽離至 `frame_source.rs`；錄影 session 的共享停止旗標與完成通道仍由 `record.rs` 管理。
- 已將 session 狀態容器、重複啟動防護、session 取出與空狀態錯誤抽離至 `recording_session.rs`；macOS 與 Windows command 共用同一組狀態契約。
- 已補強 session 安裝競態：若啟動流程在 session 安裝前發生重複啟動，後來的 worker 會收到停止訊號並等待收尾，不會遺留未管理的錄影執行緒。

### 7. 安全、封裝與發行

- 檢查 capability、檔案路徑、輸入驗證、外部程序與暫存檔清理。
- 建立 Windows installer、版本策略、簽章、更新與診斷紀錄。
- 發行前完成乾淨 Windows 11 環境安裝與移除測試。

## 主要風險與決策閘門

### macOS 重構變更追蹤

每次 macOS 重構涉及下列項目時，應在 Windows 移植紀錄中建立一筆對應項目：

- UI／互動流程或使用者可見錯誤訊息。
- Tauri command 名稱、輸入輸出結構、事件名稱或 permission。
- 影像尺寸、DPI、座標、長截圖拼接與錄影時間戳規則。
- 設定檔格式、預設值、檔案命名與輸出格式。
- 已修正的 bug、回歸測試與 workaround 移除。

每筆項目至少標示：`同步`、`Windows 不適用`、`Windows 需改寫` 或 `待實機驗證`。

| 風險 | 影響 | 決策閘門 |
|---|---|---|
| Windows Graphics Capture 的視窗／工作階段限制 | 無法穩定擷取特定程式 | 先完成受支援程式清單與 fallback 策略 |
| DPI 與座標系混用 | 選取範圍偏移、長截圖裁切錯誤 | 多 DPI 實機矩陣通過後才進入長截圖 |
| 系統音訊來源差異 | 錄影功能不一致 | 無音訊、麥克風、系統音訊分開標示狀態 |
| 防毒／SmartScreen／簽章 | 使用者無法安裝或信任 | 未簽章版本不得稱為正式發行版 |
| 長截圖依賴目標程式行為 | 跨應用程式結果不可保證 | 每個目標程式個別驗收並保留失敗證據 |

## 驗收最低標準

- Windows 11 實機：單螢幕與雙螢幕、100% 與 150% DPI。
- 截圖、存檔、複製、開啟舊檔、編輯、釘選與取消流程皆可完成。
- 錄影至少驗證 30 FPS、停止後檔案可播放且時長合理。
- 長截圖至少以 Chrome／Edge／Word 各完成成功與失敗邊界案例。
- 記錄 OS 版本、GPU、縮放比例、目標應用程式、輸出尺寸、錯誤與測試日期。
