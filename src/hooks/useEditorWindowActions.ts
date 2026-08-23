import { useCallback, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

export function useEditorWindowActions(
  isMainEditor: boolean,
  isScrollingModeRef: MutableRefObject<boolean>,
  scrollCancelRequestedRef: MutableRefObject<boolean>,
) {
  const closeEditor = useCallback(async () => {
    if (isMainEditor) {
      const current = getCurrentWindow();
      window.location.hash = "#/";
      await current.setSize(new LogicalSize(760, 112));
      return;
    }
    await invoke("close_capture_windows");
  }, [isMainEditor]);

  const cancelScrollFlow = useCallback(async () => {
    if (isScrollingModeRef.current) {
      scrollCancelRequestedRef.current = true;
      await invoke("cancel_scroll_capture");
      return;
    }
    await invoke("cancel_scroll_capture");
    await closeEditor();
  }, [closeEditor, isScrollingModeRef, scrollCancelRequestedRef]);

  return { closeEditor, cancelScrollFlow };
}
