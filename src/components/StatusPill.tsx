import { STATUS_LABELS, type ProjectStatus } from "../types";

export function StatusPill({ status }: { status: ProjectStatus }) {
  return <span className={`pill pill--${status}`}>{STATUS_LABELS[status]}</span>;
}
