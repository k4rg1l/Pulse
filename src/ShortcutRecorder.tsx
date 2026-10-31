import { useCallback, useEffect, useRef, useState } from "react";
import { getShortcut, pauseShortcut, setShortcut, type ShortcutStatus } from "./api";
import { Fit } from "./Fit";

const PUNCTUATION: Record<string, string> = {
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

const MODIFIER_KEYS = ["Control", "Alt", "Shift", "Meta"];

/** The key as printed on a keyboard, from its saved name. */
function keycapLabel(part: string): string {
  if (part === "Super") return "Win";
  return PUNCTUATION[part] ?? part.replace(/^(Key|Digit)/, "");
}

/** The modifiers held during an event, in the order a shortcut is written. */
function heldModifiers(event: KeyboardEvent): string[] {
  return [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Super",
  ].filter((part) => part !== false);
}

/** Why a key press can't be a shortcut, or null if it can. */
function rejectionOf(event: KeyboardEvent): string | null {
  if (/^F\d+$/.test(event.code)) return null;
  if (!event.ctrlKey && !event.altKey && !event.metaKey) return "Include Ctrl or Alt";
  // Ctrl plus one key is how every app copies, pastes and saves.
  if (heldModifiers(event).length === 1 && event.ctrlKey) return "Apps need that one. Add Alt";
  return null;
}

function Keycaps({ parts }: { parts: string[] }) {
  return (
    <span className="keycaps">
      {parts.map((part) => (
        <kbd className="keycap" key={part}>
          {keycapLabel(part)}
        </kbd>
      ))}
    </span>
  );
}

/**
 * Records a system-wide shortcut in place: press the keycaps, then press the new
 * shortcut. Nothing opens over the settings, and nothing else on the row moves.
 */
export function ShortcutRecorder({ active }: { active: boolean }) {
  const [status, setStatus] = useState<ShortcutStatus | null>(null);
  const [recording, setRecording] = useState(false);
  const [held, setHeld] = useState<string[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [saves, setSaves] = useState(0);
  const recordingRef = useRef(false);

  useEffect(() => {
    if (active) getShortcut().then(setStatus);
  }, [active]);

  const stopRecording = useCallback(() => {
    recordingRef.current = false;
    setRecording(false);
    setHeld([]);
    setProblem(null);
  }, []);

  const cancel = useCallback(() => {
    if (!recordingRef.current) return;
    stopRecording();
    pauseShortcut(false).then(getShortcut).then(setStatus);
  }, [stopRecording]);

  const save = useCallback(
    (shortcut: string | null) => {
      setShortcut(shortcut)
        .then((saved) => {
          setStatus(saved);
          setSaves((count) => count + 1);
          stopRecording();
        })
        .catch((error) => setProblem(String(error)));
    },
    [stopRecording],
  );

  useEffect(() => {
    if (!recording) return;

    function onKeyDown(event: KeyboardEvent) {
      // Capturing here also keeps Escape from dismissing the popover.
      event.preventDefault();
      event.stopPropagation();
      if (event.repeat) return;

      const bare = heldModifiers(event).length === 0;
      if (bare && event.key === "Escape") return cancel();
      if (bare && (event.key === "Backspace" || event.key === "Delete")) return save(null);

      setHeld(heldModifiers(event));
      if (MODIFIER_KEYS.includes(event.key)) return;

      const rejection = rejectionOf(event);
      setProblem(rejection);
      if (!rejection) save([...heldModifiers(event), event.code].join("+"));
    }

    function onKeyUp(event: KeyboardEvent) {
      event.preventDefault();
      setHeld(heldModifiers(event));
    }

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", cancel);
    };
  }, [recording, cancel, save]);

  useEffect(() => {
    if (!active) cancel();
  }, [active, cancel]);

  function start() {
    pauseShortcut(true).then(() => {
      recordingRef.current = true;
      setRecording(true);
    });
  }

  const shortcut = status?.shortcut ?? null;
  const unavailable = !recording && shortcut !== null && status?.registered === false;
  const caption =
    problem ??
    (recording
      ? "Esc cancels, Backspace clears"
      : unavailable
        ? "Another app is using this one"
        : shortcut
          ? "Opens Pulse from anywhere."
          : "Press to choose a shortcut.");
  const shown = recording ? held : (shortcut?.split("+") ?? []);

  return (
    <li className="setting setting-shortcut">
      <div className="setting-text">
        <span>Open Pulse</span>
        <span
          className={problem || unavailable ? "caption caption-problem" : "caption"}
          aria-live="polite"
        >
          <Fit>{caption}</Fit>
        </span>
      </div>
      <button
        className="shortcut"
        data-recording={recording || undefined}
        aria-label={
          recording
            ? "Recording. Press a new shortcut."
            : `Change shortcut. Currently ${shown.map(keycapLabel).join(" ") || "off"}.`
        }
        onClick={recording ? cancel : start}
      >
        <Fit end>
          {shown.length > 0 ? (
            <span key={saves} className={saves > 0 ? "keycaps-saved" : undefined}>
              <Keycaps parts={shown} />
            </span>
          ) : (
            <span className="note">{recording ? "Press keys" : "Off"}</span>
          )}
        </Fit>
      </button>
    </li>
  );
}
