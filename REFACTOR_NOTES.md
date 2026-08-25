# UniSnap 整理版說明

版本：**V 1.0**  
整理分支：`refactor/architecture`  
目前 GitHub 主分支：`main`

本文件說明整理版的架構調整、曾經修正的主要錯誤，以及目前已完成與仍需人工驗收的範圍。整理的原則是保留既有功能行為，將高耦合程式拆成責任清楚、可測試的模組；不把「編譯成功」視為桌面功能已完全驗收。

## 一、為何需要整理

早期 `capture.rs` 與 `record.rs` 同時處理 Tauri command、視窗生命週期、螢幕座標、平台輸入、影像處理、影片編碼與音訊收尾。這種歷史累積的高耦合會造成：

- 修改一個平台或錄影流程時，容易影響截圖或長截圖。
- 螢幕縮放、選取矩形與實際影格尺寸混在同一段邏輯中，不易定位 DPI 問題。
- 錄影停止、MP4 封裝、音訊寫入與錯誤回報責任不清楚。
- 長截圖的目標判斷、捲動、對齊與拼接難以分別測試。

整理版沒有把 macOS 與 Windows 混在同一個專案；Windows 仍是獨立專案與後續工作範圍。

## 二、整理後的程式架構

### 截圖與編輯

- `capture.rs`：截圖與長截圖流程入口、Tauri command 與流程協調。
- `capture_modes.rs`：全螢幕與工作區截圖模式。
- `capture_overlay.rs`、`capture_controls.rs`：截圖／錄影控制視窗。
- `capture_io.rs`、`capture_output.rs`、`image_data.rs`：影像解碼、PNG/JPG 輸出、Data URL 與剪貼簿。
- `capture_geometry.rs`、`monitor_resolution.rs`：選取範圍、縮放與螢幕座標轉換。
- `editor_windows.rs`、`editor_image.rs`、`pin_windows.rs`：圖片編輯視窗與置頂圖片視窗。

### 長截圖

- `scroll_target.rs`、`scroll_target_window.rs`：判斷目標視窗與瀏覽器／文件類型。
- `scroll_input.rs`：捲動輸入與停止條件。
- `scroll_matching.rs`：前後影格位移量比對。
- `scroll_masks.rs`：固定側欄、固定列與浮動區域判斷。
- `scroll_composite.rs`：依位移量組合影格。
- `scroll_capture_helpers.rs`：共用裁切與策略標籤。

### 螢幕錄影

- `record.rs`：錄影 command 與錄影工作階段協調。
- `system_recording.rs`：macOS ScreenCaptureKit 錄影生命週期。
- `frame_source.rs`：影格來源抽象化。
- `recording_crop.rs`：選取區域裁切與尺寸保護。
- `recording_encoder_init.rs`、`recording_video_writer.rs`、`recording_finalize.rs`：H.264 影格、MP4 寫入與完成收尾。
- `recording_audio.rs`、`recording_audio_writer.rs`：音訊軌設定與麥克風音訊寫入。
- `macos/screen_capture_kit.m`：macOS 15 以上的 ScreenCaptureKit 實體錄影橋接。

## 三、主要錯誤與修正內容

### 1. 長截圖一按就直接進入拼接或跳到其他頁籤

原因是框選流程與開始擷取流程耦合，且舊流程曾以合成滑鼠操作觸發捲動，可能誤觸瀏覽器分頁或其他控制項。

修正：

- 改為「先框選，再按開始長截圖」的兩階段流程。
- 移除會改變前景頁籤的合成點擊。
- 將捲動輸入、影格擷取與拼接拆開。

### 2. 長截圖內容殘缺、重複或斷裂

原因包括捲動後頁面尚未完成繪製、固定側欄／浮動輸入框被重複拼接，以及選取範圍包含捲軸或外層捲動區。

修正：

- 捲動後加入穩定取樣，避免直接使用尚未完成更新的影格。
- 以影像位移比對決定實際拼接位置，不只依賴固定捲動量。
- 對固定列、固定側欄與浮動區域加入遮罩／裁切處理。
- 明確要求使用者只框選主要可捲動內容區，不包含分頁列、網址列、捲軸與固定元件。

這部分仍受目標應用程式的動態載入、權限與繪製方式影響，需以實際瀏覽器、Word、PDF 或其他 App 個別驗收。

### 3. Retina／DPI 導致長截圖尺寸與選取位置錯誤

原因是視窗邏輯座標、螢幕實體像素與影格尺寸混用。

修正：

- 將螢幕縮放與影像像素轉換集中到幾何／裁切模組。
- 對選取範圍進行邊界保護，避免負座標或超出影格。
- 補上不同縮放比例與偏移螢幕的測試案例。

### 4. 三螢幕錄影選錯螢幕

原始問題是 Tauri 與 ScreenCaptureKit 各自枚舉螢幕，索引順序不保證一致；當兩個螢幕解析度相同時，單靠索引與寬高無法辨識實體螢幕。

修正：

- 從 Rust 傳遞 Tauri 目標螢幕的全域位置與尺寸。
- ScreenCaptureKit 改依 `CGDisplayBounds` 的實體位置比對 `SCDisplay`。
- log 會記錄 `target`、`selected_id` 與 `bounds`，可追蹤實際映射結果。
- 不再以解析度相同的螢幕索引作為唯一依據。

### 5. 錄影只有桌布、畫面黑屏或選取區域不正確

原因是早期影格來源、螢幕選擇與 macOS 系統錄影路徑混用，且選取區域可能被錯誤套用到另一個螢幕。

修正：

- macOS 錄影統一使用 ScreenCaptureKit 的系統錄影輸出。
- 以選定實體螢幕的 local source rectangle 建立錄影範圍。
- 保留排除 UniSnap 自身視窗的設定，避免控制列被錄入。

### 6. 錄影時長與實際錄影時間不一致

原因是影格時間基準與 MP4 封裝收尾不同步，曾造成影片顯示數分鐘但實際只錄幾秒。

修正：

- 統一錄影輸出為 MP4。
- 使用 ScreenCaptureKit 的系統時間軸與完成回呼。
- 停止錄影時等待封裝完成，再回報檔案路徑。

### 7. 停止錄影控制列被錄入影片

修正：

- 錄影內容使用排除 UniSnap 應用程式／視窗的 ScreenCaptureKit filter。
- 控制列維持獨立置頂視窗。
- 仍需在不同螢幕位置與 Dock 貼邊情境下人工驗收。

### 8. 開啟舊檔與圖片編輯流程不穩定

修正：

- 將圖片解碼、編輯視窗建立、PNG/JPG 輸出分離。
- 開啟 PNG、JPG/JPEG 後沿用同一套編輯器。
- 裁切與擴增空白使用獨立影像操作，不影響原始截圖或錄影模組。

### 9. 剪貼簿複製失敗或出現 Command not found

原因不是圖片一定未存檔，而是 Tauri command 的權限／allowlist 未完整註冊。

修正：

- 將剪貼簿複製 command 與儲存流程分離。
- 補齊 command 註冊與權限設定。
- 儲存前即可執行複製，不要求先存檔。

### 10. 編輯工具列與下層選單互相遮蔽

修正：

- 將顏色、形狀、粗細等下層控制視為工具列狀態的一部分。
- 依選取區域與視窗邊界調整工具列位置，避免固定停在右側或上方造成遮蔽。
- 保持工具列與影像編輯視窗分離，降低互相覆蓋的機會。

## 四、驗證結果

已完成的自動化檢查：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --no-default-features
cargo clippy --manifest-path src-tauri/Cargo.toml --lib --no-default-features -- -D warnings
npm run build
npm run tauri build
```

目前整理版曾完成：

- Rust 測試：22 passed，1 ignored（需實際桌面螢幕與權限）。
- Clippy：通過。
- 前端 TypeScript／Vite 建置：通過。
- macOS Apple Silicon `.app` 與 `.dmg` 封裝：通過。
- 版本：`1.0.0`，對外顯示 `V 1.0`。

仍需人工驗收的項目：

- 三螢幕逐一錄影，確認 log 的 `selected_id` 與實體螢幕一致。
- 瀏覽器、Word、PDF 與其他 App 的長截圖內容比對。
- 動態載入頁面、固定側欄與多重捲動區。
- 螢幕錄製、麥克風、系統聲音與輔助使用權限。
- 編輯文字拖曳、工具列下層選單與剪貼簿貼上。

## 五、版本與分支

- `main`：目前 GitHub 公開整理版。
- `backup/pre-refactor-main`：整理前版本備份，供比對或回復。
- macOS 整理版與 Windows 專案維持分離，不共用未驗證的平台實作。

## 六、目前限制

- 長截圖不是 macOS 提供的跨 App 原生 API；非瀏覽器 App 仍需透過畫面取樣與拼接。
- 未簽章／未公證的 macOS 測試封裝，首次開啟可能出現安全性警告。
- Windows 11 版本尚未納入本專案的驗收範圍。
- 動態內容、硬體加速視窗與受系統隱私權限制的內容，可能無法完整擷取。
