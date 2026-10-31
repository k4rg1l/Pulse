import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Unit =
  | "percent"
  | "bytes"
  | "bytes_per_second"
  | "seconds"
  | "celsius"
  | "count"
  | "amount"
  | "timestamp";

export type WidgetType =
  | "chart"
  | "mirror_chart"
  | "pace"
  | "meter"
  | "stat"
  | "bars"
  | "calendar"
  | "streak"
  | "list"
  | "details"
  | "records"
  | "colophon";

export interface TabSummary {
  source_id: string;
  name: string;
}

export interface Widget {
  id: string;
  type: WidgetType;
  title: string;
  keys: string[];
  note: string | null;
}

export interface Metric {
  key: string;
  label: string;
  unit: Unit;
}

export interface Value {
  key: string;
  value: unknown;
  fetched_at: number;
}

export interface SourceError {
  message: string;
  occurred_at: number;
}

export interface Terms {
  will: string[];
  never: string[];
  note: string;
}

export interface Setup {
  state: "unavailable" | "disconnected" | "waiting" | "connected";
  title: string;
  detail: string;
  action: string;
  terms: Terms | null;
}

export interface Tab {
  source_id: string;
  name: string;
  widgets: Widget[];
  metrics: Metric[];
  values: Value[];
  error: SourceError | null;
  setup: Setup | null;
}

export interface Setting {
  key: string;
  value: unknown;
}

export interface Anchor {
  side: "top" | "bottom" | "left" | "right";
  offset: number;
}

export interface ClientState {
  anchor: Anchor | null;
  /** False for a client started ahead of time, which waits to be opened. */
  shown: boolean;
}

export interface ApiKeyStatus {
  name: string;
  is_set: boolean;
}

export const listTabs = () => invoke<TabSummary[]>("list_tabs");

export const getTab = (source_id: string) => invoke<Tab>("get_tab", { source_id });

export const getHistory = (
  source_id: string,
  key: string,
  from: number,
  to: number,
  max_points: number,
) => invoke<Value[]>("get_history", { source_id, key, from, to, max_points });

export const clientReady = () => invoke<ClientState>("client_ready");

export const refreshSource = (source_id: string) => invoke<void>("refresh_source", { source_id });

export const connectSource = (source_id: string) => invoke<void>("connect_source", { source_id });

export const disconnectSource = (source_id: string) =>
  invoke<void>("disconnect_source", { source_id });

export const getSettings = () => invoke<Setting[]>("get_settings");

export const updateSetting = (key: string, value: unknown) =>
  invoke<void>("update_setting", { key, value });

export const getApiKeyStatus = (source_id: string) =>
  invoke<ApiKeyStatus[]>("get_api_key_status", { source_id });

export const setApiKey = (source_id: string, name: string, value: string) =>
  invoke<void>("set_api_key", { source_id, name, value });

export const onSourceChanged = (handler: (sourceId: string) => void): Promise<UnlistenFn> =>
  listen<string>("source_changed", (event) => handler(event.payload));

export const onPopoverOpened = (handler: (anchor: Anchor | null) => void): Promise<UnlistenFn> =>
  listen<Anchor | null>("popover_opened", (event) => handler(event.payload));

export const onPopoverClosing = (handler: () => void): Promise<UnlistenFn> =>
  listen("popover_closing", handler);
