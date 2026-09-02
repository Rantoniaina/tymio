import { Avatar } from "../components/Avatar";
import { ComingSoon } from "../components/ComingSoon";
import { DetailList } from "../components/DetailList";
import { ABSENT, formatDate, formatMonth, formatService, pluralise } from "../format";
import { fullName, initialsOf } from "./EmployeesView";
import type { Employee, EmployeeStats, Project } from "../types";

export type EmployeeTab = "profile" | "contract" | "leaves" | "payments";

const TABS: Array<{ id: EmployeeTab; label: string }> = [
  { id: "profile", label: "Profile" },
  { id: "contract", label: "Contract" },
  { id: "leaves", label: "Leaves" },
  { id: "payments", label: "Payments" },
];

interface EmployeeFileProps {
  employee: Employee;
  stats?: EmployeeStats;
  project: Project;
  tab: EmployeeTab;
  onTab: (tab: EmployeeTab) => void;
  onBack: () => void;
  onEdit: () => void;
}

export function EmployeeFile({
  employee,
  stats,
  project,
  tab,
  onTab,
  onBack,
  onEdit,
}: EmployeeFileProps) {
  const subtitle = [employee.role, employee.phone, employee.email].filter(Boolean).join(" · ");

  return (
    <div data-testid="employee-file" data-employee-name={fullName(employee)}>
      <button type="button" className="link-back" onClick={onBack}>
        ← All employees
      </button>

      <div className="panel person-head">
        <Avatar initials={initialsOf(employee)} seed={employee.id} size={64} />
        <div className="person-head__body">
          <h2 className="person-head__name">{fullName(employee)}</h2>
          <div className="person-head__meta">{subtitle}</div>
          <div className="person-head__tags">
            <span className="tag">Hired {formatDate(employee.hireDate)}</span>
            {stats && (
              <span className="tag" data-testid="tag-service">
                {formatService(stats.monthsOfService)} of service
              </span>
            )}
            {stats?.age != null && (
              <span className="tag" data-testid="tag-age">
                {pluralise(stats.age, "year")} old
              </span>
            )}
          </div>
        </div>
        <div className="person-head__actions">
          <button type="button" className="btn" onClick={onEdit}>
            Edit profile
          </button>
        </div>
      </div>

      <div className="tabs" role="tablist">
        {TABS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="tab"
            className="tab"
            aria-selected={tab === entry.id}
            onClick={() => onTab(entry.id)}
          >
            {entry.label}
          </button>
        ))}
      </div>

      {tab === "profile" && (
        <div className="two-up">
          <DetailList
            title="Personal"
            rows={[
              { label: "First name", value: employee.firstName },
              { label: "Last name", value: employee.lastName },
              { label: "Date of birth", value: employee.birthDate ? formatDate(employee.birthDate) : null },
              { label: "Age", value: stats?.age != null ? pluralise(stats.age, "year") : null },
              { label: "Phone", value: employee.phone },
              { label: "Email", value: employee.email },
              { label: "Address", value: employee.address },
              { label: "Emergency contact", value: employee.emergencyContact },
            ]}
          />
          <DetailList
            title="Employment & admin"
            rows={[
              { label: "National ID (CIN)", value: employee.cin },
              { label: "Project", value: project.name },
              { label: "Role", value: employee.role },
              { label: "Hired on", value: formatDate(employee.hireDate) },
              {
                label: "Service",
                value: stats ? formatService(stats.monthsOfService) : null,
              },
              {
                label: stats ? `Months worked in ${stats.month.year}` : "Months worked",
                value: stats ? String(stats.monthsWorkedThisYear) : null,
              },
              {
                label: "As of",
                value: stats ? `${formatMonth(stats.month)} · ${formatDate(stats.asOf)}` : ABSENT,
              },
              { label: "Bank account", value: employee.bankAccount },
            ]}
          />
        </div>
      )}

      {tab === "contract" && (
        <ComingSoon
          title="No contract yet"
          needs="Pay type, rate, weekly hours, probation and the leave policy arrive with the contracts slice — as effective-dated versions, so an old payslip never changes."
        />
      )}

      {tab === "leaves" && (
        <ComingSoon
          title="No leave yet"
          needs="Requests, the paid/unpaid split and the balance ledger arrive with the leave slice."
        />
      )}

      {tab === "payments" && (
        <ComingSoon
          title="No payslips yet"
          needs="Monthly runs and payslip PDFs arrive with the payroll slice, which needs contracts and leave first."
        />
      )}
    </div>
  );
}
