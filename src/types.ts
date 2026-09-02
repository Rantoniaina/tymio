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
  /** People on this project. */
  headcount: number;
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
  /** Everyone on the books, across every project. */
  people: number;
}

export interface Employee {
  id: string;
  projectId: string;
  firstName: string;
  lastName: string;
  role: string;
  email: string | null;
  phone: string | null;
  address: string | null;
  /** Digits only — spaces are stripped by the backend. */
  cin: string | null;
  birthDate: IsoDate | null;
  hireDate: IsoDate;
  bankAccount: string | null;
  emergencyContact: string | null;
  createdAt: IsoInstant;
  updatedAt: IsoInstant;
}

export interface EmployeeDraft {
  firstName: string;
  lastName: string;
  role: string;
  email: string | null;
  phone: string | null;
  address: string | null;
  cin: string | null;
  birthDate: IsoDate | null;
  hireDate: IsoDate;
  bankAccount: string | null;
  emergencyContact: string | null;
}

export interface EmployeeFilter {
  /** `null` lists everyone, across every project. */
  project: string | null;
  query: string | null;
}

export interface EmployeeStats {
  employeeId: string;
  projectId: string;
  asOf: IsoDate;
  month: YearMonth;
  age: number | null;
  monthsOfService: number;
  yearsOfService: number;
  monthsWorkedThisYear: number;
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

// ---------------------------------------------------------------- attendance

export type AttendanceSource = "schedule" | "manual";

export interface AttendanceEntry {
  id: string;
  employeeId: string;
  period: YearMonth;
  /** Half-days: 43 is 21.5 days. */
  daysWorked: number;
  /** Whole minutes. */
  hoursWorked: number;
  /** Whole minutes. */
  overtime: number;
  source: AttendanceSource;
  createdAt: IsoInstant;
  updatedAt: IsoInstant;
}

export interface AttendanceDraft {
  daysWorkedHalves: number;
  hoursWorkedMinutes: number;
  overtimeMinutes: number;
}

/** `entry` is absent for a month nobody has recorded — not a month of zero. */
export interface AttendanceRow {
  employeeId: string;
  entry: AttendanceEntry | null;
}

export interface AttendanceTotals {
  /** Half-days, like `AttendanceEntry.daysWorked`. */
  daysWorked: number;
  hoursWorkedMinutes: number;
  overtimeMinutes: number;
  recorded: number;
  missing: number;
}

export interface AttendanceSheet {
  projectId: string;
  period: YearMonth;
  rows: AttendanceRow[];
  totals: AttendanceTotals;
}
