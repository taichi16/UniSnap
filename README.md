# UniSnap-Windows

UniSnap 的 Windows 獨立專案。此目錄與 macOS 專案完全分離：

- macOS 專案：`/Users/taichi/AI/screenshot`
- macOS 重構工作區：`/Users/taichi/AI/UniSnap-refactor`
- Windows 專案：`/Users/taichi/AI/UniSnap-Windows`

目前內容是 Windows 獨立移植版本；核心截圖、編輯、長截圖、錄影、麥克風、WASAPI 系統音訊與剪貼簿程式碼已建立，但尚未宣稱通過 Windows 實機驗收。

移植分析基準請參閱 [程式碼分析](docs/CODE_ANALYSIS.md) 與 [Command 契約](docs/COMMAND_CONTRACT.md)。Windows 目前以 `/Users/taichi/AI/UniSnap-refactor` 作為主要架構參考，並以 `/Users/taichi/AI/screenshot` 進行功能回歸與歷史修補比對。

## 目標

保留 UniSnap 的產品體驗與前端功能契約，將螢幕擷取、DPI／多螢幕座標、視窗操作、長截圖捲動、剪貼簿、錄影、音訊及權限處理改為 Windows 原生實作。

## 架構

```text
UniSnap-Windows/
├── src/                  # React + TypeScript UI；不放平台 API
├── src-tauri/
│   ├── src/
│   │   ├── commands/     # Tauri command 邊界
│   │   ├── platform/     # Windows 原生能力與抽象介面
│   │   ├── capture/      # 螢幕／視窗／區域擷取
│   │   ├── stitch/       # 長截圖對齊與拼接
│   │   ├── recording/    # 錄影與音訊
│   │   └── main.rs
│   ├── capabilities/     # 最小權限清單
│   └── tauri.conf.json
├── docs/                 # 需求、架構、驗收與風險
└── tests/                # Windows 實機與跨應用程式驗收資料
```

## 開發狀態

目前已完成第一個移植切片：前端編輯器與 Tauri/Rust 功能基線已帶入，Windows application identifier、Windows capability 與非 Apple 建置流程已獨立化。請先閱讀 [移植計畫](docs/MIGRATION_PLAN.md) 與 [架構說明](docs/ARCHITECTURE.md)。

已驗證：

- `npm run build` 成功。
- `cargo check --manifest-path src-tauri/Cargo.toml` 成功。

尚未驗證：Windows 實機、Windows Graphics Capture、混合 DPI、WASAPI 系統音訊裝置行為、長截圖跨應用程式行為與安裝簽章。Windows 建置流程請參閱 [Windows 建置與發行流程](docs/WINDOWS_BUILD_RELEASE.md)。
