import { useCallback, useEffect, useState } from "react";

import { ComingSoon } from "../components/ComingSoon";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { EmployeeModal } from "../components/EmployeeModal";
import { StatusPill } from "../components/StatusPill";
import { EmployeeFile, type EmployeeTab } from "./EmployeeFile";
import { EmployeesView, fullName } from "./EmployeesView";
import { AppError, api } from "../ipc";
import { asOfFor, formatDate, formatMonth, monthKey, parseMonthKey, recentMonths, thisMonth } from "../format";
import type { Employee, EmployeeDraft, EmployeeStats, Project, ProjectStats, YearMonth } from "../types";

type View = "overview" | "employees" | "employee" | "time" | "leaves" | "payroll" | "reports";

const NAV: Array<{ id: View; label: string }> = [
  { id: "overview", label: "Overview" },
  { id: "employees", label: "Employees" },
  { id: "time", label: "Time & attendance" },
  { id: "leaves", label: "Leaves" },
  { id: "payroll", label: "Payroll" },
  { id: "reports", label: "Reports" },
];

const TITLES: Record<View, string> = {
  overview: "Project overview",
  employees: "Employees",
  employee: "Employee file",
  time: "Time & attendance",
  leaves: "Leave management",
  payroll: "Payroll",
  reports: "Reports & export",
};

/** Views whose backend slice has not been built. */
const NOT_BUILT: Partial<Record<View, string>> = {
  time: "Days worked, hours and overtime per person per month arrive with the attendance slice, seeded from this project's work calendar.",
  leaves: "Requests, approvals and the append-only balance ledger arrive with the leave slice.",
  payroll: "Monthly runs, the payslip breakdown and locking arrive with the payroll slice, which needs contracts and attendance first.",
  reports: "CSV and JSON export arrive once there is something to export.",
};

interface WorkspaceProps {
  project: Project;
  say: (message: string) => void;
  onLeave: () => void;
}

export function Workspace({ project, say, onLeave }: WorkspaceProps) {
  const [view, setView] = useState<View>("employees");
  const [month, setMonth] = useState<YearMonth>(() => thisMonth());

  const [employees, setEmployees] = useState<Employee[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [projectStats, setProjectStats] = useState<ProjectStats | null>(null);

  const [openEmployee, setOpenEmployee] = useState<Employee | null>(null);
  const [openStats, setOpenStats] = useState<EmployeeStats | undefined>(undefined);
  const [tab, setTab] = useState<EmployeeTab>("profile");

  const [form, setForm] = useState<{ employee: Employee | null } | null>(null);
  const [pendingRemoval, setPendingRemoval] = useState<Employee | null>(null);
  const [removing, setRemoving] = useState(false);

  const asOf = asOfFor(month);

  const load = useCallback(
    async (search: string, at: string) => {
      setLoading(true);
      try {
        const [listed, stats] = await Promise.all([
          api.listEmployees({ project: project.id, query: search || null }),
          api.projectStats(project.id, at),
        ]);
        setEmployees(listed);
        setProjectStats(stats);
        setLoadError(null);
      } catch (raw) {
        setLoadError(AppError.from(raw).message);
      } finally {
        setLoading(false);
      }
    },
    [project.id],
  );

  useEffect(() => {
    void load(query, asOf);
  }, [load, query, asOf]);

  // The open employee's derived numbers follow the period selector.
  useEffect(() => {
    if (!openEmployee) {
      setOpenStats(undefined);
      return;
    }
    let current = true;
    void api
      .employeeStats(openEmployee.id, asOf)
      .then((stats) => current && setOpenStats(stats))
      .catch(() => current && setOpenStats(undefined));
    return () => {
      current = false;
    };
  }, [openEmployee, asOf]);

  async function save(draft: EmployeeDraft) {
    const target = form?.employee ?? null;
    const saved = target
      ? await api.updateEmployee(target.id, draft)
      : await api.createEmployee(project.id, draft);

    setForm(null);
    say(target ? "Employee saved" : `Added ${fullName(saved)}`);
    if (openEmployee?.id === saved.id) setOpenEmployee(saved);
    await load(query, asOf);
  }

  async function confirmRemoval() {
    if (!pendingRemoval) return;
    setRemoving(true);
    try {
      await api.deleteEmployee(pendingRemoval.id);
      say(`Removed ${fullName(pendingRemoval)}`);
      if (openEmployee?.id === pendingRemoval.id) {
        setOpenEmployee(null);
        setView("employees");
      }
      setPendingRemoval(null);
      await load(query, asOf);
    } catch (raw) {
      say(AppError.from(raw).message);
    } finally {
      setRemoving(false);
    }
  }

  function open(employee: Employee) {
    setOpenEmployee(employee);
    setTab("profile");
    setView("employee");
  }

  const headcount = projectStats?.headcount ?? employees.length;

  return (
    <div className="workspace">
      <nav className="rail" aria-label="Project sections">
        <div className="rail__brand">
          <span className="rail__mark" aria-hidden="true" />
          <span className="rail__name">tymio</span>
        </div>
        <div className="rail__nav">
          {NAV.map((entry) => {
            const active = view === entry.id || (entry.id === "employees" && view === "employee");
            return (
              <button
                key={entry.id}
                type="button"
                className="rail__link"
                aria-current={active ? "page" : undefined}
                onClick={() => setView(entry.id)}
              >
                <span className="rail__marker" aria-hidden="true" />
                <span className="rail__label">{entry.label}</span>
                <span className="rail__badge">
                  {entry.id === "employees" ? headcount || "" : NOT_BUILT[entry.id] ? "soon" : ""}
                </span>
              </button>
            );
          })}
        </div>
        <div className="rail__spacer" />
        <div className="rail__footer">
          <div className="rail__eyebrow">Current project</div>
          <div className="rail__project">{project.name}</div>
          <div className="rail__location">{project.location ?? project.client ?? ""}</div>
          <button type="button" className="rail__switch" onClick={onLeave}>
            Switch project
          </button>
        </div>
      </nav>

      <div className="workspace__main">
        <header className="topbar">
          <h1 className="topbar__title">{TITLES[view]}</h1>
          <StatusPill status={project.status} />
          <div className="topbar__spacer" />
          <label className="topbar__period">
            <span className="topbar__period-label">Period</span>
            <select
              className="field__input"
              aria-label="Period"
              value={monthKey(month)}
              onChange={(e) => setMonth(parseMonthKey(e.target.value))}
            >
              {recentMonths().map((option) => (
                <option key={monthKey(option)} value={monthKey(option)}>
                  {formatMonth(option)}
                </option>
              ))}
            </select>
          </label>
        </header>

        <main className="workspace__body">
          {loadError && (
            <p className="banner" role="alert" data-testid="load-error">
              {loadError}
            </p>
          )}

          {view === "overview" && (
            <div className="stack">
              {/* The mockup's four are Headcount, Monthly cost, Days worked and
                  Leave taken. Cost, worked days and leave need contracts,
                  attendance and leave; these are what a project can answer now. */}
              <div className="kpis kpis--four">
                <Kpi label="Headcount" value={headcount} sub="on this project" />
                <Kpi
                  label="Working days"
                  value={projectStats?.workingDaysThisMonth ?? "—"}
                  sub={formatMonth(month)}
                />
                <Kpi label="Holidays" value={projectStats?.holidayCount ?? "—"} sub="in the calendar" />
                <Kpi
                  label="Duration"
                  value={
                    projectStats?.duration.percentElapsed == null
                      ? "—"
                      : `${projectStats.duration.percentElapsed}%`
                  }
                  sub={`${formatDate(project.start)} → ${formatDate(project.end)}`}
                />
              </div>
              <ComingSoon
                title="Headcount by contract, and contracts ending soon"
                needs="Both panels read contract data, which arrives with the contracts slice."
              />
            </div>
          )}

          {view === "employees" && (
            <EmployeesView
              employees={employees}
              loading={loading}
              query={query}
              onQuery={setQuery}
              onAdd={() => setForm({ employee: null })}
              onOpen={open}
              onEdit={(employee) => setForm({ employee })}
              onRemove={setPendingRemoval}
            />
          )}

          {view === "employee" &&
            (openEmployee ? (
              <EmployeeFile
                employee={openEmployee}
                stats={openStats}
                project={project}
                tab={tab}
                onTab={setTab}
                onBack={() => setView("employees")}
                onEdit={() => setForm({ employee: openEmployee })}
              />
            ) : (
              <ComingSoon title="No employee open" needs="Pick someone from the employees list." />
            ))}

          {NOT_BUILT[view] && <ComingSoon title={TITLES[view]} needs={NOT_BUILT[view]!} />}
        </main>
      </div>

      {form && (
        <EmployeeModal
          employee={form.employee}
          projectName={project.name}
          onSave={save}
          onClose={() => setForm(null)}
        />
      )}

      {pendingRemoval && (
        <ConfirmDialog
          title={`Remove ${fullName(pendingRemoval)}?`}
          body={
            <p>
              This deletes their personal file, and will take their contracts, leave and payslips
              with it once those exist. It cannot be undone.
            </p>
          }
          confirmLabel="Remove employee"
          busy={removing}
          onConfirm={() => void confirmRemoval()}
          onCancel={() => setPendingRemoval(null)}
        />
      )}
    </div>
  );
}

function Kpi({ label, value, sub }: { label: string; value: string | number; sub: string }) {
  return (
    <div className="card">
      <div className="kpi__label">{label}</div>
      <div className="kpi__value" data-testid={`kpi-${label}`}>
        {value}
      </div>
      <div className="kpi__sub">{sub}</div>
    </div>
  );
}
