import { useState, type ComponentType } from "react";
import type { WidgetType } from "../api";
import { formatBytes, formatValue } from "../format";
import { Chart } from "./Chart";
import { Figure, numberValue, rawValue, type WidgetProps } from "./types";

function Capsule({ percent }: { percent: number }) {
  return (
    <div className="capsule" aria-hidden="true">
      <div className="capsule-fill" style={{ width: `${Math.min(100, Math.max(0, percent))}%` }} />
    </div>
  );
}

function Meter({ widget, tab }: WidgetProps) {
  const [percentKey, usedKey, totalKey] = widget.keys;
  const percent = numberValue(tab, percentKey);
  const used = numberValue(tab, usedKey);
  const total = numberValue(tab, totalKey);
  return (
    <>
      <h2>{widget.title}</h2>
      <p className="readout">
        {percent === undefined ? "--" : <Figure text={`${Math.round(percent)}%`} />}
      </p>
      <Capsule percent={percent ?? 0} />
      {used !== undefined && total !== undefined && (
        <p className="caption">
          {formatBytes(used)} of {formatBytes(total)}
        </p>
      )}
    </>
  );
}

function Stat({ widget, tab }: WidgetProps) {
  const key = widget.keys[0];
  const value = numberValue(tab, key);
  const metric = tab.metrics.find((candidate) => candidate.key === key);
  return (
    <>
      <h2>{widget.title}</h2>
      <p className="readout">
        {value === undefined || !metric ? "--" : formatValue(metric.unit, value)}
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
      <header className="bars-header">
        <h2>{widget.title}</h2>
        <p className="caption">
          {hovered === null
            ? `${values.length} cores`
            : `Core ${hovered + 1}: ${Math.round(values[hovered] ?? 0)}%`}
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
      <h2>{widget.title}</h2>
      <ul className="rows">
        {rows.map((row) => (
          <li className="row" key={`${row.detail}-${row.name}`}>
            <span className="row-name">{row.name}</span>
            <span className="caption">{row.detail}</span>
            <span className="row-value">
              {formatBytes(row.total - row.used)} free of {formatBytes(row.total)}
            </span>
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
      <h2>{widget.title}</h2>
      <dl className="details">
        {rows.map((row) => (
          <div className="detail" key={row.label}>
            <dt>{row.label}</dt>
            <dd title={row.value}>{row.value}</dd>
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
