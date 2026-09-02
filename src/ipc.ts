/**
 * The only place the front end talks to Rust.
 *
 * Every command comes back either with its value or with the `AppError` shape
 * that `src-tauri/src/error.rs` serialises: `{ kind, message, fields }`. The
 * wrapper turns the second into a thrown `AppError` so callers can use
 * try/catch and read per-field messages off it.
 */

import { invoke } from "@tauri-apps/api/core";

import type {
  AuditEntry,
  Employee,
  EmployeeDraft,
  EmployeeFilter,
  EmployeeStats,
  Holiday,
  HolidayDraft,
  PortfolioStats,
  Project,
  ProjectDraft,
  ProjectFilter,
  ProjectStats,
  IsoDate,
} from "./types";

export type AppErrorKind =
  | "validation"
  | "not_found"
  | "conflict"
  | "calendar"
  | "corrupt_row"
  | "database"
  | "storage"
  | "unknown";

export interface FieldError {
  field: string;
  message: string;
}

interface AppErrorShape {
  kind: AppErrorKind;
  message: string;
  fields: FieldError[];
}

function isAppErrorShape(value: unknown): value is AppErrorShape {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as AppErrorShape).kind === "string" &&
    typeof (value as AppErrorShape).message === "string"
  );
}

export class AppError extends Error {
  readonly kind: AppErrorKind;
  readonly fields: FieldError[];

  constructor(kind: AppErrorKind, message: string, fields: FieldError[] = []) {
    super(message);
    this.name = "AppError";
    this.kind = kind;
    this.fields = fields;
  }

  /** Anything thrown across the IPC boundary, normalised. */
  static from(raw: unknown): AppError {
    if (raw instanceof AppError) return raw;
    if (isAppErrorShape(raw)) {
      return new AppError(raw.kind, raw.message, raw.fields ?? []);
    }
    if (raw instanceof Error) return new AppError("unknown", raw.message);
    return new AppError("unknown", String(raw));
  }

  /** The message for one form field, if that field is what went wrong. */
  messageFor(field: string): string | undefined {
    return this.fields.find((e) => e.field === field)?.message;
  }

  get isValidation(): boolean {
    return this.kind === "validation";
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    throw AppError.from(raw);
  }
}

export const api = {
  listProjects: (filter: ProjectFilter | null = null) =>
    call<Project[]>("list_projects", { filter }),

  getProject: (id: string) => call<Project | null>("get_project", { id }),

  createProject: (draft: ProjectDraft) => call<Project>("create_project", { draft }),

  updateProject: (id: string, draft: ProjectDraft) =>
    call<Project>("update_project", { id, draft }),

  deleteProject: (id: string) => call<Project>("delete_project", { id }),

  portfolioStats: () => call<PortfolioStats>("portfolio_stats"),

  projectStats: (id: string, asOf: IsoDate | null = null) =>
    call<ProjectStats>("project_stats", { id, asOf }),

  projectHolidays: (id: string) => call<Holiday[]>("project_holidays", { id }),

  addProjectHoliday: (id: string, holiday: HolidayDraft) =>
    call<Holiday>("add_project_holiday", { id, holiday }),

  removeProjectHoliday: (id: string, holiday: string) =>
    call<void>("remove_project_holiday", { id, holiday }),

  recentActivity: (limit: number | null = null) =>
    call<AuditEntry[]>("recent_activity", { limit }),

  // Employees. No screen calls these yet — the employees view is the next
  // slice — but the commands and their shapes are in place.
  listEmployees: (filter: EmployeeFilter | null = null) =>
    call<Employee[]>("list_employees", { filter }),

  getEmployee: (id: string) => call<Employee | null>("get_employee", { id }),

  createEmployee: (project: string, draft: EmployeeDraft) =>
    call<Employee>("create_employee", { project, draft }),

  updateEmployee: (id: string, draft: EmployeeDraft) =>
    call<Employee>("update_employee", { id, draft }),

  deleteEmployee: (id: string) => call<Employee>("delete_employee", { id }),

  employeeStats: (id: string, asOf: IsoDate | null = null) =>
    call<EmployeeStats>("employee_stats", { id, asOf }),
};
