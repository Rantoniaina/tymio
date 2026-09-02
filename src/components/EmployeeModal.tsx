import { useEffect, useRef, useState, type FormEvent } from "react";

import { AppError } from "../ipc";
import { today } from "../format";
import type { Employee, EmployeeDraft } from "../types";

function draftFrom(employee: Employee | null): EmployeeDraft {
  if (!employee) {
    return {
      firstName: "",
      lastName: "",
      role: "",
      email: "",
      phone: "",
      address: "",
      cin: "",
      birthDate: null,
      hireDate: today(),
      bankAccount: "",
      emergencyContact: "",
    };
  }
  return {
    firstName: employee.firstName,
    lastName: employee.lastName,
    role: employee.role,
    email: employee.email ?? "",
    phone: employee.phone ?? "",
    address: employee.address ?? "",
    cin: employee.cin ?? "",
    birthDate: employee.birthDate,
    hireDate: employee.hireDate,
    bankAccount: employee.bankAccount ?? "",
    emergencyContact: employee.emergencyContact ?? "",
  };
}

/** Blank optional text means "not recorded", which the backend wants as null. */
function clean(draft: EmployeeDraft): EmployeeDraft {
  const orNull = (value: string | null) => (value?.trim() ? value : null);
  return {
    ...draft,
    email: orNull(draft.email),
    phone: orNull(draft.phone),
    address: orNull(draft.address),
    cin: orNull(draft.cin),
    bankAccount: orNull(draft.bankAccount),
    emergencyContact: orNull(draft.emergencyContact),
  };
}

interface EmployeeModalProps {
  /** `null` opens the form for a new hire. */
  employee: Employee | null;
  projectName: string;
  onSave: (draft: EmployeeDraft) => Promise<void>;
  onClose: () => void;
}

export function EmployeeModal({ employee, projectName, onSave, onClose }: EmployeeModalProps) {
  const [draft, setDraft] = useState<EmployeeDraft>(() => draftFrom(employee));
  const [error, setError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
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

  const set = <K extends keyof EmployeeDraft>(key: K, value: EmployeeDraft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await onSave(clean(draft));
    } catch (raw) {
      setError(AppError.from(raw));
    } finally {
      setSaving(false);
    }
  }

  const fieldError = (field: string) => error?.messageFor(field);
  const generalError = error && !error.isValidation ? error.message : null;

  /**
   * Every field is the same shape. The label is associated by `htmlFor` and
   * the hint by `aria-describedby` — nesting them inside one `<label>` would
   * fold the hint into the input's accessible name.
   */
  const text = (
    key: keyof EmployeeDraft,
    label: string,
    options: { wide?: boolean; type?: string; help?: string; placeholder?: string } = {},
  ) => {
    const message = fieldError(key);
    const id = `employee-${key}`;
    const hint = message ? `${id}-error` : options.help ? `${id}-help` : undefined;
    return (
      <div className={`field${options.wide ? " field--wide" : ""}`} key={key}>
        <label className="field__label" htmlFor={id}>
          {label}
        </label>
        <input
          ref={key === "firstName" ? firstField : undefined}
          id={id}
          className={`field__input${message ? " field__input--invalid" : ""}`}
          type={options.type ?? "text"}
          name={key}
          value={(draft[key] as string | null) ?? ""}
          placeholder={options.placeholder}
          aria-invalid={Boolean(message)}
          aria-describedby={hint}
          onChange={(e) =>
            set(key, (e.target.value || (options.type === "date" ? null : "")) as never)
          }
        />
        {message ? (
          <span id={hint} className="field__error" data-testid={`error-${key}`}>
            {message}
          </span>
        ) : (
          options.help && (
            <span id={hint} className="field__help">
              {options.help}
            </span>
          )
        )}
      </div>
    );
  };

  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <form
        className="modal modal--wide"
        role="dialog"
        aria-modal="true"
        aria-label={employee ? "Edit employee" : "Add employee"}
        data-testid="employee-modal"
        onSubmit={submit}
      >
        <div className="modal__head">
          <div>
            <h2 className="modal__title">{employee ? "Edit employee" : "Add employee"}</h2>
            <div className="modal__sub">
              Personal file for {projectName}. Contract terms are set separately.
            </div>
          </div>
          <button type="button" className="modal__close" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>

        <div className="modal__body">
          {text("firstName", "First name")}
          {text("lastName", "Last name")}
          {text("role", "Role / job title", { wide: true, placeholder: "e.g. Site supervisor" })}
          {text("phone", "Phone", { placeholder: "+261 …" })}
          {text("email", "Email")}
          {text("address", "Address", { wide: true })}
          {text("cin", "National ID (CIN)", { help: "Digits only; spaces are ignored" })}
          {text("birthDate", "Date of birth", { type: "date" })}
          {text("hireDate", "Hire date", { type: "date" })}
          {/* Shown on the employee file in the mockup, but with no field to
              set them. The backend stores both, so they are settable here. */}
          {text("bankAccount", "Bank account")}
          {text("emergencyContact", "Emergency contact")}
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
            {employee ? "Save employee" : "Add employee"}
          </button>
        </div>
      </form>
    </div>
  );
}
