-- Contracts, as effective-dated versions.
--
-- A raise or an extension inserts a new row and closes the previous one's
-- window; the old terms stay exactly as they were. A payroll run computed in
-- March must still reproduce identically in December, after two raises have
-- happened since — which it cannot do if the March rate has been overwritten.
--
-- Two date pairs, and they mean different things:
--   * valid_from / valid_to  — when this *version of the terms* applies.
--   * start_date / end_date  — the employment contract's own duration, which
--     is itself a term and may differ between versions.
--
-- valid_to is NULL for the version in force. It is the only column here that
-- is ever written after insert, and only to close a window.

CREATE TABLE contracts (
    id                     TEXT    NOT NULL PRIMARY KEY,
    employee_id            TEXT    NOT NULL REFERENCES employees (id) ON DELETE CASCADE,

    valid_from             TEXT    NOT NULL,
    valid_to               TEXT,

    pay_type               TEXT    NOT NULL CHECK (pay_type IN ('monthly', 'daily', 'hourly')),
    -- The rate at scale 4, as an integer: 3 200 000 MGA is 32000000000.
    -- Ariary is effectively zero-decimal, but the ÷26, ÷173 and ÷8 basis
    -- conversions need the headroom.
    rate_scaled            INTEGER NOT NULL CHECK (rate_scaled > 0),

    start_date             TEXT    NOT NULL,
    end_date               TEXT,
    weekly_minutes         INTEGER NOT NULL CHECK (weekly_minutes > 0),
    probation_months       INTEGER NOT NULL CHECK (probation_months >= 0),
    -- Leave policy. A contract may have an annual grant, a monthly accrual,
    -- both, or neither; counted in half-days like every other day count here.
    annual_grant_halves    INTEGER NOT NULL CHECK (annual_grant_halves >= 0),
    monthly_accrual_halves INTEGER NOT NULL CHECK (monthly_accrual_halves >= 0),

    created_at             TEXT    NOT NULL,

    CHECK (valid_to IS NULL OR valid_to >= valid_from),
    CHECK (end_date IS NULL OR end_date >= start_date),
    -- Two versions of one contract cannot begin on the same day.
    UNIQUE (employee_id, valid_from)
);

-- "Which terms were in force on this date" is the query payroll runs.
CREATE INDEX idx_contracts_window ON contracts (employee_id, valid_from, valid_to);

-- "Contracts ending soon", on the project overview.
CREATE INDEX idx_contracts_ending ON contracts (end_date) WHERE valid_to IS NULL;
