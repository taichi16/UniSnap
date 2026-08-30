import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, ExternalLink, X } from "lucide-react";
import { startDrag } from "@crabnebula/tauri-plugin-drag";

interface QuickAccessItem { imageData: string; path: string; }

export default function QuickAccessWindow({ label }: { label: string }) {
  const [item, setItem] = useState<QuickAccessItem | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<QuickAccessItem>("get_quick_access", { label })
      .then(setItem)
      .catch((reason) => setError(String(reason)));
  }, [label]);

  const close = async () => {
    try { await invoke("close_quick_access", { label }); }
    catch { await getCurrentWindow().close(); }
  };

  const handleDragStart = (event: React.MouseEvent<HTMLAnchorElement>) => {
    if (!item) return;
    event.preventDefault();
    void startDrag({ item: [item.path], icon: item.path }).catch((error) => {
      console.error("Native file drag failed:", error);
    });
  };

  if (error) return <div className="quick-access-window"><p>快速取用載入失敗</p><button onClick={() => void close()}><X size={15} /></button></div>;
  if (!item) return <div className="quick-access-window"><span>載入中…</span></div>;

  return (
    <div className="quick-access-window">
      <header><strong>快速取用</strong><button onClick={() => void close()} aria-label="關閉"><X size={15} /></button></header>
      <a
        className="quick-access-file"
        href={`file://${item.path.split(" ").join("%20")}`}
        onMouseDown={handleDragStart}
        title="拖曳圖片檔案到其他 App"
      >
        <img src={item.imageData} alt="最近儲存的截圖" draggable={false} />
      </a>
      <footer><span title={item.path}>{item.path.split(/[\\/]/).pop()}</span><span><Copy size={13} /> 可拖曳到其他 App</span><ExternalLink size={14} /></footer>
    </div>
  );
}
