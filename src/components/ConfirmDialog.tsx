import { useEffect, useRef } from "react";

interface ConfirmDialogProps {
  title: string;
  body: React.ReactNode;
  confirmLabel: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Not in the mockup, which deletes on the first click. Deleting a project
 * cascades to every employee, contract, leave entry and payslip inside it, so
 * it asks first.
 */
export function ConfirmDialog({
  title,
  body,
  confirmLabel,
  busy = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    cancelRef.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCancel();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onCancel()}>
      <div
        className="modal modal--narrow"
        role="alertdialog"
        aria-modal="true"
        aria-label={title}
        data-testid="confirm-dialog"
      >
        <div className="modal__head">
          <h2 className="modal__title">{title}</h2>
        </div>
        <div className="modal__prose">{body}</div>
        <div className="modal__foot">
          <button type="button" className="btn" ref={cancelRef} onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="btn btn--danger" onClick={onConfirm} disabled={busy}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
