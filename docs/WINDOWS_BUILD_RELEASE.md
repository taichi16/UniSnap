# Windows 建置與發行流程

本文件只適用於 `/Users/taichi/AI/UniSnap-Windows`。macOS 專案不參與 Windows 建置。

## 開發機準備

Windows 11 開發機需要：

- Node.js 與 npm
- Rust MSVC toolchain
- Visual Studio Build Tools 的 Desktop development with C++ 工作負載
- Windows 10／11 SDK
- Tauri Windows 建置所需的 WebView2 Runtime

先在 PowerShell 執行：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/windows-preflight.ps1
```

## 建置

開發／除錯版本：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/windows-build.ps1
```

Release bundle：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/windows-build.ps1 -Release
```

建置前會檢查前端 command contract、Rust 格式與測試；安裝包通常位於：

`src-tauri/target/release/bundle/`

## 發行前必要項目

下列項目不能由 macOS 開發環境代替，必須在 Windows 完成：

1. 以乾淨 Windows 11 虛擬機或實體機安裝與移除。
2. 驗證單螢幕／多螢幕、100%／150% DPI 與負座標。
3. 驗證麥克風、WASAPI 系統音訊與長時間錄影。
4. 驗證 Chrome、Edge、Word、PDF 長截圖邊界。
5. 使用正式憑證進行程式碼簽章。
6. 檢查 SmartScreen、防毒軟體與安裝程式警告。
7. 保存版本號、SHA-256、測試環境與失敗記錄。

未完成簽章、乾淨環境安裝及 Windows 驗收前，只能稱為測試建置，不得稱為正式發行版。
