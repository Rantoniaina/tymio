-- Time & attendance: one row per employee per month.
--
-- The README says worked days are "derived from a work calendar" and the
-- design mockup shows an editable grid. Both are true: "Fill from standard
-- schedule" seeds these numbers from the project calendar, and they are then
-- adjustable by hand. `source` records which of the two last wrote the row,
-- because a disputed payslip will ask.
--
-- No floats, so days are counted in half-days (the granularity leave uses)
-- and hours in whole minutes.

CREATE TABLE attendance (
    id                   TEXT    NOT NULL PRIMARY KEY,
    employee_id          TEXT    NOT NULL REFERENCES employees (id) ON DELETE CASCADE,
    -- The civil month, YYYY-MM. A period is not a date.
    period               TEXT    NOT NULL,
    days_worked_halves   INTEGER NOT NULL CHECK (days_worked_halves >= 0),
    hours_worked_minutes INTEGER NOT NULL CHECK (hours_worked_minutes >= 0),
    overtime_minutes     INTEGER NOT NULL CHECK (overtime_minutes >= 0),
    source               TEXT    NOT NULL CHECK (source IN ('schedule', 'manual')),
    created_at           TEXT    NOT NULL,
    updated_at           TEXT    NOT NULL,
    -- One record per person per month; recording again replaces it.
    UNIQUE (employee_id, period)
);

CREATE INDEX idx_attendance_period ON attendance (period, employee_id);
