import { useState, type ReactNode } from "react";
import { connectSource, type Setup, type Tab, type Terms } from "./api";

function CheckGlyph() {
  return (
    <svg className="terms-glyph terms-glyph-will" viewBox="0 0 12 12" aria-hidden="true">
      <path d="M2 6.5 4.8 9.2 10 3" />
    </svg>
  );
}

function CrossGlyph() {
  return (
    <svg className="terms-glyph" viewBox="0 0 12 12" aria-hidden="true">
      <path d="M3 3l6 6M9 3l-6 6" />
    </svg>
  );
}

/** One side of the terms: what Pulse will do, or what it never will. */
function TermsList({ label, items, glyph }: { label: string; items: string[]; glyph: ReactNode }) {
  return (
    <section className="terms-group">
      <h3 className="note">{label}</h3>
      <ul>
        {items.map((item) => (
          <li className="terms-item" key={item}>
            {glyph}
            <span>{item}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/**
 * Everything a person agrees to, set out before they agree to it. All sources
 * share this sheet, so connecting anything reads and feels the same.
 */
function ConnectSheet({ tab, setup, terms }: { tab: Tab; setup: Setup; terms: Terms }) {
  const [failure, setFailure] = useState<string | null>(null);

  function connect() {
    connectSource(tab.source_id)
      .then(() => setFailure(null))
      .catch((error) => setFailure(String(error)));
  }

  return (
    <article className="connect-sheet">
      <header>
        <h2>{setup.title}</h2>
        <p>{setup.detail}</p>
      </header>
      <TermsList label="With your permission, Pulse will" items={terms.will} glyph={<CheckGlyph />} />
      <TermsList label="Pulse will never" items={terms.never} glyph={<CrossGlyph />} />
      <p className="connect-note" role={failure ? "alert" : undefined}>
        {failure ?? terms.note}
      </p>
      <button className="button button-primary" onClick={connect}>
        {setup.action}
      </button>
    </article>
  );
}

/** The step a source needs from the user, if it needs one. */
export function SetupCard({ tab }: { tab: Tab }) {
  const setup = tab.setup;
  if (!setup || setup.state === "connected") return null;
  if (setup.terms) return <ConnectSheet tab={tab} setup={setup} terms={setup.terms} />;

  return (
    <div className="setup-card">
      <div>
        <h2>{setup.title}</h2>
        <p className="caption">{setup.detail}</p>
      </div>
    </div>
  );
}
