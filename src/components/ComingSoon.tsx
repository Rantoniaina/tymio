/**
 * Stands in for a screen whose backend slice does not exist yet.
 *
 * The nav is kept whole on purpose — the shape of the app is part of the
 * design — but a button that silently does nothing is worse than one that
 * says what it is waiting for.
 */
export function ComingSoon({ title, needs }: { title: string; needs: string }) {
  return (
    <div className="placeholder" data-testid="coming-soon">
      <div className="placeholder__title">{title}</div>
      <p>{needs}</p>
    </div>
  );
}
