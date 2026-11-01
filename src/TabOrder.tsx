import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { getTabChoices, setTabChoices, type TabChoice } from "./api";
import { Switch } from "./Switch";

function moved(choices: TabChoice[], from: number, to: number): TabChoice[] {
  const next = [...choices];
  const [choice] = next.splice(from, 1);
  next.splice(to, 0, choice);
  return next;
}

function GripGlyph() {
  return (
    <svg viewBox="0 0 10 16" fill="currentColor" aria-hidden="true">
      {[3, 8, 13].map((y) => (
        <g key={y}>
          <circle cx="3" cy={y} r="1.1" />
          <circle cx="7" cy={y} r="1.1" />
        </g>
      ))}
    </svg>
  );
}

/**
 * The one place that decides which tabs exist and their order. Drag a handle to
 * move a tab, or focus it and use the arrow keys. Each change applies at once.
 */
export function TabOrder({ onChanged }: { onChanged: () => void }) {
  const [choices, setChoices] = useState<TabChoice[]>([]);
  const [dragging, setDragging] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const list = useRef<HTMLUListElement>(null);

  useEffect(() => {
    getTabChoices()
      .then(setChoices)
      .catch((error) => setFailure(String(error)));
  }, []);

  function save(next: TabChoice[]) {
    setChoices(next);
    setTabChoices(next)
      .then(() => {
        setFailure(null);
        onChanged();
      })
      .catch((error) => setFailure(String(error)));
  }

  function press(event: PointerEvent<HTMLButtonElement>, sourceId: string) {
    event.currentTarget.setPointerCapture(event.pointerId);
    setDragging(sourceId);
  }

  /** Swaps with a neighbour once the pointer passes the middle of that neighbour. */
  function drag(event: PointerEvent<HTMLButtonElement>) {
    if (!dragging || !list.current) return;
    const from = choices.findIndex((choice) => choice.source_id === dragging);
    const rows = [...list.current.children];
    const to = rows.findIndex((row, index) => {
      const box = row.getBoundingClientRect();
      const middle = box.top + box.height / 2;
      if (index > from) return event.clientY > middle && index === from + 1;
      if (index < from) return event.clientY < middle && index === from - 1;
      return false;
    });
    if (to >= 0) setChoices(moved(choices, from, to));
  }

  function release() {
    if (!dragging) return;
    setDragging(null);
    save(choices);
  }

  function nudge(event: KeyboardEvent, index: number) {
    const offset = { ArrowUp: -1, ArrowDown: 1 }[event.key];
    if (!offset) return;
    event.preventDefault();
    const to = index + offset;
    if (to >= 0 && to < choices.length) save(moved(choices, index, to));
  }

  const shownCount = choices.filter((choice) => choice.shown).length;

  return (
    <section>
      <h2 className="group-title">Tabs</h2>
      <ul className="group tab-order" ref={list}>
        {choices.map((choice, index) => (
          <li
            className={`setting tab-choice${dragging === choice.source_id ? " tab-choice-moving" : ""}`}
            key={choice.source_id}
          >
            <button
              className="tab-grip"
              aria-label={`Move ${choice.name}, position ${index + 1} of ${choices.length}`}
              onPointerDown={(event) => press(event, choice.source_id)}
              onPointerMove={drag}
              onPointerUp={release}
              onPointerCancel={release}
              onKeyDown={(event) => nudge(event, index)}
            >
              <GripGlyph />
            </button>
            <span className={choice.shown ? undefined : "muted"}>{choice.name}</span>
            <Switch
              label={`Show ${choice.name}`}
              on={choice.shown}
              disabled={choice.shown && shownCount === 1}
              onChange={(shown) =>
                save(
                  choices.map((other) =>
                    other.source_id === choice.source_id ? { ...other, shown } : other,
                  ),
                )
              }
            />
          </li>
        ))}
      </ul>
      {failure && (
        <p className="caption tab-order-note" role="alert">
          {failure}
        </p>
      )}
    </section>
  );
}
