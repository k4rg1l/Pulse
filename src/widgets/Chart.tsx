import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { getHistory, type Metric, type Unit } from "../api";
import { formatClock, formatDayTime, formatValue } from "../format";
import { Figure, numberValue, type WidgetProps } from "./types";

const MAX_POINTS = 240;
const DAY = 24 * 60 * 60 * 1000;
const SERIES_COLORS = ["var(--series-1)", "var(--series-2)"];

interface Point {
  time: number;
  value: number;
}

interface History {
  series: Point[][];
  from: number;
  to: number;
}

function useHistory(sourceId: string, keys: string[], rangeMs: number, version: number) {
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

function useWidth<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    if (!ref.current) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, []);
  return [ref, width] as const;
}

/** Rounds up to 1, 2 or 5 times a power of 1024, so axis labels read cleanly. */
function niceMax(unit: Unit, largest: number): number {
  if (unit === "percent") return 100;
  if (largest <= 0) return 1024;
  const base = Math.pow(1024, Math.floor(Math.log(largest) / Math.log(1024)));
  const step = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1024].find((n) => n * base >= largest);
  return (step ?? 1024) * base;
}

/** Splits a series where the app was not running, so the line never bridges a gap. */
function segments(points: Point[]): Point[][] {
  if (points.length < 2) return [];
  const steps = points.slice(1).map((point, index) => point.time - points[index].time);
  const typical = [...steps].sort((a, b) => a - b)[Math.floor(steps.length / 2)];
  const result: Point[][] = [[points[0]]];
  steps.forEach((step, index) => {
    if (step > typical * 4) result.push([]);
    result[result.length - 1].push(points[index + 1]);
  });
  return result;
}

export function Chart({ widget, tab, rangeMs, version, hero }: WidgetProps) {
  const { series, from, to } = useHistory(tab.source_id, widget.keys, rangeMs, version);
  const [containerRef, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);

  const metrics = widget.keys.map((key) => tab.metrics.find((metric) => metric.key === key));
  const unit: Unit = metrics[0]?.unit ?? "percent";
  // The leading chart runs to the panel's edges, so its labels need the inset.
  const MARGIN = hero
    ? { top: 10, right: 20, bottom: 20, left: 20 }
    : { top: 8, right: 2, bottom: 20, left: 2 };
  const height = hero ? 148 : 104;
  const innerWidth = Math.max(0, width - MARGIN.left - MARGIN.right);
  const innerHeight = height - MARGIN.top - MARGIN.bottom;

  const yMax = useMemo(
    () => niceMax(unit, Math.max(0, ...series.flat().map((point) => point.value))),
    [series, unit],
  );

  const x = (time: number) => MARGIN.left + ((time - from) / (to - from || 1)) * innerWidth;
  const y = (value: number) => MARGIN.top + (1 - Math.min(value, yMax) / yMax) * innerHeight;
  const baseline = y(0);

  const lead = series[0] ?? [];
  const hasData = series.some((points) => points.length >= 2);
  const hovered = hover === null ? null : lead[Math.min(hover, lead.length - 1)];

  function hoverAt(clientX: number, left: number) {
    if (!lead.length) return;
    const time = from + ((clientX - left - MARGIN.left) / innerWidth) * (to - from);
    let nearest = 0;
    lead.forEach((point, index) => {
      if (Math.abs(point.time - time) < Math.abs(lead[nearest].time - time)) nearest = index;
    });
    setHover(nearest);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (!lead.length) return;
    const last = lead.length - 1;
    if (event.key === "ArrowLeft") setHover(Math.max(0, (hover ?? last + 1) - 1));
    else if (event.key === "ArrowRight") setHover(Math.min(last, (hover ?? last - 1) + 1));
    else if (event.key === "Escape") setHover(null);
  }

  const latest = (metric: Metric | undefined) =>
    metric ? numberValue(tab, metric.key) : undefined;
  const headline = latest(metrics[0]);

  const timeLabel = rangeMs > DAY ? formatDayTime : (t: number) => formatClock(t, false);
  const tooltipLeft = hovered ? Math.max(8, Math.min(x(hovered.time) + 12, width - 148)) : 0;

  return (
    <>
      <header className="chart-header">
        <h2>{widget.title}</h2>
        {series.length === 1 ? (
          <p className="readout">
            {headline === undefined ? "--" : <Figure text={formatValue(unit, headline)} />}
          </p>
        ) : (
          <ul className="legend">
            {metrics.map((metric, index) => (
              <li key={widget.keys[index]}>
                <span className="legend-key" style={{ background: SERIES_COLORS[index] }} />
                <span className="legend-value">
                  {latest(metric) === undefined ? "--" : formatValue(unit, latest(metric)!)}
                </span>
                <span className="legend-label">{metric?.label}</span>
              </li>
            ))}
          </ul>
        )}
      </header>

      <div className="chart" ref={containerRef} style={{ height }}>
        {!hasData ? (
          <p className="chart-empty">Pulse draws this chart as readings come in.</p>
        ) : (
          width > 0 && (
            <svg
              width={width}
              height={height}
              role="img"
              tabIndex={0}
              aria-label={`${widget.title} over the selected range. Use the left and right arrow keys to read values.`}
              onPointerMove={(event) =>
                hoverAt(event.clientX, event.currentTarget.getBoundingClientRect().left)
              }
              onPointerLeave={() => setHover(null)}
              onBlur={() => setHover(null)}
              onKeyDown={onKeyDown}
            >
              <defs>
                <linearGradient id={`fill-${widget.id}`} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0" stopColor="var(--series-1)" stopOpacity="0.2" />
                  <stop offset="1" stopColor="var(--series-1)" stopOpacity="0" />
                </linearGradient>
              </defs>

              {[0, 0.5, 1].map((fraction) => (
                <g key={fraction}>
                  <line
                    className="grid-line"
                    x1={MARGIN.left}
                    x2={width - MARGIN.right}
                    y1={y(yMax * fraction)}
                    y2={y(yMax * fraction)}
                  />
                  {fraction > 0 && (
                    <text className="axis-label" x={MARGIN.left} y={y(yMax * fraction) + 13}>
                      {formatValue(unit, yMax * fraction)}
                    </text>
                  )}
                </g>
              ))}

              <text className="axis-label" x={MARGIN.left} y={height - 4}>
                {timeLabel(from)}
              </text>
              <text className="axis-label" x={width - MARGIN.right} y={height - 4} textAnchor="end">
                Now
              </text>

              {series.map((points, index) =>
                segments(points).map((segment) => {
                  const line = segment.map((p) => `${x(p.time)},${y(p.value)}`).join(" L ");
                  const start = x(segment[0].time);
                  const end = x(segment[segment.length - 1].time);
                  return (
                    <g key={`${index}-${segment[0].time}`}>
                      {series.length === 1 && (
                        <path
                          d={`M ${start},${baseline} L ${line} L ${end},${baseline} Z`}
                          fill={`url(#fill-${widget.id})`}
                        />
                      )}
                      <path
                        className="series-line"
                        d={`M ${line}`}
                        stroke={SERIES_COLORS[index]}
                      />
                    </g>
                  );
                }),
              )}

              {hero && lead.length > 0 && hover === null && (
                <g transform={`translate(${x(lead[lead.length - 1].time)} ${y(lead[lead.length - 1].value)})`}>
                  <circle key={version} className="pulse-ring" r="4" />
                  <circle className="pulse-dot" r="4" />
                </g>
              )}

              {hovered && (
                <line
                  className="crosshair"
                  x1={x(hovered.time)}
                  x2={x(hovered.time)}
                  y1={MARGIN.top}
                  y2={baseline}
                />
              )}
            </svg>
          )
        )}

        {hovered && (
          <div className="tooltip" style={{ left: tooltipLeft }}>
            <p className="tooltip-time">{formatClock(hovered.time, rangeMs <= DAY)}</p>
            {series.map((points, index) => {
              const point = points.reduce<Point | null>(
                (best, candidate) =>
                  !best ||
                  Math.abs(candidate.time - hovered.time) < Math.abs(best.time - hovered.time)
                    ? candidate
                    : best,
                null,
              );
              return (
                <p className="tooltip-row" key={widget.keys[index]}>
                  <span className="tooltip-key" style={{ background: SERIES_COLORS[index] }} />
                  <strong>{point ? formatValue(unit, point.value) : "--"}</strong>
                  <span>{metrics[index]?.label}</span>
                </p>
              );
            })}
          </div>
        )}
      </div>
    </>
  );
}
