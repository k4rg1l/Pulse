import { useCallback, useEffect, useState, type CSSProperties } from "react";
import { getTab, onSourceChanged, refreshSource, type Tab, type WidgetType } from "./api";
import { Fit } from "./Fit";
import { DAY, formatAgo, HOUR } from "./format";
import { SetupCard } from "./SetupCard";
import { Colophon, isWidgetShown, WidgetView } from "./widgets";

const MINUTE = 60 * 1000;

const RANGES = [
  { label: "15m", name: "15 minutes", ms: 15 * MINUTE },
  { label: "1h", name: "1 hour", ms: HOUR },
  { label: "1d", name: "1 day", ms: DAY },
  { label: "1w", name: "1 week", ms: 7 * DAY },
];

/** Widgets that can lead a tab, running to the panel's edges. */
const HERO_TYPES: WidgetType[] = ["chart", "pace"];

/** Widgets that follow the range control. A tab with none has no use for it. */
const RANGED_TYPES: WidgetType[] = ["chart", "mirror_chart"];

function useTab(sourceId: string) {
  const [tab, setTab] = useState<Tab | null>(null);
  const [version, setVersion] = useState(0);
  const [failure, setFailure] = useState<string | null>(null);

  const load = useCallback(() => {
    getTab(sourceId)
      .then((next) => {
        setTab(next);
        setFailure(null);
        setVersion((value) => value + 1);
      })
      .catch((error) => setFailure(String(error)));
  }, [sourceId]);

  useEffect(() => {
    load();
    const unlisten = onSourceChanged((changed) => {
      if (changed === sourceId) load();
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, [sourceId, load]);

  return { tab, version, failure };
}

function useNow(everyMs: number, running: boolean) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!running) return;
    const timer = setInterval(() => setNow(Date.now()), everyMs);
    return () => clearInterval(timer);
  }, [everyMs, running]);
  return now;
}

function RefreshIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9M13.5 1.8v3h-3"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

interface SourceTabProps {
  sourceId: string;
  /** False while another tab is showing. Tabs stay mounted, so switching is instant. */
  active: boolean;
  onLoaded: (sourceId: string) => void;
}

export function SourceTab({ sourceId, active, onLoaded }: SourceTabProps) {
  const { tab, version, failure } = useTab(sourceId);
  const [rangeMs, setRangeMs] = useState(RANGES[0].ms);
  const now = useNow(1000, active);
  const settled = tab !== null || failure !== null;

  useEffect(() => {
    if (settled) onLoaded(sourceId);
  }, [settled, sourceId, onLoaded]);

  if (!tab) {
    return failure ? <p className="notice">Pulse could not load this tab: {failure}</p> : null;
  }

  const updatedAt = tab.values.reduce((latest, value) => Math.max(latest, value.fetched_at), 0);
  // Until a source is connected the terms are the whole page, with nothing empty below.
  const disconnected = tab.setup?.state === "disconnected";
  const colophon = tab.widgets.find((widget) => widget.type === "colophon");
  const widgets = disconnected
    ? []
    : tab.widgets.filter((widget) => widget !== colophon && isWidgetShown(widget, tab));
  const heroId = widgets.find((widget) => HERO_TYPES.includes(widget.type))?.id;
  const usesRange = widgets.some((widget) => RANGED_TYPES.includes(widget.type));

  const selectedRange = RANGES.findIndex((range) => range.ms === rangeMs);
  const freshness = updatedAt
    ? `Updated ${formatAgo(updatedAt, now)}`
    : "Waiting for the first reading";
  const status = disconnected ? "Not connected" : freshness;

  return (
    <>
      <header className="page-header">
        <div className="page-title">
          <h1>
            <Fit>{tab.name}</Fit>
          </h1>
          <button
            className="icon-button"
            aria-label={`Refresh ${tab.name}`}
            title="Refresh now"
            onClick={() => refreshSource(sourceId)}
          >
            <RefreshIcon />
          </button>
        </div>
        <div className="page-meta">
          <p className="caption">
            <Fit>{status}</Fit>
          </p>
          {usesRange && (
            <div
              className="segmented"
              role="radiogroup"
              aria-label="Chart range"
              style={{ "--selected": selectedRange } as CSSProperties}
            >
              <span className="segmented-thumb" aria-hidden="true" />
              {RANGES.map((range) => (
                <button
                  key={range.ms}
                  role="radio"
                  aria-checked={range.ms === rangeMs}
                  aria-label={range.name}
                  onClick={() => setRangeMs(range.ms)}
                >
                  {range.label}
                </button>
              ))}
            </div>
          )}
        </div>
      </header>

      <SetupCard tab={tab} />

      {tab.error && (
        <div className="banner" role="alert">
          <div>
            <strong>{tab.name} isn't updating.</strong>
            <p>
              {tab.error.message}.{" "}
              {updatedAt ? `Showing data from ${formatAgo(updatedAt, now)}.` : ""}
            </p>
          </div>
          <button className="button" onClick={() => refreshSource(sourceId)}>
            Try again
          </button>
        </div>
      )}

      <div className="widgets">
        {widgets.map((widget) => (
          <WidgetView
            key={widget.id}
            widget={widget}
            tab={tab}
            rangeMs={rangeMs}
            version={version}
            hero={widget.id === heroId}
          />
        ))}
      </div>

      {colophon && !disconnected && <Colophon widget={colophon} tab={tab} />}
    </>
  );
}
