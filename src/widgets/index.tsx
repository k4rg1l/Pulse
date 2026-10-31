import { useState, type ComponentType } from "react";
import type { Tab, Unit, Widget, WidgetType } from "../api";
import { Fit } from "../Fit";
import { formatBytes, formatDuration, formatValue } from "../format";
import { Calendar } from "./Calendar";
import { Chart } from "./Chart";
import { Pace } from "./Pace";
import { Figure, Note, numberValue, rawValue, textValue, type WidgetProps } from "./types";

/** The most marks a streak ruler draws before they stop reading as separate days. */
const STREAK_MARKS_LIMIT = 30;

/** Keeps its line even when empty, so a caption arriving never changes the height. */
const EMPTY_LINE = " ";

function Capsule({ percent }: { percent: number }) {
  return (
    <div className="capsule" aria-hidden="true">
      <div className="capsule-fill" style={{ width: `${Math.min(100, Math.max(0, percent))}%` }} />
    </div>
  );
}

function Meter({ widget, tab }: WidgetProps) {
  const [percentKey, secondKey, totalKey] = widget.keys;
  const percent = numberValue(tab, percentKey);
  const second = numberValue(tab, secondKey);
  const total = totalKey ? numberValue(tab, totalKey) : undefined;

  let caption = EMPTY_LINE;
  if (totalKey) {
    if (second !== undefined && total !== undefined) {
      caption = `${formatBytes(second)} of ${formatBytes(total)}`;
    }
  } else if (second !== undefined) {
    caption = `Resets in ${formatDuration((second - Date.now()) / 1000)}`;
  } else if (percent !== undefined) {
    caption = "No active window";
  }

  return (
    <>
      <h2>
        <Fit>{widget.title}</Fit>
      </h2>
      <p className="readout">
        <Fit>{percent === undefined ? "--" : <Figure text={`${Math.round(percent)}%`} />}</Fit>
      </p>
      <Capsule percent={percent ?? 0} />
      <p className="caption">
        <Fit>{caption}</Fit>
      </p>
    </>
  );
}

function Stat({ widget, tab }: WidgetProps) {
  const key = widget.keys[0];
  const value = numberValue(tab, key);
  const metric = tab.metrics.find((candidate) => candidate.key === key);
  return (
    <>
      <h2>
        <Fit>{widget.title}</Fit>
      </h2>
      <p className="readout">
        <Fit>{value === undefined || !metric ? "--" : formatValue(metric.unit, value)}</Fit>
      </p>
      {widget.note && <Note text={textValue(tab, widget.note) ?? EMPTY_LINE} end={false} />}
    </>
  );
}

/** The word that follows a count of days. */
function days(count: number): string {
  return count === 1 ? "day" : "days";
}

/** A ruler as long as the record, marked off as far as the current run has reached. */
function Streak({ widget, tab }: WidgetProps) {
  const current = numberValue(tab, widget.keys[0]);
  const longest = numberValue(tab, widget.keys[1]);
  const marks = Math.min(longest ?? 0, STREAK_MARKS_LIMIT);
  const reached = longest ? Math.round(((current ?? 0) / longest) * marks) : 0;

  return (
    <>
      <h2>
        <Fit>{widget.title}</Fit>
      </h2>
      <p className="readout">
        <Fit>
          {current ?? "--"}
          {current !== undefined && <span className="figure-unit"> {days(current)}</span>}
        </Fit>
      </p>
      <div className="streak" aria-hidden="true">
        {Array.from({ length: marks }, (_, index) => (
          <span key={index} className={index < reached ? "streak-mark-reached" : undefined} />
        ))}
      </div>
      <p className="caption">
        <Fit>{longest === undefined ? EMPTY_LINE : `Best ${longest} ${days(longest)}`}</Fit>
      </p>
    </>
  );
}

function Bars({ widget, tab }: WidgetProps) {
  const [hovered, setHovered] = useState<number | null>(null);
  const raw = rawValue(tab, widget.keys[0]);
  const values = Array.isArray(raw) ? raw.map(Number) : [];
  return (
    <>
      <header className="split">
        <h2>
          <Fit>{widget.title}</Fit>
        </h2>
        <Note
          text={
            hovered === null
              ? (textValue(tab, widget.note) ?? `${values.length} cores`)
              : `Core ${hovered + 1}: ${Math.round(values[hovered] ?? 0)}%`
          }
        />
      </header>
      <div
        className="bars"
        role="img"
        aria-label={values.map((v, i) => `Core ${i + 1} ${Math.round(v)}%`).join(", ")}
        onPointerLeave={() => setHovered(null)}
      >
        {values.map((value, index) => (
          <div className="bar-slot" key={index} onPointerEnter={() => setHovered(index)}>
            <div className="bar" style={{ height: `${Math.max(2, value)}%` }} />
          </div>
        ))}
      </div>
    </>
  );
}

interface ListRow {
  name: string;
  detail: string;
  used: number;
  total: number;
}

function List({ widget, tab }: WidgetProps) {
  const raw = rawValue(tab, widget.keys[0]);
  const rows = (Array.isArray(raw) ? raw : []) as ListRow[];
  return (
    <>
      <h2>
        <Fit>{widget.title}</Fit>
      </h2>
      <ul className="rows">
        {rows.map((row) => (
          <li className="row" key={`${row.detail}-${row.name}`}>
            <div className="split">
              <Fit>
                {row.name} <span className="muted">{row.detail}</span>
              </Fit>
              <span className="caption">
                <Fit end>
                  {formatBytes(row.total - row.used)} free of {formatBytes(row.total)}
                </Fit>
              </span>
            </div>
            <Capsule percent={(row.used / row.total) * 100} />
          </li>
        ))}
      </ul>
    </>
  );
}

interface DetailRow {
  label: string;
  value: string;
}

function Details({ widget, tab }: WidgetProps) {
  const raw = rawValue(tab, widget.keys[0]);
  const rows = (Array.isArray(raw) ? raw : []) as DetailRow[];
  return (
    <>
      <h2>
        <Fit>{widget.title}</Fit>
      </h2>
      <dl className="details">
        {rows.map((row) => (
          <div className="detail" key={row.label}>
            <dt>
              <Fit>{row.label}</Fit>
            </dt>
            <dd title={row.value}>
              <Fit end>{row.value}</Fit>
            </dd>
          </div>
        ))}
      </dl>
    </>
  );
}

interface RecordRow {
  label: string;
  value: number | null;
  unit: Unit;
}

/** Large figures in pairs, for the numbers worth keeping an eye on. */
function Records({ widget, tab }: WidgetProps) {
  const raw = rawValue(tab, widget.keys[0]);
  const rows = (Array.isArray(raw) ? raw : []) as RecordRow[];
  return (
    <>
      <h2>
        <Fit>{widget.title}</Fit>
      </h2>
      <dl className="records">
        {rows.map((row) => (
          <div className="record" key={row.label}>
            <dd className="readout">
              <Fit>{row.value === null ? "--" : formatValue(row.unit, row.value)}</Fit>
            </dd>
            <dt className="caption">
              <Fit>{row.label}</Fit>
            </dt>
          </div>
        ))}
      </dl>
    </>
  );
}

/** Closes a tab with one quiet line. */
export function Colophon({ widget, tab }: Pick<WidgetProps, "widget" | "tab">) {
  const text = textValue(tab, widget.keys[0]);
  return text ? (
    <p className="colophon">
      <Fit>{text}</Fit>
    </p>
  ) : null;
}

const WIDGETS: Record<WidgetType, ComponentType<WidgetProps>> = {
  chart: Chart,
  mirror_chart: Chart,
  pace: Pace,
  meter: Meter,
  stat: Stat,
  bars: Bars,
  calendar: Calendar,
  streak: Streak,
  list: List,
  details: Details,
  records: Records,
  colophon: Colophon,
};

/** A plan decides which limits exist, so one it doesn't have is never drawn empty. */
export function isWidgetShown(widget: Widget, tab: Tab): boolean {
  return widget.type !== "pace" || numberValue(tab, widget.keys[2]) !== undefined;
}

export function WidgetView(props: WidgetProps) {
  const Component = WIDGETS[props.widget.type];
  const className = `widget widget-${props.widget.type}${props.hero ? " widget-hero" : ""}`;
  return <section className={className}>{Component && <Component {...props} />}</section>;
}
