# Capabilities

Windows 專案採最小權限。自訂 command 必須同時存在於 `lib.rs` 的 `generate_handler!` 與 `permissions/app-permissions.toml`；新增或移除 command 時必須同步檢查兩者，避免編譯成功但執行時被 capability 拒絕。
