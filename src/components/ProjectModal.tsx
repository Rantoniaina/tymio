import { useEffect, useRef, useState, type FormEvent } from "react";

import { AppError } from "../ipc";
import { WEEKDAYS, formatDate, toggleWeekday, today, worksOn } from "../format";
import { PROJECT_STATUSES, STATUS_LABELS } from "../types";
import type { Holiday, HolidayDraft, Project, ProjectDraft, ProjectStatus } from "../types";

const MON_FRI = 0b0011111;
const EIGHT_HOURS = 8 * 60;

function draftFrom(project: Project | null): ProjectDraft {
  if (!project) {
    return {
      name: "",
      client: "",
      location: "",
      status: "active",
      start: today(),
      end: null,
      workingDays: MON_FRI,
      dayLength: EIGHT_HOURS,
    };
  }
  return {
    name: project.name,
    client: project.client ?? "",
    location: project.location ?? "",
    status: project.status,
    start: project.start,
    end: project.end,
    workingDays: project.calendar.workingDays,
    dayLength: project.calendar.dayLength,
  };
}

interface ProjectModalProps {
  /** `null` opens the form for a new project. */
  project: Project | null;
  holidays: Holiday[];
  onSave: (draft: ProjectDraft) => Promise<void>;
  onAddHoliday: (draft: HolidayDraft) => Promise<void>;
  onRemoveHoliday: (holidayId: string) => Promise<void>;
  onClose: () => void;
}

export function ProjectModal({
  project,
  holidays,
  onSave,
  onAddHoliday,
  onRemoveHoliday,
  onClose,
}: ProjectModalProps) {
  const [draft, setDraft] = useState<ProjectDraft>(() => draftFrom(project));
  const [error, setError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
  const [holidayDraft, setHolidayDraft] = useState<HolidayDraft>({ date: "", name: "" });
  const [holidayError, setHolidayError] = useState<string | null>(null);
  const firstField = useRef<HTMLInputElement>(null);

  useEffect(() => {
    firstField.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const set = <K extends keyof ProjectDraft>(key: K, value: ProjectDraft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  const hours = Math.floor(draft.dayLength / 60);
  const minutes = draft.dayLength % 60;

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await onSave({
        ...draft,
        client: draft.client?.trim() ? draft.client : null,
        location: draft.location?.trim() ? draft.location : null,
        end: draft.end?.trim() ? draft.end : null,
      });
    } catch (raw) {
      setError(AppError.from(raw));
    } finally {
      setSaving(false);
    }
  }

  async function addHoliday() {
    setHolidayError(null);
    try {
      await onAddHoliday(holidayDraft);
      setHolidayDraft({ date: "", name: "" });
    } catch (raw) {
      const failure = AppError.from(raw);
      setHolidayError(failure.messageFor("name") ?? failure.message);
    }
  }

  const fieldError = (field: string) => error?.messageFor(field);
  const generalError = error && !error.isValidation ? error.message : null;

  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <form
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={project ? "Edit project" : "New project"}
        data-testid="project-modal"
        onSubmit={submit}
      >
        <div className="modal__head">
          <div>
            <h2 className="modal__title">{project ? "Edit project" : "New project"}</h2>
            <div className="modal__sub">
              Projects are the top level — employees, contracts and payroll all sit inside one.
            </div>
          </div>
          <button type="button" className="modal__close" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>

        <div className="modal__body">
          <label className="field field--wide">
            <span className="field__label">Project name</span>
            <input
              ref={firstField}
              className={`field__input${fieldError("name") ? " field__input--invalid" : ""}`}
              name="name"
              value={draft.name}
              placeholder="e.g. Ambatolampy Solar Farm"
              aria-invalid={Boolean(fieldError("name"))}
              onChange={(e) => set("name", e.target.value)}
            />
            {fieldError("name") && (
              <span className="field__error" data-testid="error-name">
                {fieldError("name")}
              </span>
            )}
          </label>

          <label className="field">
            <span className="field__label">Client</span>
            <input
              className={`field__input${fieldError("client") ? " field__input--invalid" : ""}`}
              name="client"
              value={draft.client ?? ""}
              onChange={(e) => set("client", e.target.value)}
            />
            {fieldError("client") && <span className="field__error">{fieldError("client")}</span>}
          </label>

          <label className="field">
            <span className="field__label">Location</span>
            <input
              className="field__input"
              name="location"
              value={draft.location ?? ""}
              onChange={(e) => set("location", e.target.value)}
            />
            {fieldError("location") && (
              <span className="field__error">{fieldError("location")}</span>
            )}
          </label>

          <label className="field">
            <span className="field__label">Status</span>
            <select
              className="field__input"
              name="status"
              value={draft.status}
              onChange={(e) => set("status", e.target.value as ProjectStatus)}
            >
              {PROJECT_STATUSES.map((status) => (
                <option key={status} value={status}>
                  {STATUS_LABELS[status]}
                </option>
              ))}
            </select>
          </label>

          <label className="field">
            <span className="field__label">Start date</span>
            <input
              className="field__input"
              type="date"
              name="start"
              value={draft.start}
              onChange={(e) => set("start", e.target.value)}
            />
          </label>

          <label className="field">
            <span className="field__label">End date</span>
            <input
              className={`field__input${fieldError("end") ? " field__input--invalid" : ""}`}
              type="date"
              name="end"
              value={draft.end ?? ""}
              aria-invalid={Boolean(fieldError("end"))}
              onChange={(e) => set("end", e.target.value || null)}
            />
            {fieldError("end") ? (
              <span className="field__error" data-testid="error-end">
                {fieldError("end")}
              </span>
            ) : (
              <span className="field__help">Leave empty for an open-ended project</span>
            )}
          </label>

          {/* Not in the mockup. The work calendar is a project field by the
              design spec, and payroll derives worked days from it, so it has
              to be settable somewhere. */}
          <div className="field field--wide">
            <span className="field__label" id="working-days-label">
              Working days
            </span>
            <div className="day-toggles" role="group" aria-labelledby="working-days-label">
              {WEEKDAYS.map((day) => (
                <button
                  key={day.bit}
                  type="button"
                  className="day-toggle"
                  aria-pressed={worksOn(draft.workingDays, day.bit)}
                  aria-label={day.long}
                  onClick={() => set("workingDays", toggleWeekday(draft.workingDays, day.bit))}
                >
                  {day.short}
                </button>
              ))}
            </div>
            <span className="field__help">
              Payroll counts worked days from this, minus holidays and leave.
            </span>
          </div>

          <div className="field">
            <span className="field__label" id="day-length-label">
              Standard day
            </span>
            <div className="day-length" role="group" aria-labelledby="day-length-label">
              <input
                className="field__input"
                type="number"
                min={0}
                max={24}
                name="dayLengthHours"
                aria-label="Hours per day"
                value={hours}
                onChange={(e) => set("dayLength", Number(e.target.value) * 60 + minutes)}
              />
              <span className="day-length__unit">h</span>
              <input
                className="field__input"
                type="number"
                min={0}
                max={59}
                step={5}
                name="dayLengthMinutes"
                aria-label="Minutes per day"
                value={minutes}
                onChange={(e) => set("dayLength", hours * 60 + Number(e.target.value))}
              />
              <span className="day-length__unit">min</span>
            </div>
          </div>

          {project && (
            <section className="holidays">
              <span className="field__label">Holidays</span>
              {holidays.length === 0 ? (
                <p className="holidays__empty">
                  No holidays yet. Days added here stop counting as worked days.
                </p>
              ) : (
                <ul className="holidays__list" data-testid="holiday-list">
                  {holidays.map((holiday) => (
                    <li key={holiday.id} className="holidays__row">
                      <span className="holidays__date">{formatDate(holiday.date)}</span>
                      <span className="holidays__name">{holiday.name}</span>
                      <button
                        type="button"
                        className="holidays__remove"
                        aria-label={`Remove ${holiday.name}`}
                        onClick={() => void onRemoveHoliday(holiday.id)}
                      >
                        Remove
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <div className="holidays__add">
                <input
                  className="field__input"
                  type="date"
                  aria-label="Holiday date"
                  value={holidayDraft.date}
                  onChange={(e) => setHolidayDraft({ ...holidayDraft, date: e.target.value })}
                />
                <input
                  className="field__input"
                  aria-label="Holiday name"
                  placeholder="e.g. Independence Day"
                  value={holidayDraft.name}
                  onChange={(e) => setHolidayDraft({ ...holidayDraft, name: e.target.value })}
                />
                <button
                  type="button"
                  className="btn btn--quiet"
                  disabled={!holidayDraft.date}
                  onClick={() => void addHoliday()}
                >
                  Add
                </button>
              </div>
              {holidayError && (
                <span className="field__error" data-testid="error-holiday">
                  {holidayError}
                </span>
              )}
            </section>
          )}
        </div>

        {generalError && (
          <div className="modal__prose">
            <p className="banner" data-testid="modal-error">
              {generalError}
            </p>
          </div>
        )}

        <div className="modal__foot">
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="btn btn--primary" disabled={saving}>
            {project ? "Save project" : "Create project"}
          </button>
        </div>
      </form>
    </div>
  );
}
