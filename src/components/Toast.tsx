interface ToastProps {
  message: string | null;
}

/**
 * The mockup's bottom-centre confirmation. Announced politely so a screen
 * reader hears "Project saved" without the focus moving.
 */
export function Toast({ message }: ToastProps) {
  if (!message) return null;
  return (
    <div className="toast" role="status" aria-live="polite" data-testid="toast">
      {message}
    </div>
  );
}
