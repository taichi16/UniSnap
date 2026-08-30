import { useEffect, type Dispatch, type SetStateAction } from "react";

interface ImageSize {
  width: number;
  height: number;
}

interface SelectionRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function useEditorSelectionInitialization(
  isMainEditor: boolean,
  imageLoaded: boolean,
  imageSize: ImageSize | null,
  cropRect: SelectionRect | null,
  setCropRect: Dispatch<SetStateAction<SelectionRect | null>>,
) {
  useEffect(() => {
    if (!isMainEditor || !imageLoaded || cropRect) return;
    const width = imageSize?.width ?? window.innerWidth;
    const height = imageSize?.height ?? window.innerHeight;
    setCropRect({ x: 0, y: 0, w: width, h: height });
  }, [isMainEditor, imageLoaded, cropRect, imageSize, setCropRect]);
}
