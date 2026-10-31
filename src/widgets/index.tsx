import { useState, type ComponentType } from "react";
import type { WidgetType } from "../api";
import { Fit } from "../Fit";
import { formatBytes, formatDuration, formatValue } from "../format";
import { Chart } from "./Chart";
import { Figure, numberValue, rawValue, type WidgetProps } from "./types";

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
        <p className="caption">
          <Fit end>
            {hovered === null
              ? `${values.length} cores`
              : `Core ${hovered + 1}: ${Math.round(values[hovered] ?? 0)}%`}
          </Fit>
        </p>
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

const WIDGETS: Record<WidgetType, ComponentType<WidgetProps>> = {
  chart: Chart,
  mirror_chart: Chart,
  meter: Meter,
  stat: Stat,
  bars: Bars,
  list: List,
  details: Details,
};

export function WidgetView(props: WidgetProps) {
  const Component = WIDGETS[props.widget.type];
  const className = `widget widget-${props.widget.type}${props.hero ? " widget-hero" : ""}`;
  return <section className={className}>{Component && <Component {...props} />}</section>;
}
