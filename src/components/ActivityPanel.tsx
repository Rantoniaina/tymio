import { formatInstant } from "../format";
import type { AuditEntry } from "../types";

/** One set of verbs per entity the audit log records. */
const VERBS: Record<string, Record<AuditEntry["action"], string>> = {
  project: {
    create: "Created project",
    update: "Updated project",
    delete: "Deleted project",
  },
  project_holiday: {
    create: "Added holiday",
    update: "Updated holiday",
    delete: "Removed holiday",
  },
  employee: {
    create: "Created employee",
    update: "Updated employee",
    delete: "Removed employee",
  },
};

/**
 * The audit detail is an opaque JSON snapshot; a readable name is the only
 * part this panel wants out of it. Projects and holidays carry `name`, people
 * carry their two.
 */
function nameFrom(detail: string | null): string | null {
  if (!detail) return null;
  try {
    const parsed: unknown = JSON.parse(detail);
    if (!parsed || typeof parsed !== "object") return null;
    const record = parsed as Record<string, unknown>;

    const { firstName, lastName } = record;
    if (typeof firstName === "string" && typeof lastName === "string") {
      return `${firstName} ${lastName}`;
    }
    return typeof record.name === "string" ? record.name : null;
  } catch {
    // A snapshot we cannot read is still a logged change; show it without a name.
    return null;
  }
}

function describe(entry: AuditEntry): string {
  const verb = VERBS[entry.entity]?.[entry.action] ?? `${entry.action} ${entry.entity}`;
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
