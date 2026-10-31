import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import { getHistory } from "../api";

const MAX_POINTS = 240;

export interface Point {
  time: number;
  value: number;
}

interface History {
  series: Point[][];
  from: number;
  to: number;
}

/** Each key's readings over the last `rangeMs`, read again whenever `version` rises. */
export function useHistory(sourceId: string, keys: string[], rangeMs: number, version: number) {
  const [history, setHistory] = useState<History>({ series: keys.map(() => []), from: 0, to: 0 });
  const keyList = keys.join();

  useEffect(() => {
    let current = true;
    const to = Date.now();
    const from = to - rangeMs;
    Promise.all(keys.map((key) => getHistory(sourceId, key, from, to, MAX_POINTS)))
      .then((results) => {
        if (!current) return;
        setHistory({
          series: results.map((values) =>
            values.map((point) => ({ time: point.fetched_at, value: Number(point.value) })),
          ),
          from,
          to,
        });
      })
      .catch(() => {}); // The previous frame stays up, and the next change retries.
    return () => {
      current = false;
    };
    // `keys` is rebuilt on each tab load, so the joined list stands in for it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceId, keyList, rangeMs, version]);

  return history;
}

/** The size the layout gives an element, and what one rem is at that moment. */
export function useSize<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [size, setSize] = useState({ width: 0, height: 0, rem: 13 });
  useLayoutEffect(() => {
    if (!ref.current) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
      setSize({ width, height, rem });
    });
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, []);
  return [ref, size] as const;
}

/**
 * A straight line reads wider than a rounded card of the same width, so the
 * leading chart's lines stop this far short of the cards below it.
 */
const HERO_OPTICAL_INSET = 0.31;

/**
 * The space around a plot. Data sits between `left` and `right`, and text sits
 * between `textLeft` and `textRight`.
 *
 * The leading chart runs to the panel's edges, so its text needs the page's
 * inset: --edge on the left, and --edge less --gutter on the right.
 */
export function plotMargin(hero: boolean, rem: number) {
  const textLeft = (hero ? 1.54 : 0.15) * rem;
  const textRight = (hero ? 0.84 : 0.15) * rem;
  const inset = hero ? HERO_OPTICAL_INSET * rem : 0;
  return {
    top: (hero ? 0.77 : 0.62) * rem,
    bottom: 1.54 * rem,
    textLeft,
    textRight,
    left: textLeft + inset,
    right: textRight + inset,
  };
}

/**
 * Tracks the point nearest the pointer or chosen with the arrow keys. `timeAt`
 * turns a distance from the plot's left edge into a time.
 */
export function usePointHover(points: Point[], timeAt: (offsetX: number) => number) {
  const [hover, setHover] = useState<number | null>(null);
  const hovered = hover === null ? null : points[Math.min(hover, points.length - 1)];

  function onPointerMove(event: PointerEvent<SVGSVGElement>) {
    if (!points.length) return;
    const offset = event.clientX - event.currentTarget.getBoundingClientRect().left;
    const time = timeAt(offset);
    let nearest = 0;
    points.forEach((point, index) => {
      if (Math.abs(point.time - time) < Math.abs(points[nearest].time - time)) nearest = index;
    });
    setHover(nearest);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (!points.length) return;
    const last = points.length - 1;
    if (event.key === "ArrowLeft") setHover(Math.max(0, (hover ?? last + 1) - 1));
    else if (event.key === "ArrowRight") setHover(Math.min(last, (hover ?? last - 1) + 1));
    else if (event.key === "Escape") setHover(null);
  }

  const clear = () => setHover(null);
  return {
    hovered,
    plotEvents: { onPointerMove, onKeyDown, onPointerLeave: clear, onBlur: clear },
  };
}

/** Places a tooltip on the side with more room, away from the data under the cursor. */
export function useTooltipPlacement(anchorX: number, plotWidth: number, rem: number) {
  const tooltipRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const tooltip = tooltipRef.current;
    if (!tooltip) return;
    const offset = 0.92 * rem;
    const own = tooltip.offsetWidth;
    const left = anchorX > plotWidth / 2 ? anchorX - offset - own : anchorX + offset;
    tooltip.style.left = `${Math.max(0, Math.min(left, plotWidth - own))}px`;
  });
  return tooltipRef;
}
