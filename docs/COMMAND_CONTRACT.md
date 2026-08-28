# Windows Command 契約基線

本文件是前端與 Windows backend 之間的穩定邊界。command 名稱暫時沿用既有 UI 使用的名稱；後續若需變更，必須提供相容期或同步更新前端。

## 截圖與影像

| Command | 輸入重點 | 輸出／錯誤 |
|---|---|---|
| `list_monitors` | 無 | 顯示器位置、尺寸、scale factor；列舉失敗須回傳錯誤 |
| `trigger_screenshot` | mode、monitor index | 建立擷取視窗；無螢幕或 index 無效須失敗 |
| `capture_screen_region` | monitor、矩形、輸出格式 | 回傳檔案路徑；矩形超界須失敗 |
| `capture_full_screen` | monitor | 回傳檔案路徑 |
| `capture_work_area` | monitor | 回傳檔案路徑 |
| `save_and_copy_screenshot` | image data、path、autoCopy | 存檔與複製結果須可區分 |
| `copy_screenshot_to_clipboard` | image data | Windows bitmap clipboard 寫入結果 |

## 長截圖

| Command | 輸入重點 | 必要語意 |
|---|---|---|
| `auto_scroll_capture_window` | monitor、選取矩形 | 可取消；必須回傳完成、取消保留結果或明確失敗 |
| `cancel_scroll_capture` | 無 | 可重複呼叫，不得讓下一次流程立即被取消 |

長截圖輸入的座標單位必須在契約中標示為 logical overlay points；backend 只在 Windows platform boundary 轉成 physical pixels。

## 錄影

| Command | 輸入重點 | 目前 Windows 狀態 |
|---|---|---|
| `start_recording` | monitor、矩形、FPS、麥克風、系統音訊 | Windows 使用 WASAPI loopback backend；裝置、權限、同步與 MP4 播放仍待實機驗證 |
| `stop_recording` | 無 | 回傳可播放 MP4 或明確錯誤；逾時不得假裝成功 |

## 契約規則

- 錯誤訊息必須描述使用者可採取的動作，不回傳 Apple 專用權限文字。
- 不得以空字串、空路徑或假成功事件取代失敗。
- 新 command 必須同步更新 capability、前端呼叫、單元測試與 Windows 實機驗收案例。
- 目前 Windows 自訂 command allowlist 已涵蓋 `lib.rs` 註冊的 command，包括螢幕列舉、截圖／編輯、設定、剪貼簿、釘選、長截圖、錄影與錄影控制列；macOS 專用原生 API 不列入 Windows permission。
- 可使用 `npm run verify:contracts` 檢查 `generate_handler!`、`permissions/app-permissions.toml` 與前端字面值 `invoke("...")` 是否有遺漏或多餘 command。`invoke(command, ...)` 這類動態呼叫仍需人工檢查來源。
- `trigger_scroll_capture` 的 `mode` 僅接受 `auto` 或 `manual`；其他值必須在 command 邊界直接失敗。
- `start_recording` 的 `fps` 必須介於 1 到 60；不會靜默把超出範圍的值轉換為邊界值。
- macOS 重構若改變上述 command 的輸入、輸出或錯誤語意，Windows 必須重新進行影響分析。
- macOS 重構版修正的錯誤行為，若涉及 command、事件、取消、錯誤訊息或輸出結果，視為契約變更候選，必須進行 Windows 對應分析。

## UI 回歸基線

- 編輯工具列靠近右側螢幕邊界時，所有按鈕與子工具列仍須可見且可操作。
- 工具列寬度應以實際渲染尺寸計算，不得依賴固定的舊估計值。
- 此行為來自 macOS 原始版本 `v1.0-toolbar-fixed` 修正；Windows 需以自己的 viewport、DPI 與視窗裝飾實作並驗收。
