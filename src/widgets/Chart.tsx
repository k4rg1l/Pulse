import { useMemo } from "react";
import type { Metric, Unit } from "../api";
import { Fit } from "../Fit";
import { DAY, formatClock, formatDayTime, formatValue } from "../format";
import {
  plotMargin,
  useHistory,
  usePointHover,
  useSize,
  useTooltipPlacement,
  type Point,
} from "./plot";
import {
  Figure,
  Note,
  numberValue,
  PlotLabelMask,
  PlotLabels,
  textValue,
  type WidgetProps,
} from "./types";

const SERIES_COLORS = ["var(--series-1)", "var(--series-2)"];

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

  const metrics = widget.keys.map((key) => tab.metrics.find((metric) => metric.key === key));
  const unit: Unit = metrics[0]?.unit ?? "percent";
  const MARGIN = plotMargin(hero, rem);
  const mirrored = widget.type === "mirror_chart";
  const mirrorGap = 0.12 * rem;
  const innerWidth = Math.max(0, width - MARGIN.left - MARGIN.right);
  const innerHeight = Math.max(0, height - MARGIN.top - MARGIN.bottom);

  const inRange = useMemo(
    () => series.map((points) => points.filter((point) => point.time >= from)),
    [series, from],
  );
  const yMax = useMemo(
    () => niceMax(unit, Math.max(0, ...inRange.flat().map((point) => point.value))),
    [inRange, unit],
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

  const labels = gridLines.flatMap((line) =>
    line.label ? [{ x: MARGIN.textLeft, y: line.labelY, text: line.label }] : [],
  );
  const lead = inRange[0] ?? [];
  const hasData = inRange.some((points) => points.length >= 2);
  const { hovered, plotEvents } = usePointHover(
    lead,
    (offsetX) => from + ((offsetX - MARGIN.left) / innerWidth) * (to - from),
  );

  const latest = (metric: Metric | undefined) =>
    metric ? numberValue(tab, metric.key) : undefined;
  const headline = latest(metrics[0]);
  const note = textValue(tab, widget.note);

  const timeLabel = rangeMs > DAY ? formatDayTime : (t: number) => formatClock(t, false);
  const tooltipRef = useTooltipPlacement(hovered ? x(hovered.time) : 0, width, rem);

  return (
    <>
      <header className="chart-header">
        <div className="chart-title">
          <h2>
            <Fit end={hero}>{widget.title}</Fit>
          </h2>
          {note && <Note text={note} />}
        </div>
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
              {...plotEvents}
            >
              <defs>
                <linearGradient id={`fill-${widget.id}`} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0" stopColor="var(--series-1)" stopOpacity="0.2" />
                  <stop offset="1" stopColor="var(--series-1)" stopOpacity="0" />
                </linearGradient>
                <clipPath id={`plot-${widget.id}`}>
                  <rect x={MARGIN.left} y={0} width={innerWidth} height={height} />
                </clipPath>
                <PlotLabelMask
                  id={`labels-${widget.id}`}
                  width={width}
                  height={height}
                  labels={labels}
                />
              </defs>

              {gridLines.map((line) => (
                <line
                  key={line.y}
                  className="grid-line"
                  x1={MARGIN.left}
                  x2={width - MARGIN.right}
                  y1={line.y}
                  y2={line.y}
                />
              ))}

              <text className="axis-label" x={MARGIN.textLeft} y={height - 0.3 * rem}>
                {timeLabel(from)}
              </text>
              <text
                className="axis-label"
                x={width - MARGIN.textRight}
                y={height - 0.3 * rem}
                textAnchor="end"
              >
                Now
              </text>

              <g clipPath={`url(#plot-${widget.id})`}>
                {series.map((points, index) =>
                  segments(points).map((segment) => {
                    const line = segment
                      .map((p) => `${x(p.time)},${y(p.value, index)}`)
                      .join(" L ");
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
                          mask={`url(#labels-${widget.id})`}
                        />
                      </g>
                    );
                  }),
                )}
              </g>

              <PlotLabels labels={labels} />

              {hero && lead.length > 0 && !hovered && (
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
