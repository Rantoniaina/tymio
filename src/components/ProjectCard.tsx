import { StatusPill } from "./StatusPill";
import { ABSENT, formatDate, formatDayLength } from "../format";
import type { Project, ProjectStats } from "../types";

interface ProjectCardProps {
  project: Project;
  /** Absent until the stats call for this project comes back. */
  stats?: ProjectStats;
  onOpen: (project: Project) => void;
  onEdit: (project: Project) => void;
  onDelete: (project: Project) => void;
}

export function ProjectCard({ project, stats, onOpen, onEdit, onDelete }: ProjectCardProps) {
  const where = [project.client, project.location].filter(Boolean).join(" · ") || ABSENT;
  const percent = stats?.duration.percentElapsed ?? null;

  return (
    <article className="project" data-testid="project-card" data-project-name={project.name}>
      <div className="project__head">
        <div>
          <h3 className="project__name">{project.name}</h3>
          <div className="project__where">{where}</div>
        </div>
        <StatusPill status={project.status} />
      </div>

      {/* The mockup shows headcount, monthly cost and pending leave here.
          Those read tables that do not exist yet, so this strip carries the
          project's own work-calendar figures until the employees, leave and
          payroll slices land. */}
      <div className="project__figures">
        <div>
          <div className="figure__label">Working days</div>
          <div className="figure__value" data-testid="figure-working-days">
            {stats ? stats.workingDaysThisMonth : ABSENT}
          </div>
        </div>
        <div>
          <div className="figure__label">Standard day</div>
          <div className="figure__value">{formatDayLength(project.calendar.dayLength)}</div>
        </div>
        <div>
          <div className="figure__label">Holidays</div>
          <div className="figure__value" data-testid="figure-holidays">
            {stats ? stats.holidayCount : ABSENT}
          </div>
        </div>
      </div>

      <div>
        <div className="project__dates">
          <span>{formatDate(project.start)}</span>
          <span>{formatDate(project.end)}</span>
        </div>
        <div
          className="track"
          role="progressbar"
          aria-label={`${project.name} duration elapsed`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent ?? undefined}
        >
          <div
            className={`track__fill track__fill--${project.status}`}
            style={{ width: `${percent ?? 0}%` }}
          />
        </div>
        <div className="project__progress-label">
          {percent === null ? "Open-ended — no end date set" : `${percent}% of duration elapsed`}
        </div>
      </div>

      <div className="project__actions">
        <button type="button" className="btn btn--primary" onClick={() => onOpen(project)}>
          Open project
        </button>
        <button type="button" className="btn btn--quiet" onClick={() => onEdit(project)}>
          Edit
        </button>
        <button
          type="button"
          className="btn btn--quiet btn--destructive-quiet"
          onClick={() => onDelete(project)}
        >
          Delete
        </button>
      </div>
    </article>
  );
}
