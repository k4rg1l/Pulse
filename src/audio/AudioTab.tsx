import { Fit } from "../Fit";
import { Picker } from "../Picker";
import { Figure } from "../widgets/types";
import { LivingFader } from "./LivingFader";
import {
  useAudio,
  useHeldValue,
  useImageTints,
  useLiveFrame,
  type AudioApp,
  type AudioDevice,
  type Endpoint,
  type Track,
} from "./useAudio";

/** The colour of sound with no image to take one from. Matches --champagne. */
const RESTING_TINT = "#d8c29a";

/** Matches --warning. */
const MUTED_TINT = "#e3a56b";

/** Gives the master fader and the microphone shapes of their own. */
const MASTER_SEED = 5;
const MICROPHONE_SEED = 7;

const percent = (value: number) => `${Math.round(value * 100)}%`;

function clockTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
}

function TransportIcon({ path }: { path: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d={path} />
    </svg>
  );
}

const PREVIOUS = "M3.5 3h1.6v10H3.5zM13 3v10L6 8z";
const NEXT = "M11 3h1.6v10H11zM3.5 3v10l7-5z";
const PLAY = "M4.5 2.5v11l9-5.5z";
const PAUSE = "M4 3h3v10H4zM9 3h3v10H9z";

function NowPlaying({
  track,
  onCommand,
}: {
  track: Track;
  onCommand: (command: "previous" | "toggle" | "next") => void;
}) {
  const hasTimeline = track.duration > 0;
  return (
    <section className="playing">
      <div className="playing-art">{track.art && <img src={track.art} alt="" />}</div>
      <div className="playing-text">
        <h2>
          <Fit>{track.title || "Playing"}</Fit>
        </h2>
        <p className="caption">
          <Fit>{track.artist}</Fit>
        </p>
        {hasTimeline && (
          <p className="caption playing-time">
            <Fit>
              {clockTime(track.position)} of {clockTime(track.duration)}
            </Fit>
          </p>
        )}
      </div>
      <div className="playing-controls">
        <button className="transport" aria-label="Previous" onClick={() => onCommand("previous")}>
          <TransportIcon path={PREVIOUS} />
        </button>
        <button
          className="transport transport-main"
          aria-label={track.playing ? "Pause" : "Play"}
          onClick={() => onCommand("toggle")}
        >
          <TransportIcon path={track.playing ? PAUSE : PLAY} />
        </button>
        <button className="transport" aria-label="Next" onClick={() => onCommand("next")}>
          <TransportIcon path={NEXT} />
        </button>
      </div>
      {hasTimeline && (
        <div className="playing-progress" aria-hidden="true">
          <span style={{ width: `${(track.position / track.duration) * 100}%` }} />
        </div>
      )}
    </section>
  );
}

function AppRow({
  app,
  seed,
  tint,
  live,
  frame,
  onVolume,
  onMuted,
}: {
  app: AudioApp;
  seed: number;
  tint: string;
  live: boolean;
  frame: LiveFrameRef;
  onVolume: (value: number) => Promise<unknown>;
  onMuted: (muted: boolean) => void;
}) {
  const volume = useHeldValue(app.volume, onVolume);
  const sounding = app.playing && !app.muted;
  return (
    <li className={`mix-row${sounding ? " mix-row-sounding" : ""}`}>
      <button
        className="mix-icon"
        aria-label={`${app.muted ? "Unmute" : "Mute"} ${app.name}`}
        aria-pressed={app.muted}
        onClick={() => onMuted(!app.muted)}
      >
        {app.icon ? <img src={app.icon} alt="" /> : app.name.charAt(0)}
      </button>
      <div className="mix-body">
        <div className="mix-label">
          <Fit>{app.name}</Fit>
          <span className="caption">
            <Fit end>{app.muted ? "Muted" : percent(volume.value)}</Fit>
          </span>
        </div>
        <LivingFader
          label={`${app.name} volume`}
          value={volume.value}
          onChange={volume.change}
          onRelease={volume.release}
          sounding={sounding}
          moment={() => frame.current.apps.find((moment) => moment.id === app.id)}
          seed={seed}
          tint={tint}
          live={live}
        />
      </div>
    </li>
  );
}

type LiveFrameRef = ReturnType<typeof useLiveFrame>;

/** Windows adds the hardware in brackets, which only helps to tell two alike apart. */
function shortName(name: string, all: AudioDevice[]): string {
  const short = (full: string) => full.replace(/\s*\(.*\)$/, "");
  const alike = all.filter((device) => short(device.name) === short(name)).length;
  return alike > 1 ? name : short(name);
}

/** The devices of one flow as choices, with a line that says so when there are none. */
function DevicePicker({
  label,
  empty,
  endpoint,
  onChange,
}: {
  label: string;
  empty: string;
  endpoint: Endpoint | undefined;
  onChange: (id: string) => void;
}) {
  const devices = endpoint?.devices ?? [];
  const options = devices.map(({ id, name }) => ({ value: id, label: shortName(name, devices) }));
  return (
    <Picker
      label={label}
      value={endpoint?.current ?? ""}
      onChange={(id) => id && onChange(id)}
      groups={[{ options: options.length > 0 ? options : [{ value: "", label: empty }] }]}
    />
  );
}

/** Whether an app is the one playing this track. */
function isPlayedBy(track: Track | null | undefined, app: AudioApp): boolean {
  return Boolean(track?.playing) && app.id.endsWith(`\\${track?.app}.exe`);
}

/** Every sound on the computer, on one panel you can hold. */
export function AudioTab({ live, onLoaded }: { live: boolean; onLoaded: () => void }) {
  const { audio, failure, perform } = useAudio(onLoaded);
  const frame = useLiveFrame(live);
  const tintOf = useImageTints([audio?.track?.art, ...(audio?.apps ?? []).map((app) => app.icon)]);

  const output = useHeldValue(audio?.output.volume ?? 0, (value) =>
    perform({ action: "set_volume", flow: "output", value }),
  );
  const input = useHeldValue(audio?.input.volume ?? 0, (value) =>
    perform({ action: "set_volume", flow: "input", value }),
  );

  const track = audio?.track;
  const apps = audio?.apps ?? [];
  const microphoneMuted = audio?.input.muted ?? false;
  const soundingCount = apps.filter((app) => app.playing && !app.muted).length;
  const artTint = tintOf(track?.art);

  /** Each app wears its icon's colour, and the one playing music wears the album's. */
  const colourOf = (app: AudioApp) =>
    (isPlayedBy(track, app) ? artTint : null) ?? tintOf(app.icon) ?? RESTING_TINT;

  return (
    <>
      <header className="page-header">
        <div className="page-title">
          <h1>
            <Fit>Audio</Fit>
          </h1>
        </div>
        <div className="page-meta">
          <p className="caption" role={failure ? "alert" : undefined}>
            <Fit>
              {failure ??
                (soundingCount === 0
                  ? "Silent"
                  : `${soundingCount} ${soundingCount === 1 ? "app" : "apps"} playing`)}
            </Fit>
          </p>
        </div>
      </header>

      <section className="audio-hero">
        <div className="audio-hero-top">
          <p className="readout">
            <Fit>
              <Figure text={percent(output.value)} />
            </Fit>
          </p>
          <div className="audio-device">
            <DevicePicker
              label="Output"
              empty="No output"
              endpoint={audio?.output}
              onChange={(id) => perform({ action: "set_device", id })}
            />
          </div>
        </div>
        <LivingFader
          label="Volume"
          value={output.value}
          onChange={output.change}
          onRelease={output.release}
          sounding={!audio?.output.muted}
          moment={() => frame.current}
          seed={MASTER_SEED}
          tint={RESTING_TINT}
          blend={() =>
            apps
              .filter((app) => app.playing && !app.muted)
              .map((app) => ({
                color: colourOf(app),
                weight: frame.current.apps.find((moment) => moment.id === app.id)?.peak ?? 0,
              }))
              .filter((part) => part.weight > 0)
          }
          live={live}
          large
        />
      </section>

      {track && (
        <NowPlaying
          track={track}
          onCommand={(command) => perform({ action: "transport", command })}
        />
      )}

      {apps.length > 0 && (
        <ul className="mix">
          {apps.map((app, index) => (
            <AppRow
              key={app.id}
              app={app}
              seed={index + 1}
              tint={colourOf(app)}
              live={live}
              frame={frame}
              onVolume={(value) => perform({ action: "set_app_volume", app: app.id, value })}
              onMuted={(muted) => perform({ action: "set_app_muted", app: app.id, muted })}
            />
          ))}
        </ul>
      )}

      <section className="microphone">
        <h2>Microphone</h2>
        <div className="microphone-controls">
          <DevicePicker
            label="Input"
            empty="No microphone"
            endpoint={audio?.input}
            onChange={(id) => perform({ action: "set_device", id })}
          />
          <button
            className={`button${microphoneMuted ? " microphone-muted" : ""}`}
            aria-pressed={microphoneMuted}
            disabled={!audio?.input.current}
            onClick={() =>
              perform({ action: "set_muted", flow: "input", muted: !microphoneMuted })
            }
          >
            {microphoneMuted ? "Muted" : "Mute"}
          </button>
        </div>
        <LivingFader
          label="Microphone level"
          value={input.value}
          onChange={input.change}
          onRelease={input.release}
          sounding={!microphoneMuted}
          moment={() => ({ bands: null, peak: frame.current.input })}
          seed={MICROPHONE_SEED}
          tint={microphoneMuted ? MUTED_TINT : RESTING_TINT}
          live={live}
        />
      </section>
    </>
  );
}
