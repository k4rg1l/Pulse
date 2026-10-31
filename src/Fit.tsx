import { useLayoutEffect, useRef, type ReactNode } from "react";

/**
 * Holds one line of content in a box sized by the layout, and scales the content
 * down to fit it. The box ignores the content's width, so a value can neither
 * overflow nor move anything around it. Every live value on screen goes through
 * this, and it is why a new source cannot break the layout.
 */
export function Fit({ children, end = false }: { children: ReactNode; end?: boolean }) {
  const box = useRef<HTMLSpanElement>(null);
  const content = useRef<HTMLSpanElement>(null);

  useLayoutEffect(() => {
    const boxElement = box.current;
    const contentElement = content.current;
    if (!boxElement || !contentElement) return;

    const fit = () => {
      const room = boxElement.clientWidth;
      const needed = contentElement.offsetWidth;
      contentElement.style.transform = room > 0 && needed > room ? `scale(${room / needed})` : "";
    };

    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(boxElement);
    observer.observe(contentElement);
    return () => observer.disconnect();
  }, []);

  return (
    <span className={`fit${end ? " fit-end" : ""}`} ref={box}>
      <span className="fit-content" ref={content}>
        {children}
      </span>
    </span>
  );
}
