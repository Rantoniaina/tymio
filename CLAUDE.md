# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

Scaffold only: a Tauri v2 + React/TS/Vite shell that opens one empty window. No domain code, no database, no migrations, no tests yet. `design/Tymio HR.html` is the UI target; `README.md` is the design spec.

Pinned by the scaffold: Tauri 2.11.5, React 19, Vite 7, TypeScript 5.8, Rust edition 2021. Package manager is **npm** (bun is not installed on this machine).

## Commands

```sh
npm install              # once
npm run tauri dev        # run the app (Vite on :1420 + Rust, hot reload both sides)
npm run build            # frontend only: tsc && vite build
npm run tauri build      # packaged installers (needs signing setup, see README)

cd src-tauri && cargo check   # type-check Rust without linking
cd src-tauri && cargo test    # domain-layer tests (none exist yet)
cd src-tauri && cargo test payroll::tests::unpaid_leave   # single test by path
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
- **Contracts are effective-dated versions, never updated in place.** A March payroll run must still reproduce identically in December after two raises.
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

**Contract fields**: type, rate, start, end, weekly hours (40), probation months (3), annual grant days, monthly accrual days.

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
