# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

Tauri v2 + React/TS/Vite shell. The **projects**, **employees** and **time & attendance** slices are built end to end (Rust + UI + Playwright); the **contracts** backend is built and tested with no UI yet. Leave and payroll are still to come — the nav and the employee-file tabs for those render a `ComingSoon` panel saying which slice they wait on. `design/Tymio HR.html` is the UI target; `README.md` is the design spec.

Front end under `src/`:

- `App.tsx` — two screens: pick a project, then work inside it. Owns the toast.
- `screens/` — `ProjectsScreen` (the picker), `Workspace` (rail + topbar + view routing), `EmployeesView` (the table), `EmployeeFile` (header, tabs, detail panels), `AttendanceView` (the monthly grid).

**The attendance grid holds its own input state, and only resets when the parent remounts it** via a `syncKey` (a refill, a cleared row, a new month). Re-syncing from the server on every save would wipe whatever the user had already typed into the next box — an in-flight save must never overwrite live input.
- `components/` — project card, both form modals, confirm dialog, activity panel, avatar, detail list, `ComingSoon`, toast.
- `ipc.ts` (the only place that calls `invoke`), `types.ts` (hand-mirrored from the Rust structs), `format.ts` (dd/mm/yyyy dates, space-separated thousands, month/service/weekday-mask helpers), `styles.css` (design tokens from the mockup).

**Form fields associate their label with `htmlFor` and their hint with `aria-describedby`.** Nesting a hint inside the `<label>` folds it into the input's accessible name, which breaks `getByLabel` and screen readers alike.

Rust layout under `src-tauri/src/`:

| Path | Holds |
| --- | --- |
| `domain/calendar.rs` | `WeekdayMask`, `DayLength` (minutes), `YearMonth`, `HolidaySet`, `WorkCalendar` — working-day counting |
| `domain/project.rs` | `Project`, `ProjectStatus`, `ProjectDraft` → `ValidProject`, `Holiday`, `ProjectFilter`, `DurationProgress`, `ProjectStats`, `PortfolioStats` |
| `domain/employee.rs` | `Employee`, `EmployeeDraft` → `ValidEmployee`, `EmployeeFilter`, `EmployeeStats`; age, whole-months service, and `months_worked_in` (the mockup's accrual driver) |
| `domain/contract.rs` | `PayType`, `Rate` (scale 4, serialised as a string), `WeeklyHours`, `LeaveDays`, `ContractTerms` with the ÷26 / ÷173 / ÷8 conversions, `ContractDraft` → `ValidContract`, `round_ariary` |
| `domain/attendance.rs` | `WorkedDays` (half-days), `WorkedTime` (minutes), `AttendanceDraft` → `ValidAttendance`, `AttendanceSheet`/`Totals`, and `from_standard_schedule` — the "Fill from standard schedule" rule |
| `domain/mod.rs` | `ValidationError`/`ValidationErrors`, the `id_type!` newtype macro |
| `repo/mod.rs` | `ProjectRepository`, `EmployeeRepository`, `AttendanceRepository`, `ContractRepository` and `ActivityRepository` traits, `AuditEntry` |
| `repo/sqlite.rs` | `SqliteProjectRepository`, `SqliteEmployeeRepository`, `SqliteActivityRepository` — **one struct per aggregate**, because several traits on one type make every `create`/`get`/`list` call ambiguous. Every write is a transaction that also appends its audit row |
| `db.rs` | pool setup, pragmas, embedded migrations, `Db::in_memory()` for tests |
| `commands.rs` | `AppState::new(db)` builds one repository per trait; validation happens here, then one-line `#[tauri::command]` wrappers |
| `error.rs` | `AppError`, serialised to the front end as `{ kind, message, fields }` |
| `migrations/0001_init.sql` | `projects`, `project_holidays`, `audit_log` |
| `migrations/0002_employees.sql` | `employees` — CIN unique where recorded, cascade from the project |
| `migrations/0003_attendance.sql` | `attendance` — one row per `(employee, period)`, days in half-days, hours in minutes |
| `migrations/0004_contracts.sql` | `contracts` — effective-dated versions, rate at scale 4 as an integer |

### Testing

`cargo test` covers the domain, repository and command layers. The Playwright suite (`e2e/`) covers the front end **against the real Rust backend**, because Playwright cannot drive a Tauri window — the app renders in WKWebView on macOS and there is no WebDriver for it. Instead:

- `src-tauri/examples/devserver.rs` puts the same `AppState` methods behind HTTP on `127.0.0.1:4599`, against an in-memory migrated SQLite database. It is an *example*, not a bin, so `tiny_http` never links into the shipped app.
- `vite.config.ts` proxies `/ipc` to it, and `e2e/fixtures.ts` defines `window.__TAURI_INTERNALS__.invoke` to POST there. That fixture contains no business logic — it is a transport, not a mock backend.
- `POST /ipc/__reset` gives each test a fresh database.
- `e2e/helpers.ts` holds the shared page actions (`createProject`, `enterProject`, `addEmployee`, `detail`); the specs stay declarative.

So React, the command layer, the repository, the schema and the domain rules are genuinely under test. Tauri's own IPC transport and the three platform webviews are not — those still need manual `npm run tauri dev`.

Conventions worth copying when the next slice lands: drafts are validated into a `Valid*` newtype that the repository is the only consumer of, so nothing invalid can reach the DB by another route; validation collects **every** failing field rather than stopping at the first; and `as_of` dates are passed in, never read from the clock inside the domain, so stats are reproducible in a test.

Pinned by the scaffold: Tauri 2.11.5, React 19, Vite 7, TypeScript 5.8, Rust edition 2021. Package manager is **npm** (bun is not installed on this machine).

## Commands

```sh
npm install              # once
npm run tauri dev        # run the app (Vite on :1420 + Rust, hot reload both sides)
npm run build            # typecheck all three tsconfigs, then vite build
npm run typecheck        # src, vite.config.ts and the e2e suite
npm run test:e2e         # Playwright against the real Rust backend (see below)
npm run tauri build      # packaged installers (needs signing setup, see README)

cd src-tauri && cargo check   # type-check Rust without linking
cd src-tauri && cargo test    # domain, repository and command tests
cd src-tauri && cargo test domain::project    # one module
cd src-tauri && cargo clippy --all-targets    # clean as of the projects slice
```

Notes that cost time if unknown:

- The **first** `cargo check` or `tauri dev` compiles ~500 crates and takes 4–5 minutes. Incremental builds after that are seconds. Do not assume a hang.
- `cargo` commands must run from `src-tauri/`, not the repo root.
- `npm run tauri dev` runs `npm run dev` itself — do not start Vite separately, port 1420 is `strictPort` and will fail.
- npm 11 blocks the `esbuild` postinstall with an `allow-scripts` warning. It is harmless; the frontend builds fine.

## What this is

A single-user desktop HR app. Everything is scoped to a **project**: create a project, staff it with employees, give each a contract (monthly/daily/hourly), track leave against a balance, and compute monthly pay.

`README.md` holds the full design: scope, out-of-scope decisions, schema shape, and the reasoning behind each rule. Read it before making design decisions — most "should I do X" questions are already answered there.

## Design rules that override convenience

These are settled and load-bearing. `README.md` has the full rationale; the short version:

- **No floats for money, anywhere.** `rust_decimal` in Rust, integers in storage.
- **Contracts are effective-dated versions, never updated in place.** A March payroll run must still reproduce identically in December after two raises. *Built.* `amend` writes a new version and closes the previous window the day before; there is no `update`. Two date pairs, meaning different things: `valid_from`/`valid_to` is the **version** window, `start_date`/`end_date` the contract's own duration. `discard_latest` is the only removal, and it reopens the version it superseded. The repository enforces the no-overlap invariant itself — SQLite has no exclusion constraint.
- **Payroll runs snapshot their inputs, then lock.** Each item records the contract version and the day/hour/leave counts it used.
- **Leave balance is an append-only ledger, never a mutable counter.** Accrual entries are keyed `(employee, kind, period)` with a unique constraint so re-running accrual is a no-op. See the caveat under "Mockup logic is illustrative" below.
- **Civil dates (`chrono::NaiveDate`), never UTC timestamps**, for leave, contracts, and payroll periods.
- **Payslip PDFs are generated in Rust**, never via `window.print()` or webview print-to-PDF — the three webviews render differently.
- **All DB access behind a repository trait**, so single-user SQLite could become a client of something else.
- **`PRAGMA foreign_keys = ON`** — SQLite defaults it off and this schema is heavily relational.
- **The DB file lives in the OS app data dir**, resolved at runtime via Tauri's `app_data_dir()` (`~/Library/Application Support/io.tymio.hr/tymio.db` on macOS) — never beside the binary, the CWD, or in the repo.

## Domain rules extracted from the mockup

The mockup encodes concrete business rules that exist nowhere else in prose. Preserve these unless the user changes them:

**Pay basis conversions** — needed whenever a rate has to cross units:
- monthly → daily: `rate ÷ 26`
- monthly → hourly: `rate ÷ 173`
- daily → hourly: `rate ÷ 8`

**Pay computation** (gross only — no tax, no social contributions):
- monthly: `rate + overtime − (rate ÷ 26 × unpaid days)`
- daily: `rate × days worked + overtime − unpaid days`
- hourly: `rate × hours logged + overtime`
- overtime: `hourly_equivalent × 1.3 × overtime_hours`
- "Net" in the UI means `base + overtime − deductions`. It is **not** net of tax.

**Leave**: an annual grant on 1 January, and/or N days accrued per month worked (a contract can have either, both, or neither). Once the balance is empty, further leave is unpaid and deducted from salary. Leave types seen: Annual, Sick, Unpaid. Request statuses: Pending, Approved, Rejected.

**Time & attendance**: per employee, per month — days worked, hours worked, overtime hours. Defaults come from the project's standard schedule ("Fill from standard schedule"), then are manually adjustable. This reconciles the README's "derived from a work calendar" with the mockup's editable grid: the calendar seeds the numbers, the user can override them.

*Built, end to end.* Days are stored in **half-days** and hours in **minutes** — never floats. Recording a month is an upsert on `(employee, period)`, not a create/update pair, because that is what the grid does. Seeding clips to the part of the month a person was actually employed for, carries existing overtime over (the calendar cannot know it), and has a `leave` parameter that the leave slice fills in — it is passed zero for now. `source` (`schedule` | `manual`) records which of the two last wrote each row.

**Contract fields**: type, rate, start, end, weekly hours (40), probation months (3), annual grant days, monthly accrual days.

*Built.* Rates are `rust_decimal` at scale 4, stored as a scale-4 integer, and cross the IPC boundary as **decimal strings** — a JSON number would lose `123456.7891`. The basis conversions are fixed conventions (÷26, ÷173, ÷8), deliberately not derived from the project's own day length, and they return **unrounded** decimals: `round_ariary` is applied once, at the final amount. Leave policy is two independent fields (grant *and* accrual, either/both/neither), which follows the mockup rather than the README's single `accrual_type` enum.

**Project fields**: name, client, location, status (Active | Paused | Closed), start, end.

## Locale

The mockup is localised for **Madagascar**: currency **MGA (Ariary)**, formatted `3 200 000 Ar` — space-separated thousands, `Ar` suffix. Phone numbers `+261 …`, national ID is a **CIN**, addresses are Malagasy.

**The Ariary is effectively a zero-decimal currency.** README's generic "store amounts as integer minor units (cents)" needs reading as "integer Ariary" here. Rates still need sub-unit precision because of the `÷ 26`, `÷ 173`, and `÷ 8` conversions — keep rates at scale 4 and round only at the final amount.

## The design mockup

`design/Tymio HR.html` is a 544 KB self-unpacking bundle — **grep and Read are useless on it**, the content is gzip+base64 inside a `__bundler/manifest` script. It renders as a clickable React prototype covering seven views: `overview`, `employees`, `employee` (employee file), `time` (Time & attendance), `leaves`, `payroll`, `reports`.

To read its markup or logic:

```python
import re, json
s = open('design/Tymio HR.html', encoding='utf-8', errors='replace').read()
t = json.loads(re.search(r'<script type="__bundler/template"[^>]*>(.*?)</script>', s, re.S).group(1))
# t is the design HTML; the app logic is the <script data-dc-script> block inside it
js = re.search(r'<script[^>]*data-dc-script[^>]*>(.*?)</script>', t, re.S).group(1)
```

Fonts and the React UMD bundles are also in the manifest, keyed by UUID, gzipped and base64-encoded.

**Mockup logic is illustrative, not a porting target.** It is a prototype with in-memory state: contracts are a single mutable object on the employee (not effective-dated versions), and leave balance is recomputed as `entitlement − paid days taken` (a derived counter, not a ledger). The screens and business rules are the spec; the data handling is not. Follow the design rules above instead.

## Unsettled

- **Settings view.** The mockup has none, but backup / restore / clear-to-empty-database are required (see README “Database maintenance”). Build them as Rust commands first; ask the user before designing UI for them.
- **Overtime** (`×1.3`) appears throughout the mockup but never came up when scope was agreed. Confirm with the user before building it into the payroll engine or the schema.
- **Project `client` field** is in the mockup but not in the README's schema sketch.
- The mockup shows an "HR admin" user chip, but scope is explicitly single-user with no authentication. Treat the chip as decoration.
