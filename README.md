# tymio

A cross-platform desktop app for HR management, organised around projects: create a project, staff it with employees, define their contracts, track their leave, and compute monthly pay.

**Status:** design settled, no implementation yet. This README is the reference for what gets built and why.

## Scope

Everything hangs off a **project** — it is the first thing you create and the context for everything else.

| Area | What it does |
| --- | --- |
| **Projects** | Full CRUD plus stats. Owns its work calendar (working days, hours per day, holidays). |
| **Employees** | Full CRUD plus stats. Each employee belongs to exactly one project. Full personal details. |
| **Contracts** | Monthly, daily, or hourly pay. Contract duration. Leave policy: a fixed grant each year, or a set number of days accrued each month. |
| **Leave** | Requests deducted from the available balance. Once the balance is exhausted, further days are unpaid and deducted from salary. |
| **Payroll** | Per project, per month. Derives worked days from the work calendar minus leave, applies the contract, produces a payslip breakdown. |

### Deliberately out of scope

- **Statutory deductions.** Payroll computes **gross pay only** — no tax brackets, no social contributions, no country-specific rules. Payslip lines are typed (earning / deduction / adjustment) so this could be added later, but nothing is built for it.
- **Timesheets.** Worked days are derived from the project work calendar, not entered per day.
- **Multiple users.** Single user, single machine. No server, no authentication, no sync.
- **Multi-project employees.** One employee, one project.

## Tech stack

| Layer | Choice |
| --- | --- |
| Shell | **Tauri v2** (2.11.x) |
| Front end | React + TypeScript + Vite |
| Back end | Rust — all domain logic lives here |
| Database | SQLite via `sqlx`, with migrations |
| IPC | `tauri-specta` — TypeScript types generated from the Rust commands |
| Money | `rust_decimal`, integer minor units in storage |
| Dates | `chrono::NaiveDate` |
| Payslips | Rust-side PDF generation (`printpdf` / `typst`) |
| At rest | SQLCipher (`rusqlite` `bundled-sqlcipher`), key in the OS keychain |

### Why Tauri

The app is CRUD, tables, and stats over local data, with no need for Chromium-specific APIs — Tauri's sweet spot. Against Electron it means ~3–10 MB installers instead of 120–200 MB, 40–80 MB idle memory instead of 150–400 MB, and sub-500ms cold start. Tauri v2 has been stable since late 2024 and is on a steady maintenance line, so there is no major-version churn to absorb.

The cost of that choice is testing the front end against three engines — WKWebView (macOS), WebView2 (Windows), WebKitGTK (Linux) — rather than one bundled Chromium.

## Design rules

These are the decisions that are expensive to reverse. They hold everywhere in the codebase.

**Money is never a float.** `rust_decimal` in Rust; amounts stored as integer minor units; rates stored at scale 4 so `rate × 7.5 hours` does not drift. A single `f64` in the salary path produces payslips that are wrong by a cent, and an employee finds it before the tests do.

**Contracts are versioned, never mutated.** A raise or extension inserts a new effective-dated row (`valid_from` / `valid_to`); the old row stays. A payroll run computed in March must still reproduce identically in December, after two raises have happened since.

**Payroll runs snapshot their inputs and lock.** Each `payroll_item` records the contract version, day and hour counts, and leave split it actually used. Once a run is closed it is immutable.

**Leave balance is a ledger, not a counter.** No `leave_days_remaining` column. An append-only ledger of accruals, consumptions, adjustments, carry-over and expiry, with the balance derived as a `SUM`. This buys three things a counter cannot give:

- Monthly accrual is **idempotent** — accrual entries are keyed `(employee, kind, period)` with a unique constraint, so running it twice is a no-op. With a mutable counter, one double-run silently corrupts a balance permanently and undetectably.
- Retroactive corrections work, and "why is my balance 12.5 days?" is answerable.
- The paid-vs-unpaid split resolves against the balance **as of the leave date**, not as of today.

**All dates are civil dates.** Leave periods, contract dates and payroll months are calendar dates, not UTC instants. Timestamps make a leave day shift across a DST boundary or on a laptop that travels.

**Payslip PDFs are generated in Rust, never by the webview.** `window.print()` and webview print-to-PDF render differently on WKWebView, WebView2 and WebKitGTK. Rust-side generation is byte-identical on all three platforms.

**Every DB call sits behind a repository trait.** Free now, and the difference between a week and a month if this ever goes multi-user.

**Append-only audit log from the first migration.** HR data gets disputed. "Who changed this salary, and when" is a question that will be asked.

**Foreign keys on.** SQLite defaults `PRAGMA foreign_keys` to **off**, and this schema is heavily relational.

## Data model

```
projects ──┬── holidays
           ├── employees ──┬── contracts        (effective-dated versions)
           │               ├── leave_requests ──┐
           │               └── leave_ledger  ◄──┘ (append-only, balance = SUM)
           └── payroll_runs ── payroll_items ── payroll_lines
                (draft|locked)   (per employee,     (base pay, unpaid-leave
                                  snapshots inputs)  deduction, adjustments)

audit_log  (append-only, spans everything)
```

- **`projects`** — work calendar lives here: working-days mask, `hours_per_day`, plus a `holidays` table.
- **`contracts`** — `valid_from` / `valid_to`, `pay_type` (monthly | daily | hourly), `rate` at scale 4, contract start/end, and the leave policy as `accrual_type` (annual grant | monthly accrual) + `days_per_period`. That enum covers both leave schemes.
- **`leave_requests`** — each request splits into paid days (covered by balance) and unpaid days (balance exhausted, deducted from salary).
- **Worked days** for a month = working days in the calendar − leave − absences. No timesheet table.

## Data safety

The database holds employee PII and salaries, on one machine, with no server behind it.

- Encrypted at rest with SQLCipher; key in the OS keychain, never in a config file.
- Backup and restore ship before v1 — `VACUUM INTO` for clean snapshot copies. A corrupted SQLite file with no backup is total loss of HR records.
- CSV/JSON export early: an escape hatch, and the answer to a data-subject request.

### Where the database lives

Resolved at runtime from the Tauri `AppHandle`, never relative to the executable or the working directory — an installed bundle is read-only and code-signed, and the data has to outlive app upgrades and reinstalls.

| Platform | `app_data_dir()` | File |
| --- | --- | --- |
| macOS | `~/Library/Application Support/io.tymio.hr/` | `tymio.db` |
| Windows | `%APPDATA%\io.tymio.hr\` | `tymio.db` |
| Linux | `~/.local/share/io.tymio.hr/` | `tymio.db` |

The directory is created on first run, then migrations run against the file inside it. A path override exists only for tests, pointing at a temp dir.

### Database maintenance

Three operations on that file, built as Rust commands on the repository layer:

- **Backup** — `VACUUM INTO` a user-chosen path. Clean snapshot, no WAL to reason about.
- **Restore** — close the pool, move the live file aside under a timestamped name, put the snapshot in place, reopen, run migrations.
- **Clear** — same sequence, but into a freshly migrated empty database.

The mockup has **no settings view**, so there is currently nowhere in the design to expose these. Build the commands first; the surface that hosts them still has to be designed.

## Known risks

**WebKitGTK table performance on Linux.** The most common Linux complaint about Tauri is slow layout on large tables — and this is a table-heavy app. Mitigation: virtualise any list that can grow (never render thousands of DOM rows), keep CSS animation modest, and test on Linux from the first week rather than before release.

**Distribution setup is front-loaded.** Windows code signing needs an HSM-backed certificate (Azure Key Vault is the accessible route for a small team) — since June 2023, CAs no longer issue exportable OV certificates, and unsigned Windows builds get SmartScreen warnings. macOS needs an Apple Developer account and notarization. Linux signing is optional (GPG for AppImage/RPM). `tauri build` emits `.deb` / `.rpm` / `.AppImage` in one command; the official `tauri-action` workflow produces signed installers for all three platforms in CI.

## Roadmap

1. Tauri v2 scaffold, migration set, repository layer.
2. Rust domain layer: payroll computation and leave-balance resolution, with tests — half-day leave straddling a month boundary, accrual run twice, balance exhausted mid-request.
3. Projects CRUD and stats.
4. Employees CRUD and details.
5. Contracts with versioning.
6. Leave requests against the ledger.
7. Payroll runs, locking, payslip PDF.
8. Backup/restore, export, packaging and signing.

## Watching

**Verso** — the Servo-based, all-Rust webview the Tauri team ships as an official experiment, aimed at exactly the engine-inconsistency problem this project accepts, with a roadmap toward an evergreen auto-updating runtime like WebView2. Still experimental; window decorations and transparency are only planned. Worth re-checking in a year.
