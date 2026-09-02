-- Employees. One employee belongs to exactly one project (README: multi-project
-- employees are deliberately out of scope), so the project is a plain column
-- and not a join table.
--
-- Everything a contract decides — pay type, rate, weekly hours, probation,
-- leave policy — is absent on purpose. Contracts are effective-dated versions
-- in their own table, and putting a rate here would make a March payslip
-- change when someone gets a raise in June.

CREATE TABLE employees (
    id                TEXT NOT NULL PRIMARY KEY,
    project_id        TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    first_name        TEXT NOT NULL,
    last_name         TEXT NOT NULL,
    role              TEXT NOT NULL,
    email             TEXT,
    phone             TEXT,
    address           TEXT,
    -- Malagasy national identity number, digits only (spaces are stripped on
    -- the way in so that "201 021 045" and "201021045" are one person).
    cin               TEXT,
    birth_date        TEXT,
    hire_date         TEXT NOT NULL,
    bank_account      TEXT,
    emergency_contact TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    CHECK (birth_date IS NULL OR birth_date < hire_date)
);

-- The employees screen is always inside one project, sorted by name.
CREATE INDEX idx_employees_project ON employees (project_id, last_name, first_name);

-- A CIN identifies one person. Partial, so any number of employees may have
-- no CIN recorded while a recorded one cannot be entered twice.
CREATE UNIQUE INDEX idx_employees_cin ON employees (cin) WHERE cin IS NOT NULL;
