import { getCurrentWindow } from "@tauri-apps/api/window";

const helpItems = [
  ["截圖", "選擇矩形截圖、全螢幕或工作區，再拖曳選取範圍。"],
  ["長截圖", "框選主要可捲動內容區後開始，完成後可在編輯器裁切、標註與馬賽克。"],
  ["錄影", "框選區域後選擇 FPS、麥克風與系統聲音，再按開始錄影。"],
  ["儲存", "在編輯工具列選擇 PNG 或 JPG，再按下載圖示另存。"],
  ["取消", "一般截圖按 Esc；長截圖按取消；錄影按停止並存檔。"],
];

export default function InfoWindow({ kind }: { kind: "help" | "about" }) {
  const close = () => void getCurrentWindow().close();
  return <main className="info-window">
    <header><h1>{kind === "about" ? "關於 UniSnap" : "使用說明"}</h1><button onClick={close}>關閉</button></header>
    {kind === "help" ? <ul>{helpItems.map(([title, text]) => <li key={title}><strong>{title}</strong><span>{text}</span></li>)}</ul> : <>
      <p>UniSnap 是螢幕截圖、長截圖與螢幕錄影工具。</p>
      <ul><li><strong>版本</strong><span>1.0</span></li><li><strong>技術</strong><span>Tauri、Rust、React、TypeScript</span></li><li><strong>作者</strong><span>YuJhao Wang</span></li></ul>
    </>}
  </main>;
}
