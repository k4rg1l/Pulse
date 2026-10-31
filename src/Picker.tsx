import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { Fit } from "./Fit";

export interface PickerOption {
  value: string;
  label: string;
}

export interface PickerGroup {
  label?: string;
  options: PickerOption[];
}

interface PickerProps {
  /** Names the control for assistive technology. */
  label: string;
  value: string;
  groups: PickerGroup[];
  onChange: (value: string) => void;
}

/** A dropdown drawn in the panel, since a native one is styled by Windows. */
export function Picker({ label, value, groups, onChange }: PickerProps) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const baseId = useId();

  const options = groups.flatMap((group) => group.options);
  const selected = options.find((option) => option.value === value);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(event: PointerEvent) {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    }
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  function show() {
    setActive(Math.max(0, options.findIndex((option) => option.value === value)));
    setOpen(true);
  }

  function choose(option: PickerOption | undefined) {
    if (option) onChange(option.value);
    setOpen(false);
    trigger.current?.focus();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (!open) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) {
        event.preventDefault();
        show();
      }
      return;
    }

    const last = options.length - 1;
    switch (event.key) {
      case "ArrowDown":
        setActive(Math.min(last, active + 1));
        break;
      case "ArrowUp":
        setActive(Math.max(0, active - 1));
        break;
      case "Home":
        setActive(0);
        break;
      case "End":
        setActive(last);
        break;
      case "Enter":
      case " ":
        choose(options[active]);
        break;
      case "Escape":
        // Closes the list only. The next Escape reaches the popover.
        event.stopPropagation();
        setOpen(false);
        break;
      case "Tab":
        setOpen(false);
        return;
      default:
        return;
    }
    event.preventDefault();
  }

  let index = -1;

  return (
    <div className="picker" ref={root} onKeyDown={onKeyDown}>
      <button
        ref={trigger}
        className="picker-trigger"
        role="combobox"
        aria-label={label}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-controls={`${baseId}-list`}
        aria-activedescendant={open ? `${baseId}-${active}` : undefined}
        onClick={() => (open ? setOpen(false) : show())}
      >
        <Fit>{selected?.label}</Fit>
        <svg viewBox="0 0 10 6" aria-hidden="true">
          <path d="M1 1l4 4 4-4" />
        </svg>
      </button>

      {open && (
        <ul className="picker-menu" id={`${baseId}-list`} role="listbox" aria-label={label}>
          {groups.map((group, groupIndex) => (
            <li key={group.label ?? groupIndex} role="presentation">
              {group.label && <p className="picker-group">{group.label}</p>}
              <ul role="group" aria-label={group.label}>
                {group.options.map((option) => {
                  index += 1;
                  const position = index;
                  return (
                    <li
                      key={option.value}
                      id={`${baseId}-${position}`}
                      className="picker-option"
                      role="option"
                      aria-selected={option.value === value}
                      data-active={position === active || undefined}
                      onPointerEnter={() => setActive(position)}
                      onClick={() => choose(option)}
                    >
                      {option.label}
                      {option.value === value && (
                        <svg viewBox="0 0 12 10" aria-hidden="true">
                          <path d="M1 5.2l3.4 3.3L11 1.2" />
                        </svg>
                      )}
                    </li>
                  );
                })}
              </ul>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
