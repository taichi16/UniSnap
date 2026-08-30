# UniSnap macOS v2 開發追蹤

本專案是從 UniSnap-refactor 複製出的獨立 macOS v2 專案。v2 的目標不是只增加功能，而是以 Snapzy 公開架構可觀察到的做法，降低長截圖與錄影的高耦合與不可診斷問題。

## 已完成

- 建立獨立 Git 專案：/Users/taichi/AI/UniSnap-MacOS-v2
- 保留 macOS v1 整理版基線，不回寫 /Users/taichi/AI/UniSnap-refactor
- 長截圖影格、位移、固定區域遮罩與總高度封裝為 ScrollCaptureSession
- 加入長截圖 session metrics：擷取步數、穩定取樣次數、接受／拒絕影格數、對齊失敗次數、最終輸出高度與 session 經過時間
- 加入影格尺寸與安全高度的集中驗證
- 補上 session 與 metrics 的 Rust 單元測試

## 進行中

### A. 長截圖即時影格管線

預計將目前同步擷取流程拆成：

ScreenCaptureKit region stream → bounded frame ring → serial commit scheduler → alignment / fixed-band analysis → stitched result

要求：

- 預覽與最終拼接使用同一批影格。
- stream 暫時沒有新影格時，才使用明確標記的 still fallback。
- commit 只能依序完成，結束時必須等待所有 pending work。
- 每次 commit 都要記錄 alignment path 與可信度。

### B. 錄影狀態與時間軸

預計建立明確的：

idle → preparing → recording ↔ paused → stopping → finished / failed

並記錄第一個有效影格 PTS、實際影格數與丟棄影格數、目標 FPS 與實際 FPS、ScreenCaptureKit 回壓、系統音訊／麥克風建立狀態，以及 MP4 封裝完成時間。

### C. 多螢幕實體識別

v1 已改用全域座標與 ScreenCaptureKit bounds 配對；v2 會進一步把螢幕識別抽成獨立 adapter，避免任何功能直接依賴陣列 index。

### D. 使用說明與關於資訊架構

已列入 v2 開發範圍：

- 設定頁只保留「使用說明」與「關於 UniSnap」兩個入口卡片，不直接展開整篇內容。
- 點擊後以獨立視窗或側邊抽屜顯示內容，避免設定頁因長篇文字產生過度捲動。
- 使用說明依一般截圖、長截圖、螢幕錄影、圖片編輯與快捷鍵分節，內容區可獨立捲動且標題列固定。
- 關於頁以資訊卡呈現版本、作者、技術與開發歷程。
- 說明視窗支援明確關閉按鈕與 Escape 關閉，不影響原本截圖、編輯及錄影流程。

## 尚未宣稱完成的人工驗收

下列項目必須在實際 macOS 桌面環境驗證，不能以 Rust 測試或前端建置代替：

- 三螢幕逐一錄影與不同解析度／縮放比例。
- 瀏覽器、Word、PDF 與一般 App 的長截圖內容完整度。
- 動態載入、固定側欄、浮動輸入框與多重捲動區。
- Screen Recording、Microphone、Accessibility 權限。
- 錄影影片實際 FPS、時長、畫面連續性與系統聲音。

## 驗證指令

    cargo test --manifest-path src-tauri/Cargo.toml --all-targets --no-default-features
    cargo clippy --manifest-path src-tauri/Cargo.toml --lib --no-default-features -- -D warnings
    npm run build

執行桌面測試：

    npm run tauri dev 2>&1 | tee /tmp/unisnap-v2.log

## 版本策略

- v2 目前沿用產品顯示版本 V 2.0 的開發目標，但在正式封裝前不提前宣稱 Release。
- v1 與 v2 各自開發、各自測試、各自維護。
- 任何從 Snapzy 借鏡的設計都需重新以 UniSnap 的 Tauri／Rust／macOS bridge 實作，不直接複製 Swift 原始碼。
