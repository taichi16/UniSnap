import { useCallback, type Dispatch, type SetStateAction } from "react";
import { createTextShape } from "../editor/shapeFactory";
import type { Shape } from "../editor/types";

interface TextInput { x: number; y: number; text: string }

export function useAnnotationActions(
  textInput: TextInput | null,
  shapes: Shape[],
  strokeColor: string,
  strokeWidth: number,
  textFont: string,
  setShapes: Dispatch<SetStateAction<Shape[]>>,
  setTextInput: Dispatch<SetStateAction<TextInput | null>>,
) {
  const handleTextInputBlur = useCallback(() => {
    if (textInput && textInput.text.trim()) {
      setShapes([
        ...shapes,
        createTextShape(textInput, strokeColor, strokeWidth * 6, textFont),
      ]);
    }
    setTextInput(null);
  }, [textInput, shapes, strokeColor, strokeWidth, textFont, setShapes, setTextInput]);

  const handleUndo = useCallback(() => {
    if (shapes.length > 0) setShapes(shapes.slice(0, -1));
  }, [shapes, setShapes]);

  return { handleTextInputBlur, handleUndo };
}
