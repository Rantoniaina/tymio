/**
 * Display formatting. Malagasy locale conventions throughout: dates as
 * dd/mm/yyyy, thousands separated by spaces.
 */

import type { IsoDate, IsoInstant, WorkCalendar, YearMonth } from "./types";

/** The em dash the mockup uses wherever a value is absent. */
export const ABSENT = "—";

/** `2026-02-01` → `01/02/2026`. */
export function formatDate(date: IsoDate | null | undefined): string {
  if (!date) return ABSENT;
  const [year, month, day] = date.split("-");
  if (!year || !month || !day) return date;
  return `${day}/${month}/${year}`;
}

/** An instant as `01/02/2026 14:05`, in the reader's own timezone. */
export function formatInstant(instant: IsoInstant): string {
  const at = new Date(instant);
  if (Number.isNaN(at.getTime())) return instant;
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${pad(at.getDate())}/${pad(at.getMonth() + 1)}/${at.getFullYear()} ` +
    `${pad(at.getHours())}:${pad(at.getMinutes())}`
  );
}

/** `3200000` → `3 200 000` — space-separated, as the mockup writes Ariary. */
export function formatNumber(value: number): string {
  return Math.round(value).toLocaleString("en-US").replace(/,/g, " ");
}

/** Whole minutes → `8 h` or `7 h 30`. Mirrors `DayLength::Display` in Rust. */
export function formatDayLength(minutes: number): string {
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours} h` : `${hours} h ${String(rest).padStart(2, "0")}`;
}

/** Monday first, matching the bit order of the mask. */
export const WEEKDAYS = [
  { bit: 0, short: "Mon", long: "Monday" },
  { bit: 1, short: "Tue", long: "Tuesday" },
  { bit: 2, short: "Wed", long: "Wednesday" },
  { bit: 3, short: "Thu", long: "Thursday" },
  { bit: 4, short: "Fri", long: "Friday" },
  { bit: 5, short: "Sat", long: "Saturday" },
  { bit: 6, short: "Sun", long: "Sunday" },
] as const;

export function worksOn(mask: number, bit: number): boolean {
  return (mask & (1 << bit)) !== 0;
}

export function toggleWeekday(mask: number, bit: number): number {
  return mask ^ (1 << bit);
}

/** `Mon–Fri` when the run is contiguous, otherwise `Mon, Wed, Sat`. */
export function formatWorkingDays(mask: number): string {
  const on = WEEKDAYS.filter((d) => worksOn(mask, d.bit));
  if (on.length === 0) return ABSENT;
  if (on.length === 7) return "Every day";

  const contiguous = on.every((day, index) => index === 0 || day.bit === on[index - 1].bit + 1);
  if (contiguous && on.length > 2) return `${on[0].short}–${on[on.length - 1].short}`;
  return on.map((d) => d.short).join(", ");
}

export function describeCalendar(calendar: WorkCalendar): string {
  return `${formatWorkingDays(calendar.workingDays)} · ${formatDayLength(calendar.dayLength)}`;
}

/** Today as a civil date in the reader's timezone, not UTC. */
export function today(): IsoDate {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** `{ year: 2026, month: 9 }` → `September 2026`. */
export function formatMonth(month: YearMonth): string {
  return `${MONTH_NAMES[month.month - 1]} ${month.year}`;
}

/** The `YYYY-MM` form, used as a select value. */
export function monthKey(month: YearMonth): string {
  return `${month.year}-${String(month.month).padStart(2, "0")}`;
}

export function parseMonthKey(key: string): YearMonth {
  const [year, month] = key.split("-");
  return { year: Number(year), month: Number(month) };
}

export function thisMonth(): YearMonth {
  const now = new Date();
  return { year: now.getFullYear(), month: now.getMonth() + 1 };
}

/** The last `count` months, most recent first. */
export function recentMonths(count = 12): YearMonth[] {
  const now = new Date();
  return Array.from({ length: count }, (_, back) => {
    const at = new Date(now.getFullYear(), now.getMonth() - back, 1);
    return { year: at.getFullYear(), month: at.getMonth() + 1 };
  });
}

/**
 * The date to ask the backend about for a chosen period: the end of that
 * month, except for the current one, where the future has not happened yet.
 */
export function asOfFor(month: YearMonth): IsoDate {
  const now = thisMonth();
  if (month.year === now.year && month.month === now.month) return today();
  const last = new Date(month.year, month.month, 0);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${last.getFullYear()}-${pad(last.getMonth() + 1)}-${pad(last.getDate())}`;
}

/** `18` → `18 years`, with the singular handled. */
export function pluralise(count: number, unit: string): string {
  return `${count} ${unit}${count === 1 ? "" : "s"}`;
}

/** Whole months as the employee file writes them: `1 year 7 months`. */
export function formatService(months: number): string {
  if (months < 12) return pluralise(months, "month");
  const years = Math.floor(months / 12);
  const rest = months % 12;
  return rest === 0
    ? pluralise(years, "year")
    : `${pluralise(years, "year")} ${pluralise(rest, "month")}`;
}

/**
 * The mockup's avatar palette, picked from a stable hash so one person keeps
 * one colour.
 */
const AVATAR_COLOURS = [
  "oklch(0.52 0.10 175)",
  "oklch(0.52 0.10 60)",
  "oklch(0.52 0.10 320)",
  "oklch(0.52 0.10 250)",
];

export function avatarColour(seed: string): string {
  let hash = 0;
  for (const character of seed) hash = (hash * 31 + character.codePointAt(0)!) >>> 0;
  return AVATAR_COLOURS[hash % AVATAR_COLOURS.length];
}

/* ---------------------------------------------------------- attendance units */

/** Half-days → the decimal the grid shows: `43` → `21.5`, `44` → `22`. */
export function daysFromHalves(halves: number): string {
  return halves % 2 === 0 ? String(halves / 2) : (halves / 2).toFixed(1);
}

/** The grid's decimal back into half-days, rounded to the nearest half. */
export function halvesFromDays(days: number): number {
  return Math.round(days * 2);
}

/**
 * Minutes → decimal hours for an input box. Two decimals is enough for
 * quarter-hours, which is as fine as the seeded values ever get.
 */
export function hoursFromMinutes(minutes: number): string {
  const hours = minutes / 60;
  return Number.isInteger(hours) ? String(hours) : hours.toFixed(2).replace(/0+$/, "");
}

export function minutesFromHours(hours: number): number {
  return Math.round(hours * 60);
}

/** Minutes as the totals row writes them: `176 h`, `161 h 15`. */
export function formatMinutes(minutes: number): string {
  const whole = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${whole} h` : `${whole} h ${String(rest).padStart(2, "0")}`;
}
