import type { ShortcutStatus } from "./api";
import { Keycaps } from "./ShortcutRecorder";

/**
 * Tells a new user where Pulse went, since a tray app has no window to come back
 * to. It stays until dismissed and never returns.
 */
export function WelcomeCard({
  shortcut,
  onDismiss,
}: {
  shortcut: ShortcutStatus;
  onDismiss: () => void;
}) {
  const parts = shortcut.registered ? shortcut.shortcut?.split("+") : undefined;

  return (
    <aside className="welcome">
      <div>
        <h2>Pulse is in your tray.</h2>
        <p className="caption welcome-how">
          {parts ? (
            <>
              Press <Keycaps parts={parts} /> anytime to open it.
            </>
          ) : (
            "Click its icon anytime to open it."
          )}
        </p>
      </div>
      <button className="icon-button" aria-label="Dismiss" onClick={onDismiss}>
        <svg viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
        </svg>
      </button>
    </aside>
  );
}
