/**
 * Pure geometry for the floating editor toolbar.
 *
 * Keeping this separate from CaptureWindow makes edge placement testable
 * without mounting a WebView or invoking any native Tauri command.
 */

export interface ToolbarViewport {
  width: number;
  height: number;
}

export interface DisplayRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ToolbarPlacement {
  position: "fixed" | "absolute";
  top: number;
  left: number;
  transform?: "none";
}

const EDGE_MARGIN = 10;
const FULLSCREEN_WIDTH_RATIO = 0.9;
const FULLSCREEN_HEIGHT_RATIO = 0.8;

export function isNearFullscreen(rect: DisplayRect, viewport: ToolbarViewport): boolean {
  return (
    rect.w > viewport.width * FULLSCREEN_WIDTH_RATIO &&
    rect.h > viewport.height * FULLSCREEN_HEIGHT_RATIO
  );
}

function clampLeft(left: number, toolbarWidth: number, viewportWidth: number): number {
  const maxLeft = viewportWidth - toolbarWidth - EDGE_MARGIN;
  return Math.max(EDGE_MARGIN, Math.min(maxLeft, left));
}

export function calculateToolbarPlacement(
  rect: DisplayRect,
  toolbarWidth: number,
  viewport: ToolbarViewport,
): ToolbarPlacement {
  if (isNearFullscreen(rect, viewport)) {
    return {
      position: "fixed",
      top: 56,
      left: clampLeft((viewport.width - toolbarWidth) / 2, toolbarWidth, viewport.width),
      transform: "none",
    };
  }

  let top = rect.y + rect.h + EDGE_MARGIN;
  if (top + 50 > viewport.height) top = rect.y - 52;

  return {
    position: "absolute",
    top: Math.max(EDGE_MARGIN, top),
    left: clampLeft(rect.x + (rect.w - toolbarWidth) / 2, toolbarWidth, viewport.width),
  };
}
