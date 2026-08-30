import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, ExternalLink, X } from "lucide-react";

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

  const handleDragStart = (event: React.DragEvent<HTMLImageElement>) => {
    if (!item) return;
    const encodedPath = `file://${item.path.split(" ").join("%20")}`;
    const fileName = item.path.split(/[\\/]/).pop() || "Screenshot.png";
    const mime = /\.jpe?g$/i.test(fileName) ? "image/jpeg" : "image/png";
    event.dataTransfer.effectAllowed = "copy";
    event.dataTransfer.setData("text/uri-list", `${encodedPath}\n`);
    // Do not publish the filesystem path as text: apps such as Pages then
    // insert the path string instead of treating the drag as a file drop.
    // Include the macOS file URL flavor as well as DownloadURL for apps that
    // use the browser drag-and-drop contract.
    event.dataTransfer.setData("public.file-url", encodedPath);
    event.dataTransfer.setData("DownloadURL", `${mime}:${fileName}:${encodedPath}`);
  };

  if (error) return <div className="quick-access-window"><p>快速取用載入失敗</p><button onClick={() => void close()}><X size={15} /></button></div>;
  if (!item) return <div className="quick-access-window"><span>載入中…</span></div>;

  return (
    <div className="quick-access-window">
      <header><strong>快速取用</strong><button onClick={() => void close()} aria-label="關閉"><X size={15} /></button></header>
      <img src={item.imageData} alt="最近儲存的截圖" draggable onDragStart={handleDragStart} title="拖曳圖片到其他 App" />
      <footer><span title={item.path}>{item.path.split(/[\\/]/).pop()}</span><span><Copy size={13} /> 可拖曳到其他 App</span><ExternalLink size={14} /></footer>
    </div>
  );
}
