import { useEffect, useState } from "react";
import {
  getApiKeyStatus,
  getSettings,
  getTab,
  setApiKey,
  updateSetting,
  type ApiKeyStatus,
  type Metric,
  type TabSummary,
} from "./api";
import { Picker } from "./Picker";

interface TrayStat {
  source_id: string;
  key: string;
}

interface SourceOptions {
  tab: TabSummary;
  metrics: Metric[];
  keys: ApiKeyStatus[];
}

const DEFAULT_TRAY_STAT: TrayStat = { source_id: "system", key: "cpu.usage" };

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

export function Settings({ tabs }: { tabs: TabSummary[] }) {
  const [sources, setSources] = useState<SourceOptions[]>([]);
  const [trayStat, setTrayStat] = useState<TrayStat>(DEFAULT_TRAY_STAT);
  const [defaultTab, setDefaultTab] = useState<string>("");
  const [failure, setFailure] = useState<string | null>(null);

  function loadSources() {
    Promise.all(
      tabs.map(async (tab) => ({
        tab,
        metrics: (await getTab(tab.source_id)).metrics,
        keys: await getApiKeyStatus(tab.source_id),
      })),
    )
      .then(setSources)
      .catch((error) => setFailure(String(error)));
  }

  useEffect(() => {
    loadSources();
    getSettings()
      .then((settings) => {
        for (const { key, value } of settings) {
          if (key === "tray_stat") setTrayStat(value as TrayStat);
          if (key === "default_tab") setDefaultTab(value as string);
        }
      })
      .catch((error) => setFailure(String(error)));
    // Tabs are fixed for the life of the client.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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

  const withKeys = sources.filter((source) => source.keys.length > 0);

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
              groups={sources.map(({ tab, metrics }) => ({
                label: tab.name,
                options: metrics.map((metric) => ({
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
        </ul>

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
