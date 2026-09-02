/**
 * The shapes the Rust commands send and receive.
 *
 * These mirror `src-tauri/src/domain/` by hand. The Rust structs carry
 * `#[serde(rename_all = "camelCase")]` so the names below are the wire names;
 * `tauri-specta` is the intended way to generate this file rather than write
 * it, and is still to be wired up.
 */

/** A civil date, `YYYY-MM-DD`. Never a timestamp. */
export type IsoDate = string;

/** An RFC 3339 instant in UTC. */
export type IsoInstant = string;

export type ProjectStatus = "active" | "paused" | "closed";

export const PROJECT_STATUSES: ProjectStatus[] = ["active", "paused", "closed"];

export const STATUS_LABELS: Record<ProjectStatus, string> = {
  active: "Active",
  paused: "Paused",
  closed: "Closed",
};

/** Bit 0 is Monday through bit 6 for Sunday. */
export interface WorkCalendar {
  workingDays: number;
  /** The standard day, in whole minutes. */
  dayLength: number;
}

export interface Project {
  id: string;
  name: string;
  client: string | null;
  location: string | null;
  status: ProjectStatus;
  start: IsoDate;
  /** Absent for an open-ended project. */
  end: IsoDate | null;
  calendar: WorkCalendar;
  createdAt: IsoInstant;
  updatedAt: IsoInstant;
}

/** What the new/edit form submits. Validated in Rust, never here alone. */
export interface ProjectDraft {
  name: string;
  client: string | null;
  location: string | null;
  status: ProjectStatus;
  start: IsoDate;
  end: IsoDate | null;
  workingDays: number;
  dayLength: number;
}

export interface Holiday {
  id: string;
  projectId: string;
  date: IsoDate;
  name: string;
}

export interface HolidayDraft {
  date: IsoDate;
  name: string;
}

export interface ProjectFilter {
  status: ProjectStatus | null;
  query: string | null;
}

export interface YearMonth {
  year: number;
  month: number;
}

export interface DurationProgress {
  start: IsoDate;
  end: IsoDate | null;
  totalDays: number | null;
  elapsedDays: number;
  remainingDays: number | null;
  /** 0–100, or null for an open-ended project. */
  percentElapsed: number | null;
}

export interface ProjectStats {
  projectId: string;
  status: ProjectStatus;
  asOf: IsoDate;
  month: YearMonth;
  duration: DurationProgress;
  holidayCount: number;
  workingDaysThisMonth: number;
  workingMinutesThisMonth: number;
}

export interface PortfolioStats {
  total: number;
  active: number;
  paused: number;
  closed: number;
}

export type AuditAction = "create" | "update" | "delete";

export interface AuditEntry {
  id: number;
  at: IsoInstant;
  entity: string;
  entityId: string;
  action: AuditAction;
  /** A JSON snapshot of the record after the change (before it, for a delete). */
  detail: string | null;
}
