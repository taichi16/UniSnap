# UniSnap-Windows Windows 快速開始

這是一份獨立的 Windows 專案。請勿將本專案與 macOS 專案合併，也不要修改：

- macOS 原始版
- macOS refactor 版

## 1. 解壓縮

請將壓縮檔解壓縮到 Windows 本機可寫入的目錄，例如：

`C:\Projects\UniSnap-Windows`

請避免放在唯讀目錄、雲端同步中的暫存目錄或需要系統管理員權限的目錄。

## 2. 必要環境

請先安裝：

- Node.js 與 npm
- Rust MSVC toolchain
- Visual Studio Build Tools 的 Desktop development with C++ 工作負載
- Windows SDK
- WebView2 Runtime

安裝 Rust 後，確認使用 MSVC toolchain：

```powershell
rustup default stable-x86_64-pc-windows-msvc
```

## 3. 第一次執行

以 PowerShell 進入專案根目錄：

```powershell
cd C:\Projects\UniSnap-Windows
npm ci
powershell -ExecutionPolicy Bypass -File scripts/windows-preflight.ps1
```

若環境檢查沒有阻斷錯誤，執行開發建置：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/windows-build.ps1
```

## 4. 啟動程式

開發模式：

```powershell
npm run tauri dev
```

建立 Release 安裝包：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/windows-build.ps1 -Release
```

輸出通常位於：

`src-tauri\target\release\bundle\`

## 5. 優先驗證項目

請依序驗證：

1. 程式啟動與主視窗顯示。
2. 單螢幕與多螢幕截圖。
3. 100%／150% DPI 與負座標。
4. 右側邊界工具列不被遮蔽。
5. 圖片編輯、存檔與剪貼簿。
6. Chrome／Edge 長截圖。
7. 畫面錄影與 MP4 播放。
8. 麥克風錄音。
9. 系統音訊錄音與麥克風混音。
10. Escape／停止取消流程。

完整項目請參閱：

`docs\WINDOWS_ACCEPTANCE_MATRIX.md`

## 6. 回報問題

若建置或執行失敗，請保留：

- 完整錯誤訊息
- Windows 版本
- GPU 與驅動程式版本
- 螢幕數量與縮放比例
- 使用的指令
- 是否啟用麥克風／系統音訊
- 失敗時的目標應用程式

不要回傳任何 API key、密碼、私鑰或其他機密資料。
