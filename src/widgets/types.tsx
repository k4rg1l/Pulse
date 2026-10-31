import type { Tab, Widget } from "../api";
import { Fit } from "../Fit";

export interface WidgetProps {
  widget: Widget;
  tab: Tab;
  rangeMs: number;
  /** Rises each time the source changes, so widgets know to read history again. */
  version: number;
  /** True for the tab's leading chart. */
  hero: boolean;
}

export function rawValue(tab: Tab, key: string): unknown {
  return tab.values.find((value) => value.key === key)?.value;
}

export function numberValue(tab: Tab, key: string): number | undefined {
  const value = rawValue(tab, key);
  return typeof value === "number" ? value : undefined;
}

export function textValue(tab: Tab, key: string | null): string | undefined {
  const value = key ? rawValue(tab, key) : undefined;
  return typeof value === "string" && value ? value : undefined;
}

/** The name printed on an instrument, like the lettering on a watch dial. */
export function Note({ text, end = true }: { text: string; end?: boolean }) {
  return (
    <p className="note">
      <Fit end={end}>{text}</Fit>
    </p>
  );
}

/** A large number with its percent sign set small, as a typesetter would. */
export function Figure({ text }: { text: string }) {
  const percent = text.endsWith("%");
  return (
    <>
      {percent ? text.slice(0, -1) : text}
      {percent && <span className="figure-unit">%</span>}
    </>
  );
}
