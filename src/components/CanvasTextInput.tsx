import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { getCanvasOverlayPosition } from "../editor/geometry";

export interface TextDraft {
  x: number;
  y: number;
  text: string;
}

interface CanvasTextInputProps {
  draft: TextDraft;
  onDraftChange: Dispatch<SetStateAction<TextDraft | null>>;
  onBlur: () => void;
  inputRef: MutableRefObject<HTMLTextAreaElement | null>;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  isScrollableEditor: boolean;
  color: string;
  fontSize: number;
  fontFamily: string;
}

export default function CanvasTextInput({
  draft,
  onDraftChange,
  onBlur,
  inputRef,
  canvasRef,
  isScrollableEditor,
  color,
  fontSize,
  fontFamily,
}: CanvasTextInputProps) {
  const canvas = canvasRef.current;
  const container = canvas?.parentElement;
  const position = canvas && container
    ? getCanvasOverlayPosition(
      draft,
      canvas,
      canvas.getBoundingClientRect(),
      container.getBoundingClientRect(),
      isScrollableEditor,
    )
    : draft;

  return (
    <textarea
      ref={inputRef}
      className="canvas-text-input"
      value={draft.text}
      aria-label="輸入圖片標註文字"
      onChange={(event) => onDraftChange({ ...draft, text: event.target.value })}
      onBlur={onBlur}
      style={{
        top: position.y,
        left: position.x,
        color,
        fontSize: `${fontSize}px`,
        fontFamily,
        fontWeight: "bold",
        minWidth: 100,
        minHeight: 24,
      }}
    />
  );
}
