-- Tymio — initial schema.
--
-- Conventions that hold for every migration from here on:
--   * civil dates (YYYY-MM-DD) as TEXT, never UTC instants, for anything a
--     human would call a date: project start/end, holidays, leave, payroll
--     periods. Instants (created_at, audit) are RFC 3339 in UTC.
--   * money and durations as integers. No REAL columns, anywhere.
--   * every table that hangs off another declares ON DELETE explicitly.

CREATE TABLE projects (
    id                    TEXT    PRIMARY KEY NOT NULL,
    name                  TEXT    NOT NULL,
    client                TEXT,
    location              TEXT,
    status                TEXT    NOT NULL CHECK (status IN ('active', 'paused', 'closed')),
    start_date            TEXT    NOT NULL,
    end_date              TEXT,
    -- Work calendar. Bit 0 = Monday … bit 6 = Sunday; at least one day set.
    working_days_mask     INTEGER NOT NULL CHECK (working_days_mask BETWEEN 1 AND 127),
    -- Standard day length in minutes, so 7.5 h is exact and no REAL is needed.
    hours_per_day_minutes INTEGER NOT NULL CHECK (hours_per_day_minutes BETWEEN 1 AND 1440),
    created_at            TEXT    NOT NULL,
    updated_at            TEXT    NOT NULL,
    CHECK (end_date IS NULL OR end_date >= start_date)
);

CREATE INDEX idx_projects_status ON projects (status);

-- The rest of the project work calendar: days off that are not weekends.
CREATE TABLE project_holidays (
    id         TEXT NOT NULL PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    date       TEXT NOT NULL,
    name       TEXT NOT NULL,
    UNIQUE (project_id, date)
);

CREATE INDEX idx_project_holidays_project_date ON project_holidays (project_id, date);

-- Append-only. Present from the first migration because "who changed this
-- salary, and when" is a question HR data always ends up being asked.
CREATE TABLE audit_log (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    at        TEXT    NOT NULL,
    entity    TEXT    NOT NULL,
    entity_id TEXT    NOT NULL,
    action    TEXT    NOT NULL CHECK (action IN ('create', 'update', 'delete')),
    detail    TEXT
);

CREATE INDEX idx_audit_log_entity ON audit_log (entity, entity_id, id);
