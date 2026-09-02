import { useState } from "react";

import { Avatar } from "../components/Avatar";
import {
  ABSENT,
  daysFromHalves,
  formatMinutes,
  formatMonth,
  halvesFromDays,
  hoursFromMinutes,
  minutesFromHours,
} from "../format";
import { fullName, initialsOf } from "./EmployeesView";
import type {
  AttendanceDraft,
  AttendanceRow,
  AttendanceSheet,
  Employee,
  YearMonth,
} from "../types";

/** What one row's three boxes hold while it is being edited. */
interface RowValues {
  days: string;
  hours: string;
  overtime: string;
}

const BLANK: RowValues = { days: "", hours: "", overtime: "" };

function valuesOf(row: AttendanceRow): RowValues {
  if (!row.entry) return BLANK;
  return {
    days: daysFromHalves(row.entry.daysWorked),
    hours: hoursFromMinutes(row.entry.hoursWorked),
    overtime: hoursFromMinutes(row.entry.overtime),
  };
}

function draftOf(values: RowValues): AttendanceDraft {
  return {
    daysWorkedHalves: halvesFromDays(Number(values.days) || 0),
    hoursWorkedMinutes: minutesFromHours(Number(values.hours) || 0),
    overtimeMinutes: minutesFromHours(Number(values.overtime) || 0),
  };
}

interface AttendanceViewProps {
  sheet: AttendanceSheet | null;
  roster: Employee[];
  period: YearMonth;
  busy: boolean;
  errors: Record<string, string>;
  /**
   * Changes when the whole grid is replaced from elsewhere — a refill, a
   * cleared row, a different month. The boxes reset only then; a save landing
   * while somebody is typing into the next box must not overwrite them.
   */
  syncKey: string;
  onFill: () => void;
  onRecord: (employeeId: string, draft: AttendanceDraft) => void;
  onClear: (employeeId: string) => void;
}

export function AttendanceView({
  sheet,
  roster,
  period,
  busy,
  errors,
  syncKey,
  onFill,
  onRecord,
  onClear,
}: AttendanceViewProps) {
  const people = new Map(roster.map((employee) => [employee.id, employee]));

  return (
    <div>
      <div className="toolbar">
        {/* The mockup claims approved leave is already excluded. It is not —
            there is no leave slice yet — so this says what actually happens. */}
        <p className="toolbar__prose">
          Days and hours recorded for {formatMonth(period)}. Payroll reads these numbers directly.
          Filling from the schedule uses the project's work calendar; approved leave will come off
          once the leave slice lands.
        </p>
        <div className="toolbar__spacer" />
        <button type="button" className="btn" onClick={onFill} disabled={busy || !sheet}>
          Fill from standard schedule
        </button>
      </div>

      <div className="table table--attendance" data-testid="attendance-grid">
        <div className="table__head">
          <div>Employee</div>
          <div>Source</div>
          <div>Days worked</div>
          <div>Hours worked</div>
          <div>Overtime h</div>
          <div />
        </div>

        {sheet?.rows.map((row) => (
          <AttendanceGridRow
            // Remounting on `syncKey` is what resets the boxes; see above.
            key={`${row.employeeId}|${syncKey}`}
            row={row}
            employee={people.get(row.employeeId)}
            error={errors[row.employeeId]}
            onRecord={onRecord}
            onClear={onClear}
          />
        ))}

        {sheet && sheet.rows.length === 0 && (
          <div className="table__empty" data-testid="attendance-empty">
            Nobody on this project yet. Add an employee before recording time.
          </div>
        )}

        {sheet && sheet.rows.length > 0 && (
          <div className="table__row table__total" data-testid="attendance-totals">
            <div className="table__total-label">Total</div>
            <div>
              {sheet.totals.recorded} of {sheet.rows.length}
            </div>
            <div data-testid="total-days">{daysFromHalves(sheet.totals.daysWorked)}</div>
            <div data-testid="total-hours">{formatMinutes(sheet.totals.hoursWorkedMinutes)}</div>
            <div data-testid="total-overtime">{formatMinutes(sheet.totals.overtimeMinutes)}</div>
            <div />
          </div>
        )}
      </div>
    </div>
  );
}

interface RowProps {
  row: AttendanceRow;
  employee?: Employee;
  error?: string;
  onRecord: (employeeId: string, draft: AttendanceDraft) => void;
  onClear: (employeeId: string) => void;
}

function AttendanceGridRow({ row, employee, error, onRecord, onClear }: RowProps) {
  // Seeded once per mount. The parent remounts the row when the grid is
  // replaced wholesale, which is the only time these should be thrown away.
  const [values, setValues] = useState<RowValues>(() => valuesOf(row));
  const stored = valuesOf(row);

  const name = employee ? fullName(employee) : row.employeeId;
  const dirty =
    values.days !== stored.days ||
    values.hours !== stored.hours ||
    values.overtime !== stored.overtime;

  const commit = (next: RowValues) => {
    setValues(next);
    onRecord(row.employeeId, draftOf(next));
  };

  /** `+`/`−`. Days step by half a day, hours and overtime by one hour. */
  const step = (field: keyof RowValues, by: number) => {
    const current = Number(values[field]) || 0;
    const next = Math.max(0, Math.round((current + by) * 100) / 100);
    commit({ ...values, [field]: String(next) });
  };

  const box = (field: keyof RowValues, label: string, increment: number) => (
    <div className="stepper">
      <button
        type="button"
        className="stepper__button"
        aria-label={`Decrease ${label} for ${name}`}
        onClick={() => step(field, -increment)}
      >
        −
      </button>
      <input
        className="stepper__input"
        type="number"
        min={0}
        step={increment}
        aria-label={`${label} for ${name}`}
        value={values[field]}
        placeholder={ABSENT}
        onChange={(e) => setValues({ ...values, [field]: e.target.value })}
        onBlur={() => dirty && commit(values)}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
      <button
        type="button"
        className="stepper__button"
        aria-label={`Increase ${label} for ${name}`}
        onClick={() => step(field, increment)}
      >
        +
      </button>
    </div>
  );

  return (
    <>
      <div
        className={`table__row${error ? " table__row--invalid" : ""}`}
        data-testid="attendance-row"
        data-employee-name={name}
      >
        <div className="cell-person">
          {employee && <Avatar initials={initialsOf(employee)} seed={employee.id} size={30} />}
          <div>
            <div className="cell-person__name">{name}</div>
            <div className="cell-person__role">{employee?.role ?? ""}</div>
          </div>
        </div>
        <div>
          {row.entry ? (
            <span className={`source source--${row.entry.source}`} data-testid="row-source">
              {row.entry.source === "schedule" ? "Schedule" : "Manual"}
            </span>
          ) : (
            <span className="cell-muted">{ABSENT}</span>
          )}
        </div>
        {box("days", "Days worked", 0.5)}
        {box("hours", "Hours worked", 1)}
        {box("overtime", "Overtime", 1)}
        <div className="cell-actions">
          {row.entry ? (
            <button
              type="button"
              className="btn btn--quiet btn--destructive-quiet"
              onClick={() => onClear(row.employeeId)}
            >
              Clear
            </button>
          ) : (
            <span className="cell-muted" data-testid="row-blank">
              Not recorded
            </span>
          )}
        </div>
      </div>
      {error && (
        <div className="table__error" role="alert" data-testid="row-error">
          {error}
        </div>
      )}
    </>
  );
}
