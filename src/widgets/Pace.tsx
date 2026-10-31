import { Fit } from "../Fit";
import { DAY, formatDayTime, formatDuration } from "../format";
import { plotMargin, useHistory, usePointHover, useSize, useTooltipPlacement } from "./plot";
import {
  Figure,
  Note,
  numberValue,
  PlotLabelMask,
  PlotLabels,
  textValue,
  type WidgetProps,
} from "./types";

const GRID_PERCENTS = [0, 50, 100];

/** Below this share of the window, the pace so far says nothing yet. */
const TOO_EARLY_TO_FORECAST = 0.01;

interface Graduation {
  time: number;
  label: string;
}

/** The marks along the window's length: each midnight, or each hour of a short one. */
function graduations(start: number, end: number): Graduation[] {
  const marks: Graduation[] = [];
  const mark = new Date(start);
  if (end - start >= 2 * DAY) {
    mark.setHours(24, 0, 0, 0);
    while (mark.getTime() < end) {
      marks.push({
        time: mark.getTime(),
        label: mark.toLocaleDateString([], { weekday: "short" }),
      });
      mark.setDate(mark.getDate() + 1);
    }
  } else {
    mark.setHours(mark.getHours() + 1, 0, 0, 0);
    while (mark.getTime() < end) {
      marks.push({ time: mark.getTime(), label: mark.toLocaleTimeString([], { hour: "numeric" }) });
      mark.setHours(mark.getHours() + 1);
    }
  }
  return marks;
}

/** Where the limit ends up if the rest of the window goes like the part so far. */
function forecast(used: number, start: number, resetsAt: number, now: number) {
  const elapsed = (now - start) / (resetsAt - start);
  if (used >= 100) return { text: `Back ${formatDayTime(resetsAt)}`, over: true };
  if (used === 0) return { text: "Nothing used yet", over: false };
  if (elapsed < TOO_EARLY_TO_FORECAST) return { text: "Too early to call", over: false };

  const projected = used / elapsed;
  if (projected < 100) {
    return { text: `On pace for ${Math.round(projected)}% by reset`, over: false };
  }
  const runsOutAt = start + (now - start) * (100 / used);
  return { text: `Runs out ${formatDayTime(runsOutAt)} at this pace`, over: true };
}

/**
 * A limit drawn across its whole window, so the question it answers is whether
 * this pace lasts until the reset. Usage starts at zero with the window, which
 * means the chart has a shape from its very first reading.
 */
export function Pace({ widget, tab, version, hero }: WidgetProps) {
  const [usageKey, resetsKey, windowKey] = widget.keys;
  const used = numberValue(tab, usageKey) ?? 0;
  const resetsAt = numberValue(tab, resetsKey) ?? 0;
  const windowMs = numberValue(tab, windowKey) ?? DAY;
  const start = resetsAt - windowMs;
  const now = Date.now();

  const { series } = useHistory(tab.source_id, [usageKey], windowMs, version);
  const [containerRef, { width, height, rem }] = useSize<HTMLDivElement>();
  const margin = plotMargin(hero, rem);
  const innerWidth = Math.max(0, width - margin.left - margin.right);
  const innerHeight = Math.max(0, height - margin.top - margin.bottom);
  const bottom = margin.top + innerHeight;

  const x = (time: number) => margin.left + ((time - start) / windowMs) * innerWidth;
  const y = (percent: number) => bottom - (percent / 100) * innerHeight;
  const evenPace = (time: number) => ((time - start) / windowMs) * 100;

  const readings = [
    ...series[0].filter((point) => point.time >= start),
    { time: now, value: used },
  ];
  const { hovered, plotEvents } = usePointHover(
    readings,
    (offsetX) => start + ((offsetX - margin.left) / innerWidth) * windowMs,
  );
  const tooltipRef = useTooltipPlacement(hovered ? x(hovered.time) : 0, width, rem);

  const line = readings.map((point) => `${x(point.time)},${y(point.value)}`).join(" L ");
  const first = readings[0];
  const marks = graduations(start, resetsAt);
  const spacing = (innerWidth / windowMs) * (marks.length > 1 ? marks[1].time - marks[0].time : 0);
  const labelEvery = spacing > 0 ? Math.ceil((2.3 * rem) / spacing) : 1;
  const outlook = forecast(used, start, resetsAt, now);

  // The label sits on the near side of the line, where the line is just below it.
  const paceLabelY = y(100) + rem;
  const paceLabelClearing = ((1.5 * rem) / innerHeight) * windowMs;
  const paceLabelX = x(resetsAt - paceLabelClearing) - 0.46 * rem;
  const labels = [
    ...GRID_PERCENTS.filter((percent) => percent > 0).map((percent) => ({
      x: margin.textLeft,
      y: y(percent) + rem,
      text: `${percent}%`,
    })),
    { x: paceLabelX, y: paceLabelY, text: "Even pace", anchor: "end" as const },
  ];
  const labelMask = `url(#labels-${widget.id})`;

  return (
    <>
      <header className="chart-header">
        <div className="chart-title">
          <h2>
            <Fit end={hero}>{textValue(tab, widget.note) ?? widget.title}</Fit>
          </h2>
          <Note text={`Resets in ${formatDuration((resetsAt - now) / 1000)}`} />
        </div>
        <p className="readout">
          <Fit>
            <Figure text={`${Math.round(used)}%`} />
          </Fit>
        </p>
      </header>

      <div className="chart" ref={containerRef}>
        {width > 0 && (
          <svg
            width={width}
            height={height}
            role="img"
            tabIndex={0}
            aria-label={`${used}% used. ${outlook.text}. Use the left and right arrow keys to read values.`}
            {...plotEvents}
          >
            <defs>
              <linearGradient id={`fill-${widget.id}`} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stopColor="var(--champagne)" stopOpacity="0.22" />
                <stop offset="1" stopColor="var(--champagne)" stopOpacity="0" />
              </linearGradient>
              <PlotLabelMask
                id={`labels-${widget.id}`}
                width={width}
                height={height}
                labels={labels}
              />
            </defs>

            {GRID_PERCENTS.map((percent) => (
              <line
                key={percent}
                className="grid-line"
                x1={margin.left}
                x2={width - margin.right}
                y1={y(percent)}
                y2={y(percent)}
              />
            ))}

            {marks.map((mark, index) => (
              <g key={mark.time}>
                {/* A mark this close to the start would fuse with the line's first point. */}
                {x(mark.time) - margin.left > 0.62 * rem && (
                  <line
                    className="graduation"
                    x1={x(mark.time)}
                    x2={x(mark.time)}
                    y1={bottom}
                    y2={bottom + 0.31 * rem}
                  />
                )}
                {index % labelEvery === 0 && (
                  <text className="axis-label" x={x(mark.time)} y={height - 0.1 * rem}>
                    {mark.label}
                  </text>
                )}
              </g>
            ))}

            <line
              className="pace-line"
              x1={x(start)}
              y1={y(0)}
              x2={x(resetsAt)}
              y2={y(100)}
              mask={labelMask}
            />

            <path
              d={`M ${x(start)},${y(0)} L ${line} L ${x(now)},${y(0)} Z`}
              fill={`url(#fill-${widget.id})`}
            />
            {/* Nothing was measured before the first reading, so that stretch is dimmer. */}
            <path
              className="series-line pace-unmeasured"
              d={`M ${x(start)},${y(0)} L ${x(first.time)},${y(first.value)}`}
              mask={labelMask}
            />
            <path className="series-line pace-measured" d={`M ${line}`} mask={labelMask} />

            <PlotLabels labels={labels} />

            {hero && !hovered && (
              <g transform={`translate(${x(now)} ${y(used)})`}>
                <circle key={version} className="pulse-ring" r={0.31 * rem} />
                <circle className="pulse-dot" r={0.31 * rem} />
              </g>
            )}

            {hovered && (
              <line
                className="crosshair"
                x1={x(hovered.time)}
                x2={x(hovered.time)}
                y1={margin.top}
                y2={bottom}
              />
            )}
          </svg>
        )}

        {hovered && (
          <div className="tooltip" ref={tooltipRef}>
            <p className="tooltip-time">{formatDayTime(hovered.time)}</p>
            <p className="tooltip-row">
              <strong>{Math.round(hovered.value)}%</strong>
              <span>used</span>
            </p>
            <p className="tooltip-row">
              <strong>{Math.round(evenPace(hovered.time))}%</strong>
              <span>even pace</span>
            </p>
          </div>
        )}
      </div>

      <p className={outlook.over ? "caption pace-outlook pace-outlook-over" : "caption pace-outlook"}>
        <Fit>{outlook.text}</Fit>
      </p>
    </>
  );
}
