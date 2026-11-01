import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { Fit } from "./Fit";

/** Space between the trigger and its list, and between the list and the panel's edge. */
const MENU_GAP = 6;

/** Floats the list beside its trigger, on whichever side the panel has room for it. */
function placeMenu(menu: HTMLElement, trigger: HTMLElement) {
  const button = trigger.getBoundingClientRect();
  const panel = trigger.closest(".panel-body")?.getBoundingClientRect();
  const top = panel?.top ?? 0;
  const bottom = panel?.bottom ?? window.innerHeight;
  const below = bottom - button.bottom - MENU_GAP * 2;
  const above = button.top - top - MENU_GAP * 2;

  menu.style.left = `${button.left}px`;
  menu.style.width = `${button.width}px`;
  const fitsBelow = menu.scrollHeight <= below || below >= above;
  menu.style.maxHeight = `${fitsBelow ? below : above}px`;
  menu.style.transformOrigin = fitsBelow ? "100% 0" : "100% 100%";
  menu.style.top = fitsBelow
    ? `${button.bottom + MENU_GAP}px`
    : `${button.top - MENU_GAP - menu.offsetHeight}px`;
}

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
  const menu = useRef<HTMLUListElement>(null);
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

  useLayoutEffect(() => {
    if (!open || !menu.current || !trigger.current) return;
    const list = menu.current;
    // The top layer lifts the list out of the page, so opening it moves nothing.
    list.showPopover();
    placeMenu(list, trigger.current);

    function onScroll(event: Event) {
      if (event.target !== list) setOpen(false);
    }
    const close = () => setOpen(false);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("blur", close);
    };
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
        <ul
          ref={menu}
          className="picker-menu"
          id={`${baseId}-list`}
          popover="manual"
          role="listbox"
          aria-label={label}
        >
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
