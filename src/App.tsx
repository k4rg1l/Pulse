import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent,
} from "react";
import {
  clientReady,
  getSettings,
  getShortcut,
  listTabs,
  onPopoverClosing,
  onPopoverOpened,
  updateSetting,
  type Anchor,
  type Setting,
  type ShortcutStatus,
  type TabSummary,
} from "./api";
import { Settings } from "./Settings";
import { SourceTab } from "./SourceTab";
import { UpdateCard } from "./UpdateCard";
import { WelcomeCard } from "./WelcomeCard";

const SETTINGS = "settings";
const WELCOME_DISMISSED_SETTING = "welcome_dismissed";

const dismiss = () => getCurrentWindow().close();

/** The tab Pulse opens on: the one chosen in Settings, or else the first there is. */
function openingTab(tabs: TabSummary[], settings: Setting[]) {
  const preferred = settings.find((setting) => setting.key === "default_tab")?.value;
  const chosen = tabs.find((tab) => tab.source_id === preferred);
  return (chosen ?? tabs[0])?.source_id ?? SETTINGS;
}

/**
 * Tracks the panel's anchor and whether it should be showing. A new client is only
 * shown once `ready`, so the panel opens at its real height with its content in it.
 * `prepare` runs before each opening, while the panel is still invisible.
 */
function usePopover(ready: boolean, prepare: () => Promise<void>) {
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  const [open, setOpen] = useState(false);
  const listening = useRef<Promise<unknown>>(null);

  useEffect(() => {
    const opened = onPopoverOpened((next) => {
      setAnchor(next);
      prepare().then(() => requestAnimationFrame(() => setOpen(true)));
    });
    const closing = onPopoverClosing(() => setOpen(false));
    listening.current = Promise.all([opened, closing]);

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") dismiss();
    }
    window.addEventListener("keydown", onKeyDown);

    return () => {
      opened.then((stop) => stop());
      closing.then((stop) => stop());
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [prepare]);

  useEffect(() => {
    if (!ready) return;
    let current = true;
    // The window stays hidden until this call, so listeners come first.
    listening.current?.then(clientReady).then(({ anchor: next, shown }) => {
      if (!current || !shown) return;
      setAnchor(next);
      // A frame at the closed state first, so the opening has somewhere to start.
      requestAnimationFrame(() => requestAnimationFrame(() => setOpen(true)));
    });
    return () => {
      current = false;
    };
  }, [ready]);

  return { anchor, open };
}

function SettingsGlyph() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M2.5 5h6M12 5h1.5M2.5 11H4M7.5 11h6" />
      <circle cx="10.2" cy="5" r="1.7" />
      <circle cx="5.8" cy="11" r="1.7" />
    </svg>
  );
}

export default function App() {
  const [tabs, setTabs] = useState<TabSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [loaded, setLoaded] = useState<string[]>([]);
  const [welcome, setWelcome] = useState<ShortcutStatus | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const onLoaded = useCallback((sourceId: string) => {
    setLoaded((current) => (current.includes(sourceId) ? current : [...current, sourceId]));
  }, []);

  // A client that stays alive between openings would otherwise keep its last tab.
  const selectOpeningTab = useCallback(
    () =>
      Promise.all([listTabs(), getSettings()])
        .then(([tabs, settings]) => setSelected(openingTab(tabs, settings)))
        .catch((error) => setFailure(String(error))),
    [],
  );

  const ready =
    failure !== null || selected === SETTINGS || (selected !== null && loaded.includes(selected));
  const { anchor, open } = usePopover(ready, selectOpeningTab);

  useEffect(() => {
    Promise.all([listTabs(), getSettings(), getShortcut()])
      .then(([tabs, settings, shortcut]) => {
        const dismissed = settings.some((setting) => setting.key === WELCOME_DISMISSED_SETTING);
        setTabs(tabs);
        setWelcome(dismissed ? null : shortcut);
        setSelected(openingTab(tabs, settings));
      })
      .catch((error) => setFailure(String(error)));
  }, []);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: 0 });
  }, [selected]);

  function dismissWelcome() {
    setWelcome(null);
    updateSetting(WELCOME_DISMISSED_SETTING, true);
  }

  function onStagePress(event: MouseEvent) {
    // The margin around the panel belongs to this window, so a click there never
    // takes focus away. It still counts as a click outside the panel.
    if (event.target === event.currentTarget) dismiss();
  }

  const stageStyle = { "--notch": `${anchor?.offset ?? 222}px` } as CSSProperties;

  return (
    <div
      className="stage"
      data-side={anchor?.side ?? "bottom"}
      style={stageStyle}
      onMouseDown={onStagePress}
    >
      <div className={`panel${open ? " panel-open" : ""}`}>
        <span className="notch" aria-hidden="true" />
        <div className="panel-body">
          <nav className="topbar" aria-label="Sources">
            <div className="tabs">
              {tabs.map((tab) => (
                <button
                  key={tab.source_id}
                  className="tab"
                  aria-current={selected === tab.source_id ? "page" : undefined}
                  onClick={() => setSelected(tab.source_id)}
                >
                  {tab.name}
                </button>
              ))}
            </div>
            <button
              className="tab tab-icon"
              aria-label="Settings"
              title="Settings"
              aria-current={selected === SETTINGS ? "page" : undefined}
              onClick={() => setSelected(SETTINGS)}
            >
              <SettingsGlyph />
            </button>
          </nav>

          <div className="panel-scroll" ref={scrollRef}>
            <main className="content">
              {failure && <p className="notice">Pulse could not start: {failure}</p>}
              <UpdateCard />
              {welcome && <WelcomeCard shortcut={welcome} onDismiss={dismissWelcome} />}
              {tabs.map((tab) => (
                <div key={tab.source_id} hidden={selected !== tab.source_id}>
                  <SourceTab
                    sourceId={tab.source_id}
                    active={selected === tab.source_id}
                    onLoaded={onLoaded}
                  />
                </div>
              ))}
              <div hidden={selected !== SETTINGS}>
                <Settings tabs={tabs} active={selected === SETTINGS} onOpenTab={setSelected} />
              </div>
            </main>
          </div>
        </div>
      </div>
    </div>
  );
}
