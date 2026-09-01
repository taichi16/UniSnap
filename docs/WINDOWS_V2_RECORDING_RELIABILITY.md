# Windows V2 錄影可靠性設計

## 本次改版範圍

- 麥克風裝置可列舉、可指定，也可跟隨 Windows 預設裝置。
- 錄影選取畫面會定期重新列舉裝置，外接 USB／藍牙麥克風接入後不需重啟應用程式。
- 1、2、4 或更多聲道的麥克風輸入會正規化成 44.1 kHz stereo；多聲道麥克風陣列不再被直接拒絕。
- 使用者要求麥克風或系統音訊時，初始化失敗會阻止錄影開始並顯示原因，不再靜默產生缺少音訊的影片。
- 停止錄影後進入 `finalizing` 狀態，前端輪詢完成結果；不再用固定 30 秒逾時把仍在封裝的影片誤判成失敗。
- Finalizing 期間 session 仍由 Rust 狀態容器管理，禁止第二段錄影與上一段背景封裝重疊。
- Windows 影片由系統內建 Media Foundation 編碼為 H.264；停止後由內建 AAC 編碼器與 MP4 muxer 合併音訊，不需安裝或隨程式攜帶 FFmpeg。
- Media Foundation 固定使用 Windows 內建軟體 H.264 轉換器，避免 Intel、NVIDIA 或 AMD 顯示驅動內的硬體編碼器在錄影途中使整個程式崩潰。
- MP4 先寫入同資料夾的 `.partial.mp4`，成功完成封裝後才改為正式檔名，避免把中途中斷的檔案顯示為成功錄影。

## 錄影狀態

```text
idle -> recording -> finalizing -> completed
                                -> failed
```

`stop_recording` 只提出停止要求並立即回傳目前狀態；`get_recording_status` 用於取得背景封裝結果。前端在收到 `completed` 前不得關閉控制列或允許開始下一段錄影。

## 麥克風裝置識別

目前 CPAL 未提供跨驅動皆穩定的 Windows endpoint ID，因此設定以「列舉索引＋裝置名稱」保存。若外接裝置消失或列舉結果改變，程式會退回「跟隨 Windows 預設裝置」，不會誤用另一個同名或不同格式的裝置。

後續若 CPAL 提供穩定的 Windows endpoint ID，可改存 MMDevice endpoint ID，並保留目前前端的 `microphoneDeviceId` command 契約。

## Windows 錄影架構

螢幕影格優先由 DXGI 擷取，無法啟動時使用 GDI 相容路徑；裁切後交給 Media Foundation Sink Writer 產生 H.264。麥克風與系統聲音由 WASAPI/CPAL 擷取並統一為 44.1 kHz 雙聲道；停止錄影後再以內建 AAC 編碼器合併到最終 MP4，避免影音軌互相等待。此架構僅依賴 Windows 11 內建媒體元件。
