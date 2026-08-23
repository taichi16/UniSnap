import { useEffect } from "react";

interface ShortcutOptions {
  cropRect: { x: number; y: number; w: number; h: number } | null;
  hoverColor: string;
  isScrollingMode: boolean;
  shapes: unknown[];
  mode: string;
  isStartingRecording: boolean;
  cancelScrollFlow: () => Promise<void>;
  handleCopyOnly: () => Promise<void>;
  startRecordingControl: () => Promise<void>;
  writeColor: (color: string) => Promise<void>;
  showToast: (message: string) => void;
  closeEditor: () => Promise<void>;
}

export function useEditorKeyboardShortcuts(options: ShortcutOptions) {
  useEffect(() => {
    const handleKeyDown = async (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        await options.cancelScrollFlow();
      } else if ((event.metaKey || event.ctrlKey) && (event.key === "c" || event.key === "C")) {
        event.preventDefault();
        await options.handleCopyOnly();
      } else if (!event.metaKey && !event.ctrlKey && (event.key === "c" || event.key === "C")) {
        if (!options.cropRect && options.hoverColor) {
          await options.writeColor(options.hoverColor);
          options.showToast(`已複製顏色: ${options.hoverColor}`);
          setTimeout(async () => options.closeEditor(), 600);
        }
      } else if (options.mode === "record" && options.cropRect && event.key === "Enter" && !options.isStartingRecording) {
        event.preventDefault();
        await options.startRecordingControl();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [options.cropRect, options.hoverColor, options.isScrollingMode, options.shapes, options.mode, options.isStartingRecording]);
}
