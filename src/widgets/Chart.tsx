import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { getHistory, type Metric, type Unit } from "../api";
import { Fit } from "../Fit";
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

/** The size the layout gives an element, and what one rem is at that moment. */
function useSize<T extends HTMLElement>() {
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
  const [containerRef, { width, height, rem }] = useSize<HTMLDivElement>();
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<number | null>(null);

  const metrics = widget.keys.map((key) => tab.metrics.find((metric) => metric.key === key));
  const unit: Unit = metrics[0]?.unit ?? "percent";
  // The leading chart runs to the panel's edges, so its labels need the page's
  // inset: --edge on the left, and --edge less --gutter on the right.
  const MARGIN = hero
    ? { top: 0.77 * rem, right: 0.84 * rem, bottom: 1.54 * rem, left: 1.54 * rem }
    : { top: 0.62 * rem, right: 0.15 * rem, bottom: 1.54 * rem, left: 0.15 * rem };
  const mirrored = widget.type === "mirror_chart";
  const mirrorGap = 0.12 * rem;
  const innerWidth = Math.max(0, width - MARGIN.left - MARGIN.right);
  const innerHeight = Math.max(0, height - MARGIN.top - MARGIN.bottom);

  const yMax = useMemo(
    () => niceMax(unit, Math.max(0, ...series.flat().map((point) => point.value))),
    [series, unit],
  );

  const x = (time: number) => MARGIN.left + ((time - from) / (to - from || 1)) * innerWidth;
  const bottom = MARGIN.top + innerHeight;
  const center = MARGIN.top + innerHeight / 2;
  const y = (value: number, seriesIndex = 0) => {
    const fraction = Math.min(value, yMax) / yMax;
    if (!mirrored) return bottom - fraction * innerHeight;
    // Each side starts just off the centre line, so that both stay visible at zero.
    const reach = mirrorGap + fraction * (innerHeight / 2 - mirrorGap);
    return center + (seriesIndex === 0 ? -reach : reach);
  };
  const baseline = mirrored ? center : bottom;

  const maxLabel = formatValue(unit, yMax);
  const gridLines = mirrored
    ? [
        { y: MARGIN.top, label: maxLabel, labelY: MARGIN.top + rem },
        { y: center },
        { y: bottom, label: maxLabel, labelY: bottom - 0.38 * rem },
      ]
    : [0, 0.5, 1].map((fraction) => ({
        y: y(yMax * fraction),
        label: fraction > 0 ? formatValue(unit, yMax * fraction) : undefined,
        labelY: y(yMax * fraction) + rem,
      }));

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
  const hoverX = hovered ? x(hovered.time) : 0;

  // The tooltip takes the side with more room, away from the data under the cursor.
  useLayoutEffect(() => {
    const tooltip = tooltipRef.current;
    if (!tooltip) return;
    const offset = 0.92 * rem;
    const own = tooltip.offsetWidth;
    const left = hoverX > width / 2 ? hoverX - offset - own : hoverX + offset;
    tooltip.style.left = `${Math.max(0, Math.min(left, width - own))}px`;
  });

  return (
    <>
      <header className="chart-header">
        <h2>{hero ? widget.title : <Fit>{widget.title}</Fit>}</h2>
        {series.length === 1 ? (
          <p className="readout">
            <Fit>
              {headline === undefined ? "--" : <Figure text={formatValue(unit, headline)} />}
            </Fit>
          </p>
        ) : (
          <ul className="legend">
            {metrics.map((metric, index) => (
              <li key={widget.keys[index]}>
                <Fit>
                  <span className="legend-item">
                    <span className="legend-key" style={{ background: SERIES_COLORS[index] }} />
                    <span className="legend-value">
                      {latest(metric) === undefined ? "--" : formatValue(unit, latest(metric)!)}
                    </span>
                    <span className="legend-label">{metric?.label}</span>
                  </span>
                </Fit>
              </li>
            ))}
          </ul>
        )}
      </header>

      <div className="chart" ref={containerRef}>
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

              {gridLines.map((line) => (
                <g key={line.y}>
                  <line
                    className="grid-line"
                    x1={MARGIN.left}
                    x2={width - MARGIN.right}
                    y1={line.y}
                    y2={line.y}
                  />
                  {line.label && (
                    <text className="axis-label" x={MARGIN.left} y={line.labelY}>
                      {line.label}
                    </text>
                  )}
                </g>
              ))}

              <text className="axis-label" x={MARGIN.left} y={height - 0.3 * rem}>
                {timeLabel(from)}
              </text>
              <text
                className="axis-label"
                x={width - MARGIN.right}
                y={height - 0.3 * rem}
                textAnchor="end"
              >
                Now
              </text>

              {series.map((points, index) =>
                segments(points).map((segment) => {
                  const line = segment.map((p) => `${x(p.time)},${y(p.value, index)}`).join(" L ");
                  const start = x(segment[0].time);
                  const end = x(segment[segment.length - 1].time);
                  return (
                    <g key={`${index}-${segment[0].time}`}>
                      {(mirrored || series.length === 1) && (
                        <path
                          d={`M ${start},${baseline} L ${line} L ${end},${baseline} Z`}
                          fill={mirrored ? SERIES_COLORS[index] : `url(#fill-${widget.id})`}
                          fillOpacity={mirrored ? 0.16 : undefined}
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
                  <circle key={version} className="pulse-ring" r={0.31 * rem} />
                  <circle className="pulse-dot" r={0.31 * rem} />
                </g>
              )}

              {hovered && (
                <line
                  className="crosshair"
                  x1={x(hovered.time)}
                  x2={x(hovered.time)}
                  y1={MARGIN.top}
                  y2={bottom}
                />
              )}
            </svg>
          )
        )}

        {hovered && (
          <div className="tooltip" ref={tooltipRef}>
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
