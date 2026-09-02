import { ABSENT } from "../format";

export interface Detail {
  label: string;
  value: string | null | undefined;
}

/** The label/value rows the mockup uses down the employee file. */
export function DetailList({ title, rows }: { title: string; rows: Detail[] }) {
  return (
    <section className="panel">
      <h3 className="panel__title">{title}</h3>
      <dl className="details">
        {rows.map((row) => (
          <div className="details__row" key={row.label} data-detail={row.label}>
            <dt className="details__label">{row.label}</dt>
            <dd className="details__value">{row.value || ABSENT}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
