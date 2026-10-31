import { useState } from "react";
import { Fit } from "../Fit";
import { formatCount } from "../format";
import { Note, rawValue, type WidgetProps } from "./types";

const WEEKS = 26;
const DAYS_IN_WEEK = 7;
const LEVELS = 4;

/** Keeps two month names from running together at the left edge. */
const FIRST_MONTH_LABEL_ROOM = 2;

interface DayValue {
  date: string;
  value: number;
}

interface CalendarDay {
  date: Date;
  value: number;
  isFuture: boolean;
}

/** The date as the source writes it, in local time. */
function isoDate(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/** Every day of the last `WEEKS` weeks, as whole weeks ending with this one. */
function calendarDays(values: DayValue[], today: Date): CalendarDay[] {
  const valueByDate = new Map(values.map((day) => [day.date, day.value]));
  const first = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  first.setDate(first.getDate() - first.getDay() - (WEEKS - 1) * DAYS_IN_WEEK);

  return Array.from({ length: WEEKS * DAYS_IN_WEEK }, (_, index) => {
    const date = new Date(first);
    date.setDate(first.getDate() + index);
    return { date, value: valueByDate.get(isoDate(date)) ?? 0, isFuture: date > today };
  });
}

/**
 * Busy days can be a thousand times larger than quiet ones, so levels are
 * spaced by order of magnitude. A straight scale would leave most days blank.
 */
function levelScale(days: CalendarDay[]) {
  const active = days.map((day) => day.value).filter((value) => value > 0);
  const low = Math.log(Math.min(...active));
  const high = Math.log(Math.max(...active));
  return (value: number) => {
    if (value <= 0) return 0;
    if (high === low) return LEVELS;
    return Math.min(LEVELS, 1 + Math.floor(((Math.log(value) - low) / (high - low)) * LEVELS));
  };
}

/** The first week each month appears in, for the labels above the grid. */
function monthLabels(days: CalendarDay[]) {
  const labels: { week: number; name: string }[] = [];
  for (let week = 0; week < WEEKS; week += 1) {
    const { date } = days[week * DAYS_IN_WEEK];
    const previous = days[(week - 1) * DAYS_IN_WEEK]?.date;
    if (previous?.getMonth() === date.getMonth()) continue;
    labels.push({ week, name: date.toLocaleDateString([], { month: "short" }) });
  }
  const crowded = labels.length > 1 && labels[1].week <= FIRST_MONTH_LABEL_ROOM;
  return crowded ? labels.slice(1) : labels;
}

/** One day in a few words, for the line that follows the pointer. */
function describeDay({ date, value }: CalendarDay): string {
  const name = date.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" });
  return `${name}: ${value > 0 ? formatCount(value) : "none"}`;
}

/** Half a year at a glance. Pointing at a day names it, so nothing floats over the grid. */
export function Calendar({ widget, tab }: WidgetProps) {
  const [hovered, setHovered] = useState<number | null>(null);
  const raw = rawValue(tab, widget.keys[0]);
  const today = new Date();
  const days = calendarDays(Array.isArray(raw) ? (raw as DayValue[]) : [], today);
  const level = levelScale(days);
  const activeDays = days.filter((day) => day.value > 0).length;
  const todayIso = isoDate(today);

  return (
    <>
      <header className="split">
        <h2>
          <Fit>{widget.title}</Fit>
        </h2>
        <Note
          text={hovered === null ? `${activeDays} active days` : describeDay(days[hovered])}
        />
      </header>
      <div className="calendar-months" aria-hidden="true">
        {monthLabels(days).map(({ week, name }) => (
          <span key={week} style={{ gridColumnStart: week + 1 }}>
            {name}
          </span>
        ))}
      </div>
      <div
        className="calendar"
        role="img"
        aria-label={`${activeDays} active days in the last ${WEEKS} weeks`}
        onPointerLeave={() => setHovered(null)}
      >
        {days.map((day, index) =>
          day.isFuture ? (
            <span key={index} />
          ) : (
            <span
              key={index}
              className="calendar-day"
              data-level={level(day.value)}
              data-today={isoDate(day.date) === todayIso || undefined}
              onPointerEnter={() => setHovered(index)}
            />
          ),
        )}
      </div>
    </>
  );
}
