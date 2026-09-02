/**
 * Display formatting. Malagasy locale conventions throughout: dates as
 * dd/mm/yyyy, thousands separated by spaces.
 */

import type { IsoDate, IsoInstant, WorkCalendar } from "./types";

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
