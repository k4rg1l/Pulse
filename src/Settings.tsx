import { useEffect, useState } from "react";
import {
  disconnectSource,
  getApiKeyStatus,
  getSettings,
  getStartAtLogin,
  getTab,
  setApiKey,
  setStartAtLogin,
  updateSetting,
  type ApiKeyStatus,
  type Metric,
  type Setup,
  type TabSummary,
} from "./api";
import { Picker } from "./Picker";
import { ShortcutRecorder } from "./ShortcutRecorder";
import { Switch } from "./Switch";
import { TabOrder } from "./TabOrder";
import { UpdateRow } from "./UpdateRow";

interface TrayStat {
  source_id: string;
  key: string;
}

interface SourceOptions {
  tab: TabSummary;
  metrics: Metric[];
  keys: ApiKeyStatus[];
  setup: Setup | null;
}

const DEFAULT_TRAY_STAT: TrayStat = { source_id: "system", key: "cpu.usage" };

/** Matches DEFAULT_HISTORY_DAYS in src-tauri/src/store.rs. */
const DEFAULT_HISTORY_DAYS = 14;

const HISTORY_LENGTHS = [
  { value: "7", label: "1 week" },
  { value: "14", label: "2 weeks" },
  { value: "30", label: "30 days" },
];

const statId = ({ source_id, key }: TrayStat) => `${source_id}/${key}`;

function ApiKeyRow({
  sourceId,
  status,
  onSaved,
}: {
  sourceId: string;
  status: ApiKeyStatus;
  onSaved: () => void;
}) {
  const [value, setValue] = useState("");
  const [failure, setFailure] = useState<string | null>(null);

  function save() {
    setApiKey(sourceId, status.name, value)
      .then(() => {
        setValue("");
        setFailure(null);
        onSaved();
      })
      .catch((error) => setFailure(String(error)));
  }

  return (
    <li className="setting">
      <div className="setting-text">
        <span>{status.name}</span>
        <span className="caption">{failure ?? (status.is_set ? "Saved" : "Not set")}</span>
      </div>
      <div className="setting-entry">
        <input
          className="field"
          type="password"
          autoComplete="off"
          aria-label={`${status.name} key`}
          placeholder={status.is_set ? "Replace key" : "Enter key"}
          value={value}
          onChange={(event) => setValue(event.target.value)}
        />
        <button className="button" disabled={!value} onClick={save}>
          Save key
        </button>
      </div>
    </li>
  );
}

export function Settings({
  tabs,
  active,
  onOpenTab,
  onTabsChanged,
}: {
  tabs: TabSummary[];
  active: boolean;
  onOpenTab: (sourceId: string) => void;
  onTabsChanged: () => void;
}) {
  const [sources, setSources] = useState<SourceOptions[]>([]);
  const [trayStat, setTrayStat] = useState<TrayStat>(DEFAULT_TRAY_STAT);
  const [defaultTab, setDefaultTab] = useState<string>("");
  const [keepReady, setKeepReady] = useState(false);
  const [startAtLogin, setStartsAtLogin] = useState(false);
  const [historyDays, setHistoryDays] = useState(DEFAULT_HISTORY_DAYS);
  const [failure, setFailure] = useState<string | null>(null);

  function loadSources() {
    Promise.all(
      tabs.map(async (tab) => {
        const { metrics, setup } = await getTab(tab.source_id);
        return { tab, metrics, setup, keys: await getApiKeyStatus(tab.source_id) };
      }),
    )
      .then(setSources)
      .catch((error) => setFailure(String(error)));
  }

  // Loaded ahead of being shown, so the panel is the right height on the first
  // frame, and again each time it is shown, as connections can change elsewhere.
  useEffect(() => {
    if (tabs.length > 0) loadSources();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabs, active]);

  useEffect(() => {
    getSettings()
      .then((settings) => {
        for (const { key, value } of settings) {
          if (key === "tray_stat") setTrayStat(value as TrayStat);
          if (key === "default_tab") setDefaultTab(value as string);
          if (key === "keep_client_ready") setKeepReady(value === true);
          if (key === "history_days") setHistoryDays(value as number);
        }
      })
      .catch((error) => setFailure(String(error)));
    getStartAtLogin()
      .then(setStartsAtLogin)
      .catch((error) => setFailure(String(error)));
  }, []);

  function save(key: string, value: unknown) {
    updateSetting(key, value).catch((error) => setFailure(String(error)));
  }

  function chooseTrayStat(id: string) {
    const stat = sources
      .flatMap(({ tab, metrics }) =>
        metrics.map((metric) => ({ source_id: tab.source_id, key: metric.key })),
      )
      .find((candidate) => statId(candidate) === id);
    if (!stat) return;
    setTrayStat(stat);
    save("tray_stat", stat);
  }

  function chooseDefaultTab(sourceId: string) {
    setDefaultTab(sourceId);
    save("default_tab", sourceId);
  }

  function chooseKeepReady(on: boolean) {
    setKeepReady(on);
    save("keep_client_ready", on);
  }

  function chooseStartAtLogin(on: boolean) {
    setStartAtLogin(on)
      .then(() => setStartsAtLogin(on))
      .catch((error) => setFailure(String(error)));
  }

  function chooseHistoryDays(days: string) {
    setHistoryDays(Number(days));
    save("history_days", Number(days));
  }

  function disconnect(sourceId: string) {
    disconnectSource(sourceId)
      .then(loadSources)
      .catch((error) => setFailure(String(error)));
  }

  const withKeys = sources.filter((source) => source.keys.length > 0);
  const withSetup = sources.filter(
    (source) => source.setup && source.setup.state !== "unavailable",
  );

  return (
    <>
      <header className="page-header">
        <div className="page-title">
          <h1>Settings</h1>
        </div>
      </header>

      {failure && (
        <div className="banner" role="alert">
          <div>
            <strong>Pulse could not save that.</strong>
            <p>{failure}</p>
          </div>
        </div>
      )}

      <div className="settings">
        <ul className="group">
          <li className="setting">
            <div className="setting-text">
              <span>Tray icon stat</span>
              <span className="caption">The number on the tray icon and in its tooltip.</span>
            </div>
            <Picker
              label="Tray icon stat"
              value={statId(trayStat)}
              onChange={chooseTrayStat}
              groups={sources
                .filter(({ metrics }) => metrics.length > 0)
                .map(({ tab, metrics }) => ({
                  label: tab.name,
                  options: metrics
                    .filter((metric) => metric.unit !== "timestamp")
                    .map((metric) => ({
                      value: statId({ source_id: tab.source_id, key: metric.key }),
                      label: metric.label,
                    })),
                }))}
            />
          </li>
          <li className="setting">
            <div className="setting-text">
              <span>Open to</span>
              <span className="caption">The tab Pulse shows when it opens.</span>
            </div>
            <Picker
              label="Open to"
              value={defaultTab || tabs[0]?.source_id}
              onChange={chooseDefaultTab}
              groups={[
                { options: tabs.map((tab) => ({ value: tab.source_id, label: tab.name })) },
              ]}
            />
          </li>
          <ShortcutRecorder active={active} />
          <li className="setting">
            <div className="setting-text">
              <span>Keep Pulse ready</span>
              <span className="caption">
                Opens instantly, using about 120 MB more memory.
              </span>
            </div>
            <Switch label="Keep Pulse ready" on={keepReady} onChange={chooseKeepReady} />
          </li>
          <li className="setting">
            <div className="setting-text">
              <span>Start at login</span>
              <span className="caption">Pulse starts with Windows.</span>
            </div>
            <Switch label="Start at login" on={startAtLogin} onChange={chooseStartAtLogin} />
          </li>
          <li className="setting">
            <div className="setting-text">
              <span>Keep history</span>
              <span className="caption">How far back charts can reach.</span>
            </div>
            <Picker
              label="Keep history"
              value={String(historyDays)}
              onChange={chooseHistoryDays}
              groups={[{ options: HISTORY_LENGTHS }]}
            />
          </li>
          <UpdateRow />
        </ul>

        <TabOrder onChanged={onTabsChanged} />

        {withSetup.length > 0 && (
          <section>
            <h2 className="group-title">Connections</h2>
            <ul className="group">
              {withSetup.map(({ tab, setup }) => {
                const connected = setup?.state !== "disconnected";
                return (
                  <li className="setting" key={tab.source_id}>
                    <div className="setting-text">
                      <span>{tab.name}</span>
                      <span className="caption">{connected ? "Connected" : "Not connected"}</span>
                    </div>
                    {/* Connecting happens on the tab, where the terms are, never here. */}
                    <button
                      className="button"
                      onClick={() =>
                        connected ? disconnect(tab.source_id) : onOpenTab(tab.source_id)
                      }
                    >
                      {connected ? "Disconnect" : "Review"}
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        )}

        {withKeys.map(({ tab, keys }) => (
          <section key={tab.source_id}>
            <h2 className="group-title">{tab.name} API keys</h2>
            <ul className="group">
              {keys.map((status) => (
                <ApiKeyRow
                  key={status.name}
                  sourceId={tab.source_id}
                  status={status}
                  onSaved={loadSources}
                />
              ))}
            </ul>
          </section>
        ))}
      </div>
    </>
  );
}
