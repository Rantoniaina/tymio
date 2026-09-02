import { useCallback, useEffect, useRef, useState } from "react";

import { ActivityPanel } from "./components/ActivityPanel";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { ProjectCard } from "./components/ProjectCard";
import { ProjectModal } from "./components/ProjectModal";
import { Toast } from "./components/Toast";
import { AppError, api } from "./ipc";
import { STATUS_LABELS } from "./types";
import type {
  AuditEntry,
  Holiday,
  HolidayDraft,
  PortfolioStats,
  Project,
  ProjectDraft,
  ProjectStats,
  ProjectStatus,
} from "./types";

import "./styles.css";

const ACTIVITY_LIMIT = 8;

const FILTERS: Array<{ label: string; value: ProjectStatus | null }> = [
  { label: "All", value: null },
  ...(["active", "paused", "closed"] as ProjectStatus[]).map((value) => ({
    label: STATUS_LABELS[value],
    value,
  })),
];

/** `null` means the form is closed; `{ project: null }` means "new project". */
type FormTarget = { project: Project | null } | null;

export default function App() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [stats, setStats] = useState<Record<string, ProjectStats>>({});
  const [portfolio, setPortfolio] = useState<PortfolioStats>({
    total: 0,
    active: 0,
    paused: 0,
    closed: 0,
    people: 0,
  });
  const [activity, setActivity] = useState<AuditEntry[]>([]);
  const [status, setStatus] = useState<ProjectStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [form, setForm] = useState<FormTarget>(null);
  const [holidays, setHolidays] = useState<Holiday[]>([]);
  const [pendingDelete, setPendingDelete] = useState<Project | null>(null);
  const [deleting, setDeleting] = useState(false);

  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const say = useCallback((message: string) => {
    setToast(message);
    clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 2600);
  }, []);

  useEffect(() => () => clearTimeout(toastTimer.current), []);

  const load = useCallback(async (forStatus: ProjectStatus | null) => {
    setLoading(true);
    try {
      const [listed, counts, recent] = await Promise.all([
        api.listProjects({ status: forStatus, query: null }),
        api.portfolioStats(),
        api.recentActivity(ACTIVITY_LIMIT),
      ]);

      setProjects(listed);
      setPortfolio(counts);
      setActivity(recent);
      setLoadError(null);

      // One stats call per visible card. Local IPC, and the project list is
      // tens of rows — if it ever is not, this becomes one batched command.
      const measured = await Promise.all(listed.map((project) => api.projectStats(project.id)));
      setStats(Object.fromEntries(measured.map((s) => [s.projectId, s])));
    } catch (raw) {
      setLoadError(AppError.from(raw).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(status);
  }, [load, status]);

  async function openForm(project: Project | null) {
    setForm({ project });
    setHolidays(project ? await api.projectHolidays(project.id) : []);
  }

  async function save(draft: ProjectDraft) {
    const target = form?.project ?? null;
    const saved = target
      ? await api.updateProject(target.id, draft)
      : await api.createProject(draft);

    setForm(null);
    say(target ? "Project saved" : `Created ${saved.name}`);
    await load(status);
  }

  async function addHoliday(draft: HolidayDraft) {
    const project = form?.project;
    if (!project) return;
    await api.addProjectHoliday(project.id, draft);
    setHolidays(await api.projectHolidays(project.id));
    await load(status);
  }

  async function removeHoliday(holidayId: string) {
    const project = form?.project;
    if (!project) return;
    await api.removeProjectHoliday(project.id, holidayId);
    setHolidays(await api.projectHolidays(project.id));
    await load(status);
  }

  async function confirmDelete() {
    if (!pendingDelete) return;
    setDeleting(true);
    try {
      await api.deleteProject(pendingDelete.id);
      say(`Deleted ${pendingDelete.name}`);
      setPendingDelete(null);
      await load(status);
    } catch (raw) {
      say(AppError.from(raw).message);
    } finally {
      setDeleting(false);
    }
  }

  const kpis = [
    { label: "Active projects", value: portfolio.active, sub: `${portfolio.total} in total` },
    { label: "Paused", value: portfolio.paused, sub: "on hold" },
    { label: "Closed", value: portfolio.closed, sub: "finished or archived" },
  ];

  return (
    <div className="screen">
      <div className="shell">
        <header className="masthead">
          <div>
            <div className="wordmark">
              <div className="wordmark__mark" aria-hidden="true" />
              <div className="wordmark__name">tymio</div>
            </div>
            <h1 className="masthead__title">Choose a project</h1>
            <p className="masthead__lede">
              Everything — people, contracts, leaves and payroll — lives inside a project. Pick one
              to continue, or open a new one.
            </p>
          </div>
          <button type="button" className="btn btn--primary" onClick={() => void openForm(null)}>
            New project
          </button>
        </header>

        {loadError && (
          <p className="banner" role="alert" data-testid="load-error">
            {loadError}
          </p>
        )}

        <div className="columns">
          <div>
            {/* The mockup's other three KPIs — headcount, pending leave and
                monthly payroll — need the employees, leave and payroll
                slices. These are the counts projects can answer alone. */}
            <div className="kpis">
              {kpis.map((kpi) => (
                <div className="card" key={kpi.label}>
                  <div className="kpi__label">{kpi.label}</div>
                  <div className="kpi__value" data-testid={`kpi-${kpi.label}`}>
                    {kpi.value}
                  </div>
                  <div className="kpi__sub">{kpi.sub}</div>
                </div>
              ))}
            </div>

            <div className="section-head">
              <h2 className="section-head__title">All projects</h2>
              <div className="section-head__rule" />
              {FILTERS.map((filter) => (
                <button
                  key={filter.label}
                  type="button"
                  className="chip"
                  aria-pressed={status === filter.value}
                  onClick={() => setStatus(filter.value)}
                >
                  {filter.label}
                </button>
              ))}
            </div>

            {projects.length === 0 && !loading ? (
              <div className="placeholder" data-testid="empty-state">
                <div className="placeholder__title">
                  {status ? `No ${STATUS_LABELS[status].toLowerCase()} projects` : "No projects yet"}
                </div>
                <p>
                  {status
                    ? "Try another filter, or create a project with this status."
                    : "Create the first project, then staff it with employees."}
                </p>
              </div>
            ) : (
              <div className="project-grid">
                {projects.map((project) => (
                  <ProjectCard
                    key={project.id}
                    project={project}
                    stats={stats[project.id]}
                    onOpen={() =>
                      say("The project workspace arrives with the employees slice.")
                    }
                    onEdit={(target) => void openForm(target)}
                    onDelete={setPendingDelete}
                  />
                ))}
              </div>
            )}
          </div>

          <aside className="aside">
            <ActivityPanel entries={activity} />
          </aside>
        </div>
      </div>

      {form && (
        <ProjectModal
          project={form.project}
          holidays={holidays}
          onSave={save}
          onAddHoliday={addHoliday}
          onRemoveHoliday={removeHoliday}
          onClose={() => setForm(null)}
        />
      )}

      {pendingDelete && (
        <ConfirmDialog
          title={`Delete ${pendingDelete.name}?`}
          body={
            <p>
              This removes the project and everything inside it — employees, contracts, leave and
              payroll. It cannot be undone.
            </p>
          }
          confirmLabel="Delete project"
          busy={deleting}
          onConfirm={() => void confirmDelete()}
          onCancel={() => setPendingDelete(null)}
        />
      )}

      <Toast message={toast} />
    </div>
  );
}
