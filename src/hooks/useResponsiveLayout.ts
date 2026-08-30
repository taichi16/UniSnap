import { useEffect, useState, type RefObject } from "react";

export function useResponsiveLayout(toolbarRef: RefObject<HTMLDivElement | null>, dependencies: readonly unknown[] = []) {
  const [toolbarWidth, setToolbarWidth] = useState(700);
  const [viewport, setViewport] = useState(() => ({ width: window.innerWidth, height: window.innerHeight }));

  useEffect(() => {
    const updateViewport = () => setViewport({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", updateViewport);
    const toolbar = toolbarRef.current;
    if (!toolbar) return () => window.removeEventListener("resize", updateViewport);
    const updateToolbarWidth = () => {
      const width = toolbar.getBoundingClientRect().width;
      if (width > 0) setToolbarWidth(Math.ceil(width));
    };
    updateToolbarWidth();
    const observer = typeof ResizeObserver !== "undefined" ? new ResizeObserver(updateToolbarWidth) : null;
    observer?.observe(toolbar);
    return () => {
      window.removeEventListener("resize", updateViewport);
      observer?.disconnect();
    };
  // The caller controls the same layout dependencies that previously drove
  // the component-local effect.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, dependencies);

  return { toolbarWidth, viewport };
}
