import { Avatar } from "../components/Avatar";
import { ABSENT, formatDate } from "../format";
import type { Employee } from "../types";

interface EmployeesViewProps {
  employees: Employee[];
  loading: boolean;
  query: string;
  onQuery: (query: string) => void;
  onAdd: () => void;
  onOpen: (employee: Employee) => void;
  onEdit: (employee: Employee) => void;
  onRemove: (employee: Employee) => void;
}

export function initialsOf(employee: Employee): string {
  const letter = (name: string) => name.charAt(0).toUpperCase();
  return `${letter(employee.firstName)}${letter(employee.lastName)}`;
}

export function fullName(employee: Employee): string {
  return `${employee.firstName} ${employee.lastName}`;
}

export function EmployeesView({
  employees,
  loading,
  query,
  onQuery,
  onAdd,
  onOpen,
  onEdit,
  onRemove,
}: EmployeesViewProps) {
  return (
    <div>
      <div className="toolbar">
        <input
          className="field__input toolbar__search"
          type="search"
          aria-label="Search employees"
          placeholder="Search name, role, CIN…"
          value={query}
          onChange={(e) => onQuery(e.target.value)}
        />
        <div className="toolbar__spacer" />
        <button type="button" className="btn btn--primary" onClick={onAdd}>
          Add employee
        </button>
      </div>

      <div className="table" data-testid="employee-table">
        {/* The mockup's columns are Contract, Rate, Leave left and This month.
            All four are contract, leave or attendance data, none of which
            exists yet; these are the facts the employee record itself holds. */}
        <div className="table__head">
          <div>Employee</div>
          <div>Hired</div>
          <div>National ID</div>
          <div>Contact</div>
          <div />
        </div>

        {employees.map((employee) => (
          <div
            className="table__row"
            key={employee.id}
            data-testid="employee-row"
            data-employee-name={fullName(employee)}
          >
            <div className="cell-person">
              <Avatar initials={initialsOf(employee)} seed={employee.id} />
              <div>
                <div className="cell-person__name">{fullName(employee)}</div>
                <div className="cell-person__role">{employee.role}</div>
              </div>
            </div>
            <div className="cell-mono">{formatDate(employee.hireDate)}</div>
            <div className="cell-mono">{employee.cin ?? ABSENT}</div>
            <div className="cell-contact">
              <div>{employee.phone ?? ABSENT}</div>
              <div className="cell-contact__email">{employee.email ?? ABSENT}</div>
            </div>
            <div className="cell-actions">
              <button type="button" className="btn btn--dark" onClick={() => onOpen(employee)}>
                Open
              </button>
              <button type="button" className="btn btn--quiet" onClick={() => onEdit(employee)}>
                Edit
              </button>
              <button
                type="button"
                className="btn btn--quiet btn--destructive-quiet"
                onClick={() => onRemove(employee)}
              >
                Remove
              </button>
            </div>
          </div>
        ))}

        {employees.length === 0 && !loading && (
          <div className="table__empty" data-testid="employees-empty">
            {query
              ? "No employees match this search."
              : "Nobody on this project yet. Add the first employee."}
          </div>
        )}
      </div>
    </div>
  );
}
