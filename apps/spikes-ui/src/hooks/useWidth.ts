import { useCallback, useRef, useState } from "react";

/** The width of an element, following resizes. A callback ref, so it starts measuring whenever the element appears. */
export function useWidth(): [(element: HTMLElement | null) => void, number] {
  const [width, setWidth] = useState(600);
  const observer = useRef<ResizeObserver | null>(null);
  const ref = useCallback((element: HTMLElement | null) => {
    observer.current?.disconnect();
    if (!element) return;
    observer.current = new ResizeObserver(([entry]) => entry && setWidth(Math.max(200, Math.floor(entry.contentRect.width))));
    observer.current.observe(element);
  }, []);
  return [ref, width];
}
