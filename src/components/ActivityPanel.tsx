import { formatInstant } from "../format";
import type { AuditEntry } from "../types";

const PROJECT_VERBS = {
  create: "Created project",
  update: "Updated project",
  delete: "Deleted project",
} as const;

const HOLIDAY_VERBS = {
  create: "Added holiday",
  update: "Updated holiday",
  delete: "Removed holiday",
} as const;

/** The audit detail is an opaque JSON snapshot; the name is the readable part. */
function nameFrom(detail: string | null): string | null {
  if (!detail) return null;
  try {
    const parsed: unknown = JSON.parse(detail);
    if (parsed && typeof parsed === "object" && "name" in parsed) {
      const name = (parsed as { name: unknown }).name;
      return typeof name === "string" ? name : null;
    }
  } catch {
    // A snapshot we cannot read is still a logged change; show it without a name.
  }
  return null;
}

function describe(entry: AuditEntry): string {
  const verbs = entry.entity === "project_holiday" ? HOLIDAY_VERBS : PROJECT_VERBS;
  const verb = verbs[entry.action];
  const name = nameFrom(entry.detail);
  return name ? `${verb} — ${name}` : verb;
}

export function ActivityPanel({ entries }: { entries: AuditEntry[] }) {
  return (
    <section className="panel">
      <h2 className="panel__title">Recent activity</h2>
      <div className="panel__body" data-testid="activity">
        {entries.length === 0 ? (
          <p className="holidays__empty">Nothing has happened yet.</p>
        ) : (
          entries.map((entry) => (
            <div className="entry" key={entry.id}>
              <span className={`entry__dot entry__dot--${entry.action}`} aria-hidden="true" />
              <div>
                <div className="entry__what">{describe(entry)}</div>
                <div className="entry__when">{formatInstant(entry.at)}</div>
              </div>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
