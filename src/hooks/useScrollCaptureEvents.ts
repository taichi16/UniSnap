import { useEffect, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export function useScrollCaptureEvents(
  mode: string,
  isScrollingModeRef: MutableRefObject<boolean>,
  scrollCancelRequestedRef: MutableRefObject<boolean>,
  cancelScrollFlow: () => Promise<void>,
  showToast: (message: string) => void,
) {
  useEffect(() => {
    if (mode === "record") return;
    let unlisten: (() => void) | undefined;
    listen("global-escape", async () => {
      if (isScrollingModeRef.current) {
        scrollCancelRequestedRef.current = true;
        await invoke("cancel_scroll_capture");
      } else {
        await cancelScrollFlow();
      }
    }).then((cleanup) => { unlisten = cleanup; });
    return () => { unlisten?.(); };
  }, [mode]);

  useEffect(() => {
    if (mode === "record") return;
    let unlisten: (() => void) | undefined;
    listen<{ strategy: string; message: string }>("scroll-capture-strategy", (event) => {
      if (event.payload?.message) showToast(event.payload.message);
    }).then((cleanup) => { unlisten = cleanup; });
    return () => { unlisten?.(); };
  }, [mode]);
}
