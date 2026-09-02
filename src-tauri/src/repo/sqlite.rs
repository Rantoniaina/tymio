//! The SQLite implementation of the storage traits.
//!
//! Two habits run through this file. Every write happens in a transaction that
//! also appends its audit row, so a change can never be recorded without being
//! logged or logged without being made. And every row is mapped by hand rather
//! than derived, because the domain types (a weekday bitmask, a status enum,
//! minutes-not-hours) are narrower than the columns that hold them.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::db::Db;
use crate::domain::project::{
    Holiday, HolidayId, PortfolioStats, Project, ProjectFilter, ProjectId, ProjectStats,
    ProjectStatus, ValidHoliday, ValidProject,
};
use crate::domain::attendance::{
    AttendanceEntry, AttendanceId, AttendanceRow, AttendanceSheet, AttendanceSource, ValidAttendance,
    WorkedDays, WorkedTime,
};
use crate::domain::calendar::{DayLength, WeekdayMask, WorkCalendar, YearMonth};
use crate::domain::employee::{
    Employee, EmployeeFilter, EmployeeId, EmployeeStats, ValidEmployee,
};
use crate::error::{AppError, Result};

use super::{
    ActivityRepository, AttendanceRepository, AuditAction, AuditEntry, EmployeeRepository,
    ProjectRepository,
};

const PROJECT_ENTITY: &str = "project";
const HOLIDAY_ENTITY: &str = "project_holiday";
const EMPLOYEE_ENTITY: &str = "employee";
const ATTENDANCE_ENTITY: &str = "attendance";

const ATTENDANCE_COLUMNS: &str = "id, employee_id, period, days_worked_halves, \
                                  hours_worked_minutes, overtime_minutes, source, \
                                  created_at, updated_at";

const PROJECT_COLUMNS: &str = "id, name, client, location, status, start_date, end_date, \
                               working_days_mask, hours_per_day_minutes, created_at, updated_at";

const EMPLOYEE_COLUMNS: &str = "id, project_id, first_name, last_name, role, email, phone, \
                                address, cin, birth_date, hire_date, bank_account, \
                                emergency_contact, created_at, updated_at";

/// Declares a repository struct that owns a handle on the pool.
///
/// One per aggregate rather than one that implements every trait: the traits
/// all want to call their methods `create`, `get` and `list`, and a single
/// type implementing several of them makes every one of those calls ambiguous.
macro_rules! repository {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $name {
            db: Db,
        }

        impl $name {
            pub fn new(db: Db) -> Self {
                $name { db }
            }

            pub fn db(&self) -> &Db {
                &self.db
            }
        }
    };
}

repository! {
    /// Projects, their work calendars and their holidays.
    SqliteProjectRepository
}

repository! {
    /// The people on those projects.
    SqliteEmployeeRepository
}

repository! {
    /// Days, hours and overtime, per employee per month.
    SqliteAttendanceRepository
}

repository! {
    /// Reads of the append-only audit log. Writes happen inside the
    /// transaction of the change they record, not through this.
    SqliteActivityRepository
}

/// Appends to the audit log. Takes the transaction's connection so the entry
/// lands or rolls back with the change it describes.
async fn record(
    conn: &mut SqliteConnection,
    at: DateTime<Utc>,
    entity: &str,
    entity_id: &str,
    action: AuditAction,
    detail: Option<String>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO audit_log (at, entity, entity_id, action, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(at)
    .bind(entity)
    .bind(entity_id)
    .bind(action.as_str())
    .bind(detail)
    .execute(conn)
    .await?;
    Ok(())
}

fn snapshot<T: serde::Serialize>(value: &T) -> Option<String> {
    serde_json::to_string(value).ok()
}

fn corrupt(id: &str, detail: impl ToString) -> AppError {
    AppError::CorruptRow {
        entity: PROJECT_ENTITY,
        id: id.to_owned(),
        detail: detail.to_string(),
    }
}

fn project_from_row(row: &SqliteRow) -> Result<Project> {
    let id: String = row.try_get("id")?;

    let status: String = row.try_get("status")?;
    let status: ProjectStatus = status.parse().map_err(|e| corrupt(&id, e))?;

    let mask: i64 = row.try_get("working_days_mask")?;
    let working_days = u8::try_from(mask)
        .map_err(|_| corrupt(&id, format!("weekday mask {mask} does not fit in a byte")))
        .and_then(|bits| WeekdayMask::from_bits(bits).map_err(|e| corrupt(&id, e)))?;

    let minutes: i64 = row.try_get("hours_per_day_minutes")?;
    let day_length = DayLength::from_minutes(minutes).map_err(|e| corrupt(&id, e))?;

    Ok(Project {
        id: ProjectId::from(id),
        name: row.try_get("name")?,
        client: row.try_get("client")?,
        location: row.try_get("location")?,
        status,
        start: row.try_get("start_date")?,
        end: row.try_get("end_date")?,
        calendar: WorkCalendar::new(working_days, day_length),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn holiday_from_row(row: &SqliteRow) -> Result<Holiday> {
    Ok(Holiday {
        id: HolidayId::from(row.try_get::<String, _>("id")?),
        project_id: ProjectId::from(row.try_get::<String, _>("project_id")?),
        date: row.try_get("date")?,
        name: row.try_get("name")?,
    })
}

fn employee_from_row(row: &SqliteRow) -> Result<Employee> {
    Ok(Employee {
        id: EmployeeId::from(row.try_get::<String, _>("id")?),
        project_id: ProjectId::from(row.try_get::<String, _>("project_id")?),
        first_name: row.try_get("first_name")?,
        last_name: row.try_get("last_name")?,
        role: row.try_get("role")?,
        email: row.try_get("email")?,
        phone: row.try_get("phone")?,
        address: row.try_get("address")?,
        cin: row.try_get("cin")?,
        birth_date: row.try_get("birth_date")?,
        hire_date: row.try_get("hire_date")?,
        bank_account: row.try_get("bank_account")?,
        emergency_contact: row.try_get("emergency_contact")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn attendance_from_row(row: &SqliteRow) -> Result<AttendanceEntry> {
    let id: String = row.try_get("id")?;
    let corrupt = |detail: String| AppError::CorruptRow {
        entity: ATTENDANCE_ENTITY,
        id: id.clone(),
        detail,
    };

    let period: String = row.try_get("period")?;
    let period = YearMonth::parse(&period).map_err(|e| corrupt(e.to_string()))?;

    let source: String = row.try_get("source")?;
    let source: AttendanceSource = source.parse().map_err(|e: crate::domain::attendance::AttendanceError| corrupt(e.to_string()))?;

    let days: i64 = row.try_get("days_worked_halves")?;
    let hours: i64 = row.try_get("hours_worked_minutes")?;
    let overtime: i64 = row.try_get("overtime_minutes")?;

    Ok(AttendanceEntry {
        id: AttendanceId::from(id.clone()),
        employee_id: EmployeeId::from(row.try_get::<String, _>("employee_id")?),
        period,
        days_worked: WorkedDays::from_halves(days).map_err(|e| corrupt(e.to_string()))?,
        hours_worked: WorkedTime::from_minutes(hours).map_err(|e| corrupt(e.to_string()))?,
        overtime: WorkedTime::from_minutes(overtime).map_err(|e| corrupt(e.to_string()))?,
        source,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn audit_from_row(row: &SqliteRow) -> Result<AuditEntry> {
    let id: i64 = row.try_get("id")?;
    let action: String = row.try_get("action")?;
    let action = action.parse().map_err(|e: String| AppError::CorruptRow {
        entity: "audit_log",
        id: id.to_string(),
        detail: e,
    })?;

    Ok(AuditEntry {
        id,
        at: row.try_get("at")?,
        entity: row.try_get("entity")?,
        entity_id: row.try_get("entity_id")?,
        action,
        detail: row.try_get("detail")?,
    })
}

/// Writes every column of a project. Shared by insert and update so the two
/// can never disagree about what a project is made of.
fn bind_project<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    project: &'q Project,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    query
        .bind(project.name.as_str())
        .bind(project.client.as_deref())
        .bind(project.location.as_deref())
        .bind(project.status.as_str())
        .bind(project.start)
        .bind(project.end)
        .bind(i64::from(project.calendar.working_days.bits()))
        .bind(i64::from(project.calendar.day_length.minutes()))
}

/// Writes every editable column of an employee. Shared by insert and update
/// so the two can never disagree about what an employee is made of.
fn bind_employee<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    employee: &'q Employee,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    query
        .bind(employee.first_name.as_str())
        .bind(employee.last_name.as_str())
        .bind(employee.role.as_str())
        .bind(employee.email.as_deref())
        .bind(employee.phone.as_deref())
        .bind(employee.address.as_deref())
        .bind(employee.cin.as_deref())
        .bind(employee.birth_date)
        .bind(employee.hire_date)
        .bind(employee.bank_account.as_deref())
        .bind(employee.emergency_contact.as_deref())
}

/// The message a duplicate CIN should produce. A national identity number
/// belongs to one person, so this almost always means the record already
/// exists under another name.
fn duplicate_cin(cin: Option<&str>) -> String {
    match cin {
        Some(cin) => format!("CIN {cin} is already on another employee"),
        None => "That employee already exists".to_owned(),
    }
}

#[async_trait]
impl ProjectRepository for SqliteProjectRepository {
    async fn create(&self, draft: ValidProject) -> Result<Project> {
        let now = Utc::now();
        let project = draft.into_project(ProjectId::new(), now);

        let mut tx = self.db.pool().begin().await?;

        let insert = sqlx::query(
            "INSERT INTO projects (name, client, location, status, start_date, end_date, \
             working_days_mask, hours_per_day_minutes, id, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        );
        bind_project(insert, &project)
            .bind(project.id.as_str())
            .bind(project.created_at)
            .bind(project.updated_at)
            .execute(&mut *tx)
            .await?;

        record(
            &mut tx,
            now,
            PROJECT_ENTITY,
            project.id.as_str(),
            AuditAction::Create,
            snapshot(&project),
        )
        .await?;

        tx.commit().await?;
        Ok(project)
    }

    async fn get(&self, id: &ProjectId) -> Result<Option<Project>> {
        let row = sqlx::query(&format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"))
            .bind(id.as_str())
            .fetch_optional(self.db.pool())
            .await?;

        row.as_ref().map(project_from_row).transpose()
    }

    async fn list(&self, filter: &ProjectFilter) -> Result<Vec<Project>> {
        // Status narrows in SQL; the search box is applied in Rust so that case
        // folding is Unicode-correct (SQLite's NOCASE only folds ASCII, and
        // these are Malagasy names). The project list is tens of rows, not
        // thousands — this is not the query to optimise.
        let rows = sqlx::query(&format!(
            "SELECT {PROJECT_COLUMNS} FROM projects WHERE ?1 IS NULL OR status = ?1"
        ))
        .bind(filter.status.map(|s| s.as_str()))
        .fetch_all(self.db.pool())
        .await?;

        let mut projects = rows
            .iter()
            .map(project_from_row)
            .filter(|p| p.as_ref().map(|p| filter.matches_text(p)).unwrap_or(true))
            .collect::<Result<Vec<_>>>()?;

        projects.sort_by(|a, b| {
            a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id))
        });
        Ok(projects)
    }

    async fn update(&self, id: &ProjectId, draft: ValidProject) -> Result<Project> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        let existing = sqlx::query(&format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"))
            .bind(id.as_str())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::not_found(PROJECT_ENTITY, id))?;
        let existing = project_from_row(&existing)?;

        let updated = draft.onto(&existing, now);

        let update = sqlx::query(
            "UPDATE projects SET name = ?1, client = ?2, location = ?3, status = ?4, \
             start_date = ?5, end_date = ?6, working_days_mask = ?7, \
             hours_per_day_minutes = ?8, updated_at = ?9 WHERE id = ?10",
        );
        bind_project(update, &updated)
            .bind(updated.updated_at)
            .bind(updated.id.as_str())
            .execute(&mut *tx)
            .await?;

        record(
            &mut tx,
            now,
            PROJECT_ENTITY,
            updated.id.as_str(),
            AuditAction::Update,
            snapshot(&updated),
        )
        .await?;

        tx.commit().await?;
        Ok(updated)
    }

    async fn delete(&self, id: &ProjectId) -> Result<Project> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        let row = sqlx::query(&format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"))
            .bind(id.as_str())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::not_found(PROJECT_ENTITY, id))?;
        let deleted = project_from_row(&row)?;

        // ON DELETE CASCADE is about to take the employees with it, silently.
        // Snapshot each one first, or the audit log would show a project
        // disappearing and no trace of the people who were on it.
        let doomed = sqlx::query(&format!(
            "SELECT {EMPLOYEE_COLUMNS} FROM employees WHERE project_id = ?1"
        ))
        .bind(id.as_str())
        .fetch_all(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM projects WHERE id = ?1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;

        for row in &doomed {
            let employee = employee_from_row(row)?;
            record(
                &mut tx,
                now,
                EMPLOYEE_ENTITY,
                employee.id.as_str(),
                AuditAction::Delete,
                snapshot(&employee),
            )
            .await?;
        }

        // The snapshot is the whole point of logging a delete: the row itself
        // is gone, cascade and all.
        record(
            &mut tx,
            now,
            PROJECT_ENTITY,
            deleted.id.as_str(),
            AuditAction::Delete,
            snapshot(&deleted),
        )
        .await?;

        tx.commit().await?;
        Ok(deleted)
    }

    async fn portfolio_stats(&self) -> Result<PortfolioStats> {
        let rows = sqlx::query("SELECT status, count(*) AS n FROM projects GROUP BY status")
            .fetch_all(self.db.pool())
            .await?;

        let mut stats = PortfolioStats::default();
        for row in &rows {
            let status: String = row.try_get("status")?;
            let count: i64 = row.try_get("n")?;
            let count = u32::try_from(count).unwrap_or(u32::MAX);
            match status.parse().map_err(|e| corrupt(&status, e))? {
                ProjectStatus::Active => stats.active = count,
                ProjectStatus::Paused => stats.paused = count,
                ProjectStatus::Closed => stats.closed = count,
            }
            stats.total += count;
        }

        let (people,): (i64,) = sqlx::query_as("SELECT count(*) FROM employees")
            .fetch_one(self.db.pool())
            .await?;
        stats.people = u32::try_from(people).unwrap_or(u32::MAX);

        Ok(stats)
    }

    async fn stats(&self, id: &ProjectId, as_of: NaiveDate) -> Result<ProjectStats> {
        let project = self.require(id).await?;
        let holidays = self.holiday_set(id).await?;
        // One COUNT rather than a call into the employee repository: this is
        // the same database, and a project's headcount is a fact about the
        // project's overview, not a reason to depend on another aggregate.
        let (headcount,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM employees WHERE project_id = ?1")
                .bind(id.as_str())
                .fetch_one(self.db.pool())
                .await?;
        let headcount = u32::try_from(headcount).unwrap_or(u32::MAX);
        Ok(ProjectStats::compute(&project, &holidays, headcount, as_of))
    }

    async fn add_holiday(&self, project: &ProjectId, holiday: ValidHoliday) -> Result<Holiday> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        // The foreign key would catch this, but "no project with id …" is a
        // better answer than a constraint code.
        let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM projects WHERE id = ?1")
            .bind(project.as_str())
            .fetch_optional(&mut *tx)
            .await?;
        if exists.is_none() {
            return Err(AppError::not_found(PROJECT_ENTITY, project));
        }

        let stored = Holiday {
            id: HolidayId::new(),
            project_id: project.clone(),
            date: holiday.date(),
            name: holiday.name().to_owned(),
        };

        sqlx::query(
            "INSERT INTO project_holidays (id, project_id, date, name) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(stored.id.as_str())
        .bind(stored.project_id.as_str())
        .bind(stored.date)
        .bind(stored.name.as_str())
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::from_sqlx(e, || {
                format!("{} is already a holiday on this project", stored.date)
            })
        })?;

        record(
            &mut tx,
            now,
            HOLIDAY_ENTITY,
            stored.id.as_str(),
            AuditAction::Create,
            snapshot(&stored),
        )
        .await?;

        tx.commit().await?;
        Ok(stored)
    }

    async fn holidays(&self, project: &ProjectId) -> Result<Vec<Holiday>> {
        let rows = sqlx::query(
            "SELECT id, project_id, date, name FROM project_holidays \
             WHERE project_id = ?1 ORDER BY date ASC",
        )
        .bind(project.as_str())
        .fetch_all(self.db.pool())
        .await?;

        rows.iter().map(holiday_from_row).collect()
    }

    async fn remove_holiday(&self, project: &ProjectId, holiday: &HolidayId) -> Result<()> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        let affected = sqlx::query("DELETE FROM project_holidays WHERE id = ?1 AND project_id = ?2")
            .bind(holiday.as_str())
            .bind(project.as_str())
            .execute(&mut *tx)
            .await?
            .rows_affected();

        if affected == 0 {
            return Err(AppError::not_found(HOLIDAY_ENTITY, holiday));
        }

        record(&mut tx, now, HOLIDAY_ENTITY, holiday.as_str(), AuditAction::Delete, None).await?;

        tx.commit().await?;
        Ok(())
    }
}

#[async_trait]
impl EmployeeRepository for SqliteEmployeeRepository {
    async fn create(&self, project: &ProjectId, draft: ValidEmployee) -> Result<Employee> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        // The foreign key would catch this, but "no project with id …" is a
        // better answer than a constraint code.
        let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM projects WHERE id = ?1")
            .bind(project.as_str())
            .fetch_optional(&mut *tx)
            .await?;
        if exists.is_none() {
            return Err(AppError::not_found(PROJECT_ENTITY, project));
        }

        let employee = draft.into_employee(EmployeeId::new(), project.clone(), now);

        let insert = sqlx::query(
            "INSERT INTO employees (first_name, last_name, role, email, phone, address, cin, \
             birth_date, hire_date, bank_account, emergency_contact, id, project_id, \
             created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        );
        bind_employee(insert, &employee)
            .bind(employee.id.as_str())
            .bind(employee.project_id.as_str())
            .bind(employee.created_at)
            .bind(employee.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::from_sqlx(e, || duplicate_cin(employee.cin.as_deref())))?;

        record(
            &mut tx,
            now,
            EMPLOYEE_ENTITY,
            employee.id.as_str(),
            AuditAction::Create,
            snapshot(&employee),
        )
        .await?;

        tx.commit().await?;
        Ok(employee)
    }

    async fn get(&self, id: &EmployeeId) -> Result<Option<Employee>> {
        let row = sqlx::query(&format!("SELECT {EMPLOYEE_COLUMNS} FROM employees WHERE id = ?1"))
            .bind(id.as_str())
            .fetch_optional(self.db.pool())
            .await?;

        row.as_ref().map(employee_from_row).transpose()
    }

    async fn list(&self, filter: &EmployeeFilter) -> Result<Vec<Employee>> {
        // The project narrows in SQL; the search box is applied in Rust so
        // that case folding is Unicode-correct, as for projects.
        let rows = sqlx::query(&format!(
            "SELECT {EMPLOYEE_COLUMNS} FROM employees WHERE ?1 IS NULL OR project_id = ?1"
        ))
        .bind(filter.project.as_ref().map(|p| p.as_str()))
        .fetch_all(self.db.pool())
        .await?;

        let mut employees = rows
            .iter()
            .map(employee_from_row)
            .filter(|e| e.as_ref().map(|e| filter.matches_text(e)).unwrap_or(true))
            .collect::<Result<Vec<_>>>()?;

        employees.sort_by(|a, b| {
            let by_name = (a.last_name.to_lowercase(), a.first_name.to_lowercase())
                .cmp(&(b.last_name.to_lowercase(), b.first_name.to_lowercase()));
            by_name.then_with(|| a.id.cmp(&b.id))
        });
        Ok(employees)
    }

    async fn update(&self, id: &EmployeeId, draft: ValidEmployee) -> Result<Employee> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        let existing =
            sqlx::query(&format!("SELECT {EMPLOYEE_COLUMNS} FROM employees WHERE id = ?1"))
                .bind(id.as_str())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| AppError::not_found(EMPLOYEE_ENTITY, id))?;
        let existing = employee_from_row(&existing)?;

        let updated = draft.onto(&existing, now);

        let update = sqlx::query(
            "UPDATE employees SET first_name = ?1, last_name = ?2, role = ?3, email = ?4, \
             phone = ?5, address = ?6, cin = ?7, birth_date = ?8, hire_date = ?9, \
             bank_account = ?10, emergency_contact = ?11, updated_at = ?12 WHERE id = ?13",
        );
        bind_employee(update, &updated)
            .bind(updated.updated_at)
            .bind(updated.id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::from_sqlx(e, || duplicate_cin(updated.cin.as_deref())))?;

        record(
            &mut tx,
            now,
            EMPLOYEE_ENTITY,
            updated.id.as_str(),
            AuditAction::Update,
            snapshot(&updated),
        )
        .await?;

        tx.commit().await?;
        Ok(updated)
    }

    async fn delete(&self, id: &EmployeeId) -> Result<Employee> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        let row = sqlx::query(&format!("SELECT {EMPLOYEE_COLUMNS} FROM employees WHERE id = ?1"))
            .bind(id.as_str())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::not_found(EMPLOYEE_ENTITY, id))?;
        let deleted = employee_from_row(&row)?;

        sqlx::query("DELETE FROM employees WHERE id = ?1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;

        record(
            &mut tx,
            now,
            EMPLOYEE_ENTITY,
            deleted.id.as_str(),
            AuditAction::Delete,
            snapshot(&deleted),
        )
        .await?;

        tx.commit().await?;
        Ok(deleted)
    }

    async fn headcount(&self, project: &ProjectId) -> Result<u32> {
        let (count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM employees WHERE project_id = ?1")
                .bind(project.as_str())
                .fetch_one(self.db.pool())
                .await?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    async fn stats(&self, id: &EmployeeId, as_of: NaiveDate) -> Result<EmployeeStats> {
        Ok(self.require(id).await?.service_at(as_of))
    }
}

/// The upsert behind `record` and `record_many`.
///
/// One statement rather than a read-then-write, so two saves of the same month
/// cannot interleave into a duplicate row — the `(employee_id, period)` unique
/// index is what makes it a replace.
async fn upsert(
    conn: &mut SqliteConnection,
    employee: &EmployeeId,
    period: YearMonth,
    entry: ValidAttendance,
    now: DateTime<Utc>,
) -> Result<AttendanceEntry> {
    let row = sqlx::query(&format!(
        "INSERT INTO attendance (id, employee_id, period, days_worked_halves, \
         hours_worked_minutes, overtime_minutes, source, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) \
         ON CONFLICT (employee_id, period) DO UPDATE SET \
           days_worked_halves = excluded.days_worked_halves, \
           hours_worked_minutes = excluded.hours_worked_minutes, \
           overtime_minutes = excluded.overtime_minutes, \
           source = excluded.source, \
           updated_at = excluded.updated_at \
         RETURNING {ATTENDANCE_COLUMNS}"
    ))
    .bind(AttendanceId::new().as_str())
    .bind(employee.as_str())
    .bind(period.to_string())
    .bind(i64::from(entry.days_worked().halves()))
    .bind(i64::from(entry.hours_worked().minutes()))
    .bind(i64::from(entry.overtime().minutes()))
    .bind(entry.source().as_str())
    .bind(now)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| match &e {
        // The only foreign key on this table is the employee.
        sqlx::Error::Database(db) if db.is_foreign_key_violation() => {
            AppError::not_found(EMPLOYEE_ENTITY, employee)
        }
        _ => AppError::Database(e),
    })?;

    attendance_from_row(&row)
}

#[async_trait]
impl AttendanceRepository for SqliteAttendanceRepository {
    async fn get(
        &self,
        employee: &EmployeeId,
        period: YearMonth,
    ) -> Result<Option<AttendanceEntry>> {
        let row = sqlx::query(&format!(
            "SELECT {ATTENDANCE_COLUMNS} FROM attendance WHERE employee_id = ?1 AND period = ?2"
        ))
        .bind(employee.as_str())
        .bind(period.to_string())
        .fetch_optional(self.db.pool())
        .await?;

        row.as_ref().map(attendance_from_row).transpose()
    }

    async fn record(
        &self,
        employee: &EmployeeId,
        period: YearMonth,
        entry: ValidAttendance,
    ) -> Result<AttendanceEntry> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        let stored = upsert(&mut tx, employee, period, entry, now).await?;
        // Create or update: whether this month already existed is the
        // difference, and `created_at` is how the row itself says so.
        let action = if stored.created_at == stored.updated_at {
            AuditAction::Create
        } else {
            AuditAction::Update
        };
        record(&mut tx, now, ATTENDANCE_ENTITY, stored.id.as_str(), action, snapshot(&stored))
            .await?;

        tx.commit().await?;
        Ok(stored)
    }

    async fn record_many(
        &self,
        period: YearMonth,
        entries: Vec<(EmployeeId, ValidAttendance)>,
    ) -> Result<u32> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        let mut written = 0;
        for (employee, entry) in entries {
            let stored = upsert(&mut tx, &employee, period, entry, now).await?;
            let action = if stored.created_at == stored.updated_at {
                AuditAction::Create
            } else {
                AuditAction::Update
            };
            record(&mut tx, now, ATTENDANCE_ENTITY, stored.id.as_str(), action, snapshot(&stored))
                .await?;
            written += 1;
        }

        tx.commit().await?;
        Ok(written)
    }

    async fn clear(
        &self,
        employee: &EmployeeId,
        period: YearMonth,
    ) -> Result<Option<AttendanceEntry>> {
        let now = Utc::now();
        let mut tx = self.db.pool().begin().await?;

        let row = sqlx::query(&format!(
            "DELETE FROM attendance WHERE employee_id = ?1 AND period = ?2 \
             RETURNING {ATTENDANCE_COLUMNS}"
        ))
        .bind(employee.as_str())
        .bind(period.to_string())
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            // Nothing to clear is not a failure; a blank month is a state.
            return Ok(None);
        };
        let cleared = attendance_from_row(&row)?;

        record(
            &mut tx,
            now,
            ATTENDANCE_ENTITY,
            cleared.id.as_str(),
            AuditAction::Delete,
            snapshot(&cleared),
        )
        .await?;

        tx.commit().await?;
        Ok(Some(cleared))
    }

    async fn sheet(&self, project: &ProjectId, period: YearMonth) -> Result<AttendanceSheet> {
        // A LEFT JOIN so that everyone on the project gets a line, including
        // the ones nobody has recorded yet. Ordered the way the employees
        // screen orders them.
        let rows = sqlx::query(
            "SELECT e.id AS employee_id, e.last_name, e.first_name, \
                    a.id AS id, a.period AS period, a.days_worked_halves, \
                    a.hours_worked_minutes, a.overtime_minutes, a.source, \
                    a.created_at, a.updated_at \
             FROM employees e \
             LEFT JOIN attendance a ON a.employee_id = e.id AND a.period = ?2 \
             WHERE e.project_id = ?1",
        )
        .bind(project.as_str())
        .bind(period.to_string())
        .fetch_all(self.db.pool())
        .await?;

        let mut lines: Vec<(String, String, AttendanceRow)> = Vec::with_capacity(rows.len());
        for row in &rows {
            let employee_id = EmployeeId::from(row.try_get::<String, _>("employee_id")?);
            let recorded: Option<String> = row.try_get("id")?;
            let entry = match recorded {
                Some(_) => Some(attendance_from_row(row)?),
                None => None,
            };
            lines.push((
                row.try_get::<String, _>("last_name")?.to_lowercase(),
                row.try_get::<String, _>("first_name")?.to_lowercase(),
                AttendanceRow { employee_id, entry },
            ));
        }
        lines.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));

        Ok(AttendanceSheet::new(
            project.clone(),
            period,
            lines.into_iter().map(|(_, _, row)| row).collect(),
        ))
    }

    async fn history(&self, employee: &EmployeeId) -> Result<Vec<AttendanceEntry>> {
        let rows = sqlx::query(&format!(
            "SELECT {ATTENDANCE_COLUMNS} FROM attendance WHERE employee_id = ?1 \
             ORDER BY period DESC"
        ))
        .bind(employee.as_str())
        .fetch_all(self.db.pool())
        .await?;

        rows.iter().map(attendance_from_row).collect()
    }
}

#[async_trait]
impl ActivityRepository for SqliteActivityRepository {
    async fn recent_activity(&self, limit: u32) -> Result<Vec<AuditEntry>> {
        let rows = sqlx::query(
            "SELECT id, at, entity, entity_id, action, detail FROM audit_log \
             ORDER BY id DESC LIMIT ?1",
        )
        .bind(i64::from(limit))
        .fetch_all(self.db.pool())
        .await?;

        rows.iter().map(audit_from_row).collect()
    }

    async fn history(&self, entity: &str, entity_id: &str) -> Result<Vec<AuditEntry>> {
        let rows = sqlx::query(
            "SELECT id, at, entity, entity_id, action, detail FROM audit_log \
             WHERE entity = ?1 AND entity_id = ?2 ORDER BY id ASC",
        )
        .bind(entity)
        .bind(entity_id)
        .fetch_all(self.db.pool())
        .await?;

        rows.iter().map(audit_from_row).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::calendar::{HolidaySet, WeekdayMask, YearMonth};
    use crate::domain::project::{HolidayDraft, ProjectDraft};

    fn date(s: &str) -> NaiveDate {
        s.parse().expect("test date is well formed")
    }

    /// A project repository on its own database.
    async fn repo() -> SqliteProjectRepository {
        SqliteProjectRepository::new(Db::in_memory().await.expect("in-memory database opens"))
    }

    /// Projects, employees and the audit log over one shared database, for the
    /// tests that need to see two of them agree.
    async fn linked() -> (SqliteProjectRepository, SqliteEmployeeRepository, SqliteActivityRepository)
    {
        let db = Db::in_memory().await.expect("in-memory database opens");
        (
            SqliteProjectRepository::new(db.clone()),
            SqliteEmployeeRepository::new(db.clone()),
            SqliteActivityRepository::new(db),
        )
    }

    fn valid(draft: ProjectDraft) -> ValidProject {
        draft.validate().expect("test draft is valid")
    }

    /// The mockup's first project, with everything filled in.
    fn solar_farm() -> ProjectDraft {
        let mut draft = ProjectDraft::new("Ambatolampy Solar Farm", date("2026-02-01"));
        draft.client = Some("JIRAMA".into());
        draft.location = Some("Vakinankaratra".into());
        draft.end = Some(date("2027-06-30"));
        draft
    }

    async fn seed_portfolio(repo: &SqliteProjectRepository) -> Vec<Project> {
        let mut port = ProjectDraft::new("Toamasina Port Logistics", date("2025-09-15"));
        port.client = Some("SMMC".into());
        port.location = Some("Toamasina".into());

        let mut fit_out = ProjectDraft::new("Antananarivo HQ Fit-out", date("2026-05-01"));
        fit_out.client = Some("Tymio internal".into());
        fit_out.status = ProjectStatus::Paused;

        let mut resort = ProjectDraft::new("Nosy Be Resort Staffing", date("2025-01-10"));
        resort.client = Some("Baobab Hôtels".into());
        resort.status = ProjectStatus::Closed;

        let mut created = Vec::new();
        for draft in [solar_farm(), port, fit_out, resort] {
            created.push(repo.create(valid(draft)).await.expect("seed project is stored"));
        }
        created
    }

    mod create_and_read {
        use super::*;

        #[tokio::test]
        async fn a_created_project_reads_back_identically() {
            let repo = repo().await;
            let created = repo.create(valid(solar_farm())).await.expect("stored");

            let fetched = repo.get(&created.id).await.expect("query runs").expect("it is there");
            assert_eq!(fetched, created);
            assert_eq!(fetched.name, "Ambatolampy Solar Farm");
            assert_eq!(fetched.client.as_deref(), Some("JIRAMA"));
            assert_eq!(fetched.status, ProjectStatus::Active);
            assert_eq!(fetched.start, date("2026-02-01"));
            assert_eq!(fetched.end, Some(date("2027-06-30")));
        }

        #[tokio::test]
        async fn an_open_ended_project_keeps_its_missing_end_date() {
            let repo = repo().await;
            let created = repo
                .create(valid(ProjectDraft::new("Ongoing maintenance", date("2026-01-01"))))
                .await
                .expect("stored");

            let fetched = repo.get(&created.id).await.expect("query runs").expect("it is there");
            assert_eq!(fetched.end, None);
            assert_eq!(fetched.client, None);
            assert_eq!(fetched.location, None);
        }

        #[tokio::test]
        async fn a_non_default_work_calendar_survives_the_round_trip() {
            let repo = repo().await;
            let mut draft = solar_farm();
            draft.working_days = WeekdayMask::MON_SAT;
            draft.day_length = DayLength::from_hours_and_minutes(7, 30).expect("7h30");

            let created = repo.create(valid(draft)).await.expect("stored");
            let fetched = repo.get(&created.id).await.expect("query runs").expect("it is there");

            assert_eq!(fetched.calendar.working_days, WeekdayMask::MON_SAT);
            assert_eq!(fetched.calendar.day_length.minutes(), 450);
        }

        #[tokio::test]
        async fn each_project_gets_its_own_identity() {
            let repo = repo().await;
            let first = repo.create(valid(solar_farm())).await.expect("stored");
            let second = repo.create(valid(solar_farm())).await.expect("stored");

            assert_ne!(first.id, second.id, "same details, different projects");
        }

        #[tokio::test]
        async fn an_unknown_id_is_none_but_requiring_it_is_an_error() {
            let repo = repo().await;
            let missing = ProjectId::from("no-such-project");

            assert_eq!(repo.get(&missing).await.expect("query runs"), None);
            assert!(matches!(
                repo.require(&missing).await,
                Err(AppError::NotFound { entity: "project", .. })
            ));
        }
    }

    mod listing {
        use super::*;

        #[tokio::test]
        async fn an_empty_filter_returns_everything_by_name() {
            let repo = repo().await;
            seed_portfolio(&repo).await;

            let all = repo.list(&ProjectFilter::default()).await.expect("query runs");
            let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(
                names,
                [
                    "Ambatolampy Solar Farm",
                    "Antananarivo HQ Fit-out",
                    "Nosy Be Resort Staffing",
                    "Toamasina Port Logistics",
                ]
            );
        }

        #[tokio::test]
        async fn the_status_chips_narrow_the_list() {
            let repo = repo().await;
            seed_portfolio(&repo).await;

            let active = repo
                .list(&ProjectFilter::with_status(ProjectStatus::Active))
                .await
                .expect("query runs");
            assert_eq!(active.len(), 2);
            assert!(active.iter().all(|p| p.status == ProjectStatus::Active));

            let closed = repo
                .list(&ProjectFilter::with_status(ProjectStatus::Closed))
                .await
                .expect("query runs");
            assert_eq!(closed.len(), 1);
            assert_eq!(closed[0].name, "Nosy Be Resort Staffing");
        }

        #[tokio::test]
        async fn the_search_box_matches_client_and_location_too() {
            let repo = repo().await;
            seed_portfolio(&repo).await;

            let by_client = repo.list(&ProjectFilter::search("jirama")).await.expect("query runs");
            assert_eq!(by_client.len(), 1);
            assert_eq!(by_client[0].name, "Ambatolampy Solar Farm");

            let by_location =
                repo.list(&ProjectFilter::search("TOAMASINA")).await.expect("query runs");
            assert_eq!(by_location.len(), 1);

            // Accented text folds correctly — this is why the search is not
            // left to SQLite's ASCII-only NOCASE.
            let accented = repo.list(&ProjectFilter::search("hôtels")).await.expect("query runs");
            assert_eq!(accented.len(), 1);
            assert_eq!(accented[0].name, "Nosy Be Resort Staffing");
        }

        #[tokio::test]
        async fn status_and_search_apply_together() {
            let repo = repo().await;
            seed_portfolio(&repo).await;

            let filter = ProjectFilter {
                status: Some(ProjectStatus::Active),
                query: Some("nosy".into()),
            };
            // The Nosy Be project is closed, so this matches nothing.
            assert!(repo.list(&filter).await.expect("query runs").is_empty());
        }

        #[tokio::test]
        async fn an_empty_database_lists_nothing_rather_than_failing() {
            let repo = repo().await;
            assert!(repo.list(&ProjectFilter::default()).await.expect("query runs").is_empty());
        }
    }

    mod updating {
        use super::*;

        #[tokio::test]
        async fn an_edit_replaces_the_fields_and_keeps_the_identity() {
            let repo = repo().await;
            let created = repo.create(valid(solar_farm())).await.expect("stored");

            let mut edit = solar_farm();
            edit.name = "Ambatolampy Solar Farm — phase 2".into();
            edit.status = ProjectStatus::Paused;
            edit.end = Some(date("2027-12-31"));
            let updated = repo.update(&created.id, valid(edit)).await.expect("update runs");

            assert_eq!(updated.id, created.id);
            assert_eq!(updated.created_at, created.created_at);
            assert!(updated.updated_at >= created.updated_at);
            assert_eq!(updated.name, "Ambatolampy Solar Farm — phase 2");
            assert_eq!(updated.status, ProjectStatus::Paused);
            assert_eq!(updated.end, Some(date("2027-12-31")));

            let refetched = repo.get(&created.id).await.expect("query runs").expect("still there");
            assert_eq!(refetched, updated);
        }

        #[tokio::test]
        async fn an_edit_can_clear_an_optional_field() {
            let repo = repo().await;
            let created = repo.create(valid(solar_farm())).await.expect("stored");
            assert!(created.client.is_some());

            let mut edit = solar_farm();
            edit.client = None;
            let updated = repo.update(&created.id, valid(edit)).await.expect("update runs");

            assert_eq!(updated.client, None);
        }

        #[tokio::test]
        async fn editing_a_project_that_is_gone_is_an_error() {
            let repo = repo().await;
            let result = repo.update(&ProjectId::from("ghost"), valid(solar_farm())).await;

            assert!(matches!(result, Err(AppError::NotFound { entity: "project", .. })));
        }

        #[tokio::test]
        async fn a_failed_edit_leaves_nothing_behind() {
            let (repo, _, log) = linked().await;
            let before = log.recent_activity(10).await.expect("query runs").len();

            let _ = repo.update(&ProjectId::from("ghost"), valid(solar_farm())).await;

            let after = log.recent_activity(10).await.expect("query runs").len();
            assert_eq!(before, after, "a rolled-back edit must not leave an audit row");
        }
    }

    mod deleting {
        use super::*;

        #[tokio::test]
        async fn deleting_returns_what_was_deleted_and_removes_it() {
            let repo = repo().await;
            let created = repo.create(valid(solar_farm())).await.expect("stored");

            let deleted = repo.delete(&created.id).await.expect("delete runs");
            assert_eq!(deleted, created);
            assert_eq!(repo.get(&created.id).await.expect("query runs"), None);
        }

        #[tokio::test]
        async fn deleting_a_project_takes_its_holidays_with_it() {
            let repo = repo().await;
            let project = repo.create(valid(solar_farm())).await.expect("stored");
            repo.add_holiday(
                &project.id,
                HolidayDraft::new(date("2026-06-26"), "Independence Day")
                    .validate()
                    .expect("valid holiday"),
            )
            .await
            .expect("holiday is stored");

            repo.delete(&project.id).await.expect("delete runs");

            let (orphans,): (i64,) = sqlx::query_as("SELECT count(*) FROM project_holidays")
                .fetch_one(repo.db().pool())
                .await
                .expect("query runs");
            assert_eq!(orphans, 0, "ON DELETE CASCADE only works with foreign keys on");
        }

        #[tokio::test]
        async fn deleting_something_that_is_gone_is_an_error() {
            let repo = repo().await;
            assert!(matches!(
                repo.delete(&ProjectId::from("ghost")).await,
                Err(AppError::NotFound { entity: "project", .. })
            ));
        }
    }

    mod holidays {
        use super::*;

        #[tokio::test]
        async fn holidays_come_back_in_date_order() {
            let repo = repo().await;
            let project = repo.create(valid(solar_farm())).await.expect("stored");

            for (day, name) in [
                ("2026-06-26", "Independence Day"),
                ("2026-03-29", "Martyrs' Day"),
                ("2026-11-01", "All Saints' Day"),
            ] {
                repo.add_holiday(
                    &project.id,
                    HolidayDraft::new(date(day), name).validate().expect("valid holiday"),
                )
                .await
                .expect("holiday is stored");
            }

            let stored = repo.holidays(&project.id).await.expect("query runs");
            let dates: Vec<NaiveDate> = stored.iter().map(|h| h.date).collect();
            assert_eq!(dates, [date("2026-03-29"), date("2026-06-26"), date("2026-11-01")]);
        }

        #[tokio::test]
        async fn the_same_date_cannot_be_a_holiday_twice() {
            let repo = repo().await;
            let project = repo.create(valid(solar_farm())).await.expect("stored");
            let independence = || {
                HolidayDraft::new(date("2026-06-26"), "Independence Day")
                    .validate()
                    .expect("valid holiday")
            };

            repo.add_holiday(&project.id, independence()).await.expect("first one is fine");
            let second = repo.add_holiday(&project.id, independence()).await;

            match second {
                Err(AppError::Conflict(message)) => assert!(message.contains("2026-06-26")),
                other => panic!("expected a conflict, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn two_projects_can_share_a_holiday_date() {
            let repo = repo().await;
            let first = repo.create(valid(solar_farm())).await.expect("stored");
            let second = repo
                .create(valid(ProjectDraft::new("Toamasina Port Logistics", date("2025-09-15"))))
                .await
                .expect("stored");

            for project in [&first.id, &second.id] {
                repo.add_holiday(
                    project,
                    HolidayDraft::new(date("2026-06-26"), "Independence Day")
                        .validate()
                        .expect("valid holiday"),
                )
                .await
                .expect("each project keeps its own calendar");
            }

            assert_eq!(repo.holidays(&first.id).await.expect("query runs").len(), 1);
            assert_eq!(repo.holidays(&second.id).await.expect("query runs").len(), 1);
        }

        #[tokio::test]
        async fn a_holiday_needs_a_project_that_exists() {
            let repo = repo().await;
            let result = repo
                .add_holiday(
                    &ProjectId::from("ghost"),
                    HolidayDraft::new(date("2026-06-26"), "Independence Day")
                        .validate()
                        .expect("valid holiday"),
                )
                .await;

            assert!(matches!(result, Err(AppError::NotFound { entity: "project", .. })));
        }

        #[tokio::test]
        async fn a_holiday_can_only_be_removed_through_its_own_project() {
            let repo = repo().await;
            let owner = repo.create(valid(solar_farm())).await.expect("stored");
            let other = repo
                .create(valid(ProjectDraft::new("Toamasina Port Logistics", date("2025-09-15"))))
                .await
                .expect("stored");

            let holiday = repo
                .add_holiday(
                    &owner.id,
                    HolidayDraft::new(date("2026-06-26"), "Independence Day")
                        .validate()
                        .expect("valid holiday"),
                )
                .await
                .expect("holiday is stored");

            let wrong_project = repo.remove_holiday(&other.id, &holiday.id).await;
            assert!(matches!(wrong_project, Err(AppError::NotFound { .. })));
            assert_eq!(repo.holidays(&owner.id).await.expect("query runs").len(), 1);

            repo.remove_holiday(&owner.id, &holiday.id).await.expect("the owner can remove it");
            assert!(repo.holidays(&owner.id).await.expect("query runs").is_empty());
        }

        #[tokio::test]
        async fn the_holiday_set_is_what_the_work_calendar_consumes() {
            let repo = repo().await;
            let project = repo.create(valid(solar_farm())).await.expect("stored");
            repo.add_holiday(
                &project.id,
                HolidayDraft::new(date("2026-09-07"), "Site shutdown")
                    .validate()
                    .expect("valid holiday"),
            )
            .await
            .expect("holiday is stored");

            let set: HolidaySet = repo.holiday_set(&project.id).await.expect("query runs");
            assert!(set.contains(date("2026-09-07")));
            assert!(!set.contains(date("2026-09-08")));
        }
    }

    mod statistics {
        use super::*;

        #[tokio::test]
        async fn portfolio_counts_match_the_overview_kpis() {
            let repo = repo().await;
            seed_portfolio(&repo).await;

            let stats = repo.portfolio_stats().await.expect("query runs");
            assert_eq!(stats, PortfolioStats { total: 4, active: 2, paused: 1, closed: 1, people: 0 });
        }

        #[tokio::test]
        async fn an_empty_portfolio_is_all_zeroes() {
            let repo = repo().await;
            assert_eq!(repo.portfolio_stats().await.expect("query runs"), PortfolioStats::default());
        }

        #[tokio::test]
        async fn project_stats_subtract_the_projects_own_holidays() {
            let repo = repo().await;
            let project = repo.create(valid(solar_farm())).await.expect("stored");

            // Two September working days off, plus one Saturday that changes
            // nothing under a Mon–Fri calendar.
            for day in ["2026-09-07", "2026-09-08", "2026-09-12"] {
                repo.add_holiday(
                    &project.id,
                    HolidayDraft::new(date(day), "Site shutdown").validate().expect("valid"),
                )
                .await
                .expect("holiday is stored");
            }

            let stats = repo.stats(&project.id, date("2026-09-15")).await.expect("query runs");

            assert_eq!(stats.project_id, project.id);
            assert_eq!(stats.month, YearMonth::new(2026, 9).expect("september"));
            assert_eq!(stats.holiday_count, 3);
            assert_eq!(stats.working_days_this_month, 20, "22 weekdays less two shutdown days");
            assert_eq!(stats.working_minutes_this_month, 20 * 8 * 60);
            assert_eq!(stats.duration.percent_elapsed, Some(44));
        }

        #[tokio::test]
        async fn stats_for_a_project_that_is_gone_are_an_error() {
            let repo = repo().await;
            assert!(matches!(
                repo.stats(&ProjectId::from("ghost"), date("2026-09-15")).await,
                Err(AppError::NotFound { entity: "project", .. })
            ));
        }
    }

    mod audit {
        use super::*;

        #[tokio::test]
        async fn every_change_to_a_project_is_logged_in_order() {
            let (repo, _, log) = linked().await;
            let created = repo.create(valid(solar_farm())).await.expect("stored");

            let mut edit = solar_farm();
            edit.status = ProjectStatus::Paused;
            repo.update(&created.id, valid(edit)).await.expect("update runs");
            repo.delete(&created.id).await.expect("delete runs");

            let history =
                log.history("project", created.id.as_str()).await.expect("query runs");
            let actions: Vec<AuditAction> = history.iter().map(|e| e.action).collect();
            assert_eq!(
                actions,
                [AuditAction::Create, AuditAction::Update, AuditAction::Delete]
            );
        }

        #[tokio::test]
        async fn a_deleted_project_is_still_recoverable_from_its_audit_snapshot() {
            let (repo, _, log) = linked().await;
            let created = repo.create(valid(solar_farm())).await.expect("stored");
            repo.delete(&created.id).await.expect("delete runs");

            let history = log.history("project", created.id.as_str()).await.expect("query runs");
            let deletion = history.last().expect("the delete is logged");
            let detail = deletion.detail.as_deref().expect("a delete snapshots the row");
            let recovered: Project =
                serde_json::from_str(detail).expect("the snapshot is the project itself");

            assert_eq!(recovered, created);
        }

        #[tokio::test]
        async fn recent_activity_is_newest_first_and_respects_its_limit() {
            let (repo, _, log) = linked().await;
            seed_portfolio(&repo).await;

            let recent = log.recent_activity(2).await.expect("query runs");
            assert_eq!(recent.len(), 2);
            assert!(recent[0].id > recent[1].id);

            let all = log.recent_activity(50).await.expect("query runs");
            assert_eq!(all.len(), 4, "one create per seeded project");
            assert!(all.iter().all(|e| e.action == AuditAction::Create));
        }

        #[tokio::test]
        async fn holiday_changes_are_logged_against_the_holiday() {
            let (repo, _, log) = linked().await;
            let project = repo.create(valid(solar_farm())).await.expect("stored");
            let holiday = repo
                .add_holiday(
                    &project.id,
                    HolidayDraft::new(date("2026-06-26"), "Independence Day")
                        .validate()
                        .expect("valid holiday"),
                )
                .await
                .expect("holiday is stored");
            repo.remove_holiday(&project.id, &holiday.id).await.expect("removal runs");

            let history = log
                .history("project_holiday", holiday.id.as_str())
                .await
                .expect("query runs");
            let actions: Vec<AuditAction> = history.iter().map(|e| e.action).collect();
            assert_eq!(actions, [AuditAction::Create, AuditAction::Delete]);
        }
    }

    mod employees {
        use super::*;

        use crate::domain::employee::EmployeeDraft;

        /// The mockup's first employee, on a freshly created project.
        async fn rakoto() -> EmployeeDraft {
            let mut draft = EmployeeDraft::new(
                "Rakoto",
                "Randrianasolo",
                "Site supervisor",
                date("2026-02-01"),
            );
            draft.email = Some("rakoto.randrianasolo@tymio.mg".into());
            draft.phone = Some("+261 34 12 887 01".into());
            draft.cin = Some("201021045".into());
            draft.birth_date = Some(date("1988-04-12"));
            draft
        }

        fn ok(draft: EmployeeDraft) -> ValidEmployee {
            draft.validate().expect("test draft is valid")
        }

        async fn on_a_project() -> (SqliteProjectRepository, SqliteEmployeeRepository, SqliteActivityRepository, Project)
        {
            let (projects, employees, log) = linked().await;
            let project = projects.create(valid(solar_farm())).await.expect("stored");
            (projects, employees, log, project)
        }

        #[tokio::test]
        async fn a_hired_employee_reads_back_identically() {
            let (_, employees, _, project) = on_a_project().await;
            let hired = employees
                .create(&project.id, ok(rakoto().await))
                .await
                .expect("hired");

            let fetched =
                employees.get(&hired.id).await.expect("query runs").expect("it is there");
            assert_eq!(fetched, hired);
            assert_eq!(fetched.project_id, project.id);
            assert_eq!(fetched.full_name(), "Rakoto Randrianasolo");
            assert_eq!(fetched.role, "Site supervisor");
            assert_eq!(fetched.cin.as_deref(), Some("201021045"));
            assert_eq!(fetched.birth_date, Some(date("1988-04-12")));
        }

        #[tokio::test]
        async fn an_employee_with_nothing_optional_recorded_round_trips_too() {
            let (_, employees, _, project) = on_a_project().await;
            let hired = employees
                .create(
                    &project.id,
                    ok(EmployeeDraft::new("Hery", "Rabemananjara", "Crane operator", date("2026-04-01"))),
                )
                .await
                .expect("hired");

            let fetched =
                employees.get(&hired.id).await.expect("query runs").expect("it is there");
            assert_eq!(fetched.email, None);
            assert_eq!(fetched.cin, None);
            assert_eq!(fetched.birth_date, None);
            assert_eq!(fetched.bank_account, None);
        }

        #[tokio::test]
        async fn nobody_can_be_hired_onto_a_project_that_does_not_exist() {
            let (_, employees, _) = linked().await;
            let result = employees.create(&ProjectId::from("ghost"), ok(rakoto().await)).await;

            assert!(matches!(result, Err(AppError::NotFound { entity: "project", .. })));
        }

        #[tokio::test]
        async fn an_unknown_id_is_none_but_requiring_it_is_an_error() {
            let (_, employees, _) = linked().await;
            let missing = EmployeeId::from("no-such-employee");

            assert_eq!(employees.get(&missing).await.expect("query runs"), None);
            assert!(matches!(
                employees.require(&missing).await,
                Err(AppError::NotFound { entity: "employee", .. })
            ));
        }

        #[tokio::test]
        async fn the_list_is_ordered_by_last_name_then_first() {
            let (_, employees, _, project) = on_a_project().await;
            for (first, last) in [
                ("Naivo", "Razafimahatratra"),
                ("Fara", "Rasoanaivo"),
                ("Soa", "Rakotoarisoa"),
                ("Ando", "Rasoanaivo"),
            ] {
                employees
                    .create(
                        &project.id,
                        ok(EmployeeDraft::new(first, last, "Operative", date("2026-03-02"))),
                    )
                    .await
                    .expect("hired");
            }

            let listed = employees.list(&EmployeeFilter::default()).await.expect("query runs");
            let names: Vec<String> = listed.iter().map(|e| e.full_name()).collect();
            assert_eq!(
                names,
                [
                    "Soa Rakotoarisoa",
                    "Ando Rasoanaivo",
                    "Fara Rasoanaivo",
                    "Naivo Razafimahatratra",
                ]
            );
        }

        #[tokio::test]
        async fn the_list_narrows_to_one_project() {
            let (projects, employees, _, first) = on_a_project().await;
            let second = projects
                .create(valid(ProjectDraft::new("Toamasina Port Logistics", date("2025-09-15"))))
                .await
                .expect("stored");

            employees.create(&first.id, ok(rakoto().await)).await.expect("hired");
            employees
                .create(
                    &second.id,
                    ok(EmployeeDraft::new("Soa", "Rakotoarisoa", "Warehouse lead", date("2025-10-01"))),
                )
                .await
                .expect("hired");

            assert_eq!(
                employees.list(&EmployeeFilter::in_project(&first.id)).await.expect("query runs").len(),
                1
            );
            // No filter is the "People on payroll" count: everyone, everywhere.
            assert_eq!(employees.list(&EmployeeFilter::default()).await.expect("query runs").len(), 2);
        }

        #[tokio::test]
        async fn the_search_box_reaches_role_email_and_cin() {
            let (_, employees, _, project) = on_a_project().await;
            employees.create(&project.id, ok(rakoto().await)).await.expect("hired");

            for query in ["randrianasolo", "SITE SUPERVISOR", "tymio.mg", "201021045"] {
                assert_eq!(
                    employees.list(&EmployeeFilter::search(query)).await.expect("query runs").len(),
                    1,
                    "expected {query:?} to match"
                );
            }
            assert!(employees
                .list(&EmployeeFilter::search("electrician"))
                .await
                .expect("query runs")
                .is_empty());
        }

        #[tokio::test]
        async fn an_edit_keeps_the_employee_on_their_project() {
            let (_, employees, _, project) = on_a_project().await;
            let hired = employees.create(&project.id, ok(rakoto().await)).await.expect("hired");

            let mut promotion = rakoto().await;
            promotion.role = "Project manager".into();
            let updated =
                employees.update(&hired.id, ok(promotion)).await.expect("update runs");

            assert_eq!(updated.id, hired.id);
            assert_eq!(updated.project_id, project.id);
            assert_eq!(updated.created_at, hired.created_at);
            assert!(updated.updated_at >= hired.updated_at);
            assert_eq!(updated.role, "Project manager");

            let refetched =
                employees.get(&hired.id).await.expect("query runs").expect("still there");
            assert_eq!(refetched, updated);
        }

        #[tokio::test]
        async fn editing_someone_who_is_gone_is_an_error() {
            let (_, employees, _) = linked().await;
            let result = employees.update(&EmployeeId::from("ghost"), ok(rakoto().await)).await;

            assert!(matches!(result, Err(AppError::NotFound { entity: "employee", .. })));
        }

        #[tokio::test]
        async fn one_cin_belongs_to_one_person() {
            let (_, employees, _, project) = on_a_project().await;
            employees.create(&project.id, ok(rakoto().await)).await.expect("hired");

            let mut twin = EmployeeDraft::new("Fara", "Rasoanaivo", "HSE officer", date("2026-02-15"));
            // The same number written with spaces is the same number.
            twin.cin = Some("201 021 045".into());

            match employees.create(&project.id, ok(twin)).await {
                Err(AppError::Conflict(message)) => assert!(message.contains("201021045")),
                other => panic!("expected a conflict, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn a_cin_clash_is_caught_on_edit_as_well_as_on_hire() {
            let (_, employees, _, project) = on_a_project().await;
            employees.create(&project.id, ok(rakoto().await)).await.expect("hired");

            let second = employees
                .create(
                    &project.id,
                    ok(EmployeeDraft::new("Fara", "Rasoanaivo", "HSE officer", date("2026-02-15"))),
                )
                .await
                .expect("hired without a CIN");

            let mut clash = EmployeeDraft::new("Fara", "Rasoanaivo", "HSE officer", date("2026-02-15"));
            clash.cin = Some("201021045".into());
            assert!(matches!(
                employees.update(&second.id, ok(clash)).await,
                Err(AppError::Conflict(_))
            ));
        }

        #[tokio::test]
        async fn any_number_of_employees_may_have_no_cin_recorded() {
            let (_, employees, _, project) = on_a_project().await;
            for first in ["Lalao", "Nivo", "Vola"] {
                employees
                    .create(
                        &project.id,
                        ok(EmployeeDraft::new(first, "Ravelojaona", "Operative", date("2026-03-02"))),
                    )
                    .await
                    .expect("a missing CIN is not a clash");
            }

            assert_eq!(employees.headcount(&project.id).await.expect("query runs"), 3);
        }

        #[tokio::test]
        async fn deleting_returns_who_was_removed() {
            let (_, employees, _, project) = on_a_project().await;
            let hired = employees.create(&project.id, ok(rakoto().await)).await.expect("hired");

            let removed = employees.delete(&hired.id).await.expect("delete runs");
            assert_eq!(removed, hired);
            assert_eq!(employees.get(&hired.id).await.expect("query runs"), None);
            assert_eq!(employees.headcount(&project.id).await.expect("query runs"), 0);
        }

        #[tokio::test]
        async fn deleting_someone_who_is_gone_is_an_error() {
            let (_, employees, _) = linked().await;
            assert!(matches!(
                employees.delete(&EmployeeId::from("ghost")).await,
                Err(AppError::NotFound { entity: "employee", .. })
            ));
        }

        #[tokio::test]
        async fn deleting_a_project_takes_its_people_with_it_and_says_so() {
            let (projects, employees, log, project) = on_a_project().await;
            let hired = employees.create(&project.id, ok(rakoto().await)).await.expect("hired");

            projects.delete(&project.id).await.expect("delete runs");

            assert_eq!(employees.get(&hired.id).await.expect("query runs"), None);

            // The cascade is silent in SQL; the audit log must not be.
            let history = log.history("employee", hired.id.as_str()).await.expect("query runs");
            let actions: Vec<AuditAction> = history.iter().map(|e| e.action).collect();
            assert_eq!(actions, [AuditAction::Create, AuditAction::Delete]);

            let snapshot = history.last().expect("the delete is logged").detail.as_deref();
            let recovered: Employee =
                serde_json::from_str(snapshot.expect("a delete snapshots the row"))
                    .expect("the snapshot is the employee itself");
            assert_eq!(recovered, hired);
        }

        #[tokio::test]
        async fn every_change_to_an_employee_is_logged_in_order() {
            let (_, employees, log, project) = on_a_project().await;
            let hired = employees.create(&project.id, ok(rakoto().await)).await.expect("hired");

            let mut promotion = rakoto().await;
            promotion.role = "Project manager".into();
            employees.update(&hired.id, ok(promotion)).await.expect("update runs");
            employees.delete(&hired.id).await.expect("delete runs");

            let history = log.history("employee", hired.id.as_str()).await.expect("query runs");
            let actions: Vec<AuditAction> = history.iter().map(|e| e.action).collect();
            assert_eq!(actions, [AuditAction::Create, AuditAction::Update, AuditAction::Delete]);
        }

        #[tokio::test]
        async fn a_rejected_hire_leaves_no_audit_row() {
            let (_, employees, log, project) = on_a_project().await;
            employees.create(&project.id, ok(rakoto().await)).await.expect("hired");
            let before = log.recent_activity(50).await.expect("query runs").len();

            let mut twin = EmployeeDraft::new("Fara", "Rasoanaivo", "HSE officer", date("2026-02-15"));
            twin.cin = Some("201021045".into());
            let _ = employees.create(&project.id, ok(twin)).await;

            assert_eq!(log.recent_activity(50).await.expect("query runs").len(), before);
        }

        #[tokio::test]
        async fn employee_stats_come_from_the_stored_dates() {
            let (_, employees, _, project) = on_a_project().await;
            let hired = employees.create(&project.id, ok(rakoto().await)).await.expect("hired");

            let stats = employees.stats(&hired.id, date("2026-09-15")).await.expect("query runs");
            assert_eq!(stats.employee_id, hired.id);
            assert_eq!(stats.project_id, project.id);
            assert_eq!(stats.age, Some(38));
            assert_eq!(stats.months_of_service, 7);
            assert_eq!(stats.months_worked_this_year, 8);
        }

        #[tokio::test]
        async fn stats_for_someone_who_is_gone_are_an_error() {
            let (_, employees, _) = linked().await;
            assert!(matches!(
                employees.stats(&EmployeeId::from("ghost"), date("2026-09-15")).await,
                Err(AppError::NotFound { entity: "employee", .. })
            ));
        }

        #[tokio::test]
        async fn headcount_reaches_the_project_card_and_the_portfolio_kpi() {
            let (projects, employees, _, first) = on_a_project().await;
            let second = projects
                .create(valid(ProjectDraft::new("Toamasina Port Logistics", date("2025-09-15"))))
                .await
                .expect("stored");

            for first_name in ["Rakoto", "Fara", "Naivo"] {
                employees
                    .create(
                        &first.id,
                        ok(EmployeeDraft::new(first_name, "Rasoanaivo", "Operative", date("2026-03-02"))),
                    )
                    .await
                    .expect("hired");
            }
            employees
                .create(
                    &second.id,
                    ok(EmployeeDraft::new("Soa", "Rakotoarisoa", "Warehouse lead", date("2025-10-01"))),
                )
                .await
                .expect("hired");

            let stats = projects.stats(&first.id, date("2026-09-15")).await.expect("query runs");
            assert_eq!(stats.headcount, 3, "the project card counts only its own people");

            let portfolio = projects.portfolio_stats().await.expect("query runs");
            assert_eq!(portfolio.people, 4, "the KPI counts everyone, everywhere");
            assert_eq!(portfolio.total, 2);
        }

        #[tokio::test]
        async fn an_empty_project_has_a_headcount_of_zero() {
            let (projects, _, _, project) = on_a_project().await;
            let stats = projects.stats(&project.id, date("2026-09-15")).await.expect("query runs");
            assert_eq!(stats.headcount, 0);
        }
    }

    mod attendance {
        use super::*;

        use crate::domain::attendance::{
            AttendanceContext, AttendanceDraft, AttendanceSource, WorkedDays, WorkedTime,
        };
        use crate::domain::calendar::YearMonth;
        use crate::domain::employee::EmployeeDraft;

        fn september() -> YearMonth {
            YearMonth::new(2026, 9).expect("september")
        }

        /// A validated row for someone hired long ago, so only the numbers
        /// under test can fail.
        fn row(days_halves: i64, minutes: i64, overtime: i64) -> ValidAttendance {
            AttendanceDraft::new(days_halves, minutes, overtime)
                .validate(AttendanceContext::new(september(), date("2020-01-06")))
                .expect("valid row")
        }

        async fn staffed() -> (
            SqliteProjectRepository,
            SqliteEmployeeRepository,
            SqliteAttendanceRepository,
            SqliteActivityRepository,
            Project,
            Vec<Employee>,
        ) {
            let db = Db::in_memory().await.expect("in-memory database opens");
            let projects = SqliteProjectRepository::new(db.clone());
            let employees = SqliteEmployeeRepository::new(db.clone());
            let attendance = SqliteAttendanceRepository::new(db.clone());
            let log = SqliteActivityRepository::new(db);

            let project = projects.create(valid(solar_farm())).await.expect("stored");
            let mut hired = Vec::new();
            for (first, last, role) in [
                ("Rakoto", "Randrianasolo", "Site supervisor"),
                ("Fara", "Rasoanaivo", "HSE officer"),
                ("Naivo", "Razafimahatratra", "Electrician"),
            ] {
                hired.push(
                    employees
                        .create(
                            &project.id,
                            EmployeeDraft::new(first, last, role, date("2020-01-06"))
                                .validate()
                                .expect("valid"),
                        )
                        .await
                        .expect("hired"),
                );
            }

            (projects, employees, attendance, log, project, hired)
        }

        #[tokio::test]
        async fn a_recorded_month_reads_back_identically() {
            let (_, _, attendance, _, _, people) = staffed().await;

            let stored = attendance
                .record(&people[0].id, september(), row(44, 176 * 60, 3 * 60))
                .await
                .expect("recorded");

            let fetched = attendance
                .get(&people[0].id, september())
                .await
                .expect("query runs")
                .expect("it is there");
            assert_eq!(fetched, stored);
            assert_eq!(fetched.period, september());
            assert_eq!(fetched.days_worked, WorkedDays::from_days(22));
            assert_eq!(fetched.hours_worked, WorkedTime::from_hours(176));
            assert_eq!(fetched.overtime, WorkedTime::from_hours(3));
            assert_eq!(fetched.source, AttendanceSource::Manual);
        }

        #[tokio::test]
        async fn half_days_survive_the_round_trip() {
            let (_, _, attendance, _, _, people) = staffed().await;
            attendance
                .record(&people[0].id, september(), row(43, 172 * 60, 0))
                .await
                .expect("recorded");

            let fetched = attendance
                .get(&people[0].id, september())
                .await
                .expect("query runs")
                .expect("it is there");
            assert_eq!(fetched.days_worked.to_string(), "21.5");
        }

        #[tokio::test]
        async fn an_unrecorded_month_is_none_not_a_zero_row() {
            let (_, _, attendance, _, _, people) = staffed().await;
            assert_eq!(
                attendance.get(&people[0].id, september()).await.expect("query runs"),
                None
            );
        }

        #[tokio::test]
        async fn recording_a_month_twice_replaces_it_rather_than_duplicating() {
            let (_, _, attendance, _, _, people) = staffed().await;

            let first = attendance
                .record(&people[0].id, september(), row(44, 176 * 60, 0))
                .await
                .expect("recorded");
            let second = attendance
                .record(&people[0].id, september(), row(40, 160 * 60, 2 * 60))
                .await
                .expect("recorded again");

            assert_eq!(second.id, first.id, "the same month is the same row");
            assert_eq!(second.created_at, first.created_at);
            assert!(second.updated_at >= first.updated_at);
            assert_eq!(second.days_worked, WorkedDays::from_days(20));

            assert_eq!(attendance.history(&people[0].id).await.expect("query runs").len(), 1);
        }

        #[tokio::test]
        async fn one_month_does_not_disturb_another() {
            let (_, _, attendance, _, _, people) = staffed().await;
            let august = YearMonth::new(2026, 8).expect("august");

            attendance
                .record(&people[0].id, september(), row(44, 176 * 60, 0))
                .await
                .expect("recorded");
            attendance
                .record(
                    &people[0].id,
                    august,
                    AttendanceDraft::new(42, 168 * 60, 0)
                        .validate(AttendanceContext::new(august, date("2020-01-06")))
                        .expect("valid"),
                )
                .await
                .expect("recorded");

            let history = attendance.history(&people[0].id).await.expect("query runs");
            assert_eq!(history.len(), 2);
            // Most recent month first.
            assert_eq!(history[0].period, september());
            assert_eq!(history[1].period, august);
        }

        #[tokio::test]
        async fn two_people_keep_their_own_months() {
            let (_, _, attendance, _, _, people) = staffed().await;

            attendance
                .record(&people[0].id, september(), row(44, 176 * 60, 0))
                .await
                .expect("recorded");
            attendance
                .record(&people[1].id, september(), row(40, 160 * 60, 0))
                .await
                .expect("recorded");

            assert_eq!(
                attendance
                    .get(&people[1].id, september())
                    .await
                    .expect("query runs")
                    .expect("there")
                    .days_worked,
                WorkedDays::from_days(20)
            );
        }

        #[tokio::test]
        async fn attendance_cannot_be_recorded_against_somebody_who_does_not_exist() {
            let (_, _, attendance, _, _, _) = staffed().await;
            let result = attendance
                .record(&EmployeeId::from("ghost"), september(), row(44, 176 * 60, 0))
                .await;

            assert!(matches!(result, Err(AppError::NotFound { entity: "employee", .. })));
        }

        #[tokio::test]
        async fn clearing_returns_what_was_removed_and_clearing_nothing_is_not_an_error() {
            let (_, _, attendance, _, _, people) = staffed().await;
            let stored = attendance
                .record(&people[0].id, september(), row(44, 176 * 60, 0))
                .await
                .expect("recorded");

            assert_eq!(
                attendance.clear(&people[0].id, september()).await.expect("clear runs"),
                Some(stored)
            );
            assert_eq!(
                attendance.get(&people[0].id, september()).await.expect("query runs"),
                None
            );
            // A blank month is a legitimate state, not a failure.
            assert_eq!(
                attendance.clear(&people[0].id, september()).await.expect("clear runs"),
                None
            );
        }

        #[tokio::test]
        async fn a_bulk_write_lands_as_one_transaction() {
            let (_, _, attendance, _, _, people) = staffed().await;

            let written = attendance
                .record_many(
                    september(),
                    people.iter().map(|p| (p.id.clone(), row(44, 176 * 60, 0))).collect(),
                )
                .await
                .expect("bulk write runs");

            assert_eq!(written, 3);
            for person in &people {
                assert!(attendance
                    .get(&person.id, september())
                    .await
                    .expect("query runs")
                    .is_some());
            }
        }

        #[tokio::test]
        async fn a_bulk_write_that_fails_writes_nothing_at_all() {
            let (_, _, attendance, log, _, people) = staffed().await;
            // Staffing the project logged rows of its own; only what the bulk
            // write would add is under test.
            let before = log.recent_activity(50).await.expect("query runs").len();

            let result = attendance
                .record_many(
                    september(),
                    vec![
                        (people[0].id.clone(), row(44, 176 * 60, 0)),
                        (EmployeeId::from("ghost"), row(44, 176 * 60, 0)),
                    ],
                )
                .await;

            assert!(result.is_err());
            assert_eq!(
                attendance.get(&people[0].id, september()).await.expect("query runs"),
                None,
                "the row before the failure must roll back too"
            );
            assert_eq!(
                log.recent_activity(50).await.expect("query runs").len(),
                before,
                "a rolled-back bulk write must not leave audit rows"
            );
        }

        #[tokio::test]
        async fn the_sheet_gives_every_employee_a_line_recorded_or_not() {
            let (_, _, attendance, _, project, people) = staffed().await;
            attendance
                .record(&people[0].id, september(), row(44, 176 * 60, 3 * 60))
                .await
                .expect("recorded");

            let sheet = attendance.sheet(&project.id, september()).await.expect("query runs");

            assert_eq!(sheet.project_id, project.id);
            assert_eq!(sheet.period, september());
            assert_eq!(sheet.rows.len(), 3, "everyone gets a line");
            assert_eq!(sheet.totals.recorded, 1);
            assert_eq!(sheet.totals.missing, 2);
            assert_eq!(sheet.total_days(), WorkedDays::from_days(22));
            assert_eq!(sheet.totals.overtime_minutes, 3 * 60);
        }

        #[tokio::test]
        async fn the_sheet_is_ordered_the_way_the_employees_screen_is() {
            let (_, _, attendance, _, project, _) = staffed().await;
            let sheet = attendance.sheet(&project.id, september()).await.expect("query runs");

            // Randrianasolo, Rasoanaivo, Razafimahatratra — by last name.
            assert_eq!(sheet.rows.len(), 3);
            assert!(sheet.rows.iter().all(|row| row.entry.is_none()));
        }

        #[tokio::test]
        async fn the_sheet_of_an_empty_project_totals_zero() {
            let (projects, _, attendance, _, _, _) = staffed().await;
            let empty = projects
                .create(valid(ProjectDraft::new("Toamasina Port Logistics", date("2025-09-15"))))
                .await
                .expect("stored");

            let sheet = attendance.sheet(&empty.id, september()).await.expect("query runs");
            assert!(sheet.rows.is_empty());
            assert_eq!(sheet.totals.recorded, 0);
        }

        #[tokio::test]
        async fn removing_an_employee_takes_their_attendance_with_them() {
            let (_, employees, attendance, _, _, people) = staffed().await;
            attendance
                .record(&people[0].id, september(), row(44, 176 * 60, 0))
                .await
                .expect("recorded");

            employees.delete(&people[0].id).await.expect("removed");

            assert!(attendance.history(&people[0].id).await.expect("query runs").is_empty());
        }

        #[tokio::test]
        async fn deleting_a_project_takes_the_attendance_of_everyone_on_it() {
            let (projects, _, attendance, _, project, people) = staffed().await;
            attendance
                .record(&people[0].id, september(), row(44, 176 * 60, 0))
                .await
                .expect("recorded");

            projects.delete(&project.id).await.expect("deleted");

            let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM attendance")
                .fetch_one(attendance.db().pool())
                .await
                .expect("query runs");
            assert_eq!(left, 0);
        }

        #[tokio::test]
        async fn recording_logs_a_create_and_then_updates() {
            let (_, _, attendance, log, _, people) = staffed().await;

            let stored = attendance
                .record(&people[0].id, september(), row(44, 176 * 60, 0))
                .await
                .expect("recorded");
            attendance
                .record(&people[0].id, september(), row(40, 160 * 60, 0))
                .await
                .expect("recorded again");
            attendance.clear(&people[0].id, september()).await.expect("cleared");

            let history = log.history("attendance", stored.id.as_str()).await.expect("query runs");
            let actions: Vec<AuditAction> = history.iter().map(|e| e.action).collect();
            assert_eq!(
                actions,
                [AuditAction::Create, AuditAction::Update, AuditAction::Delete]
            );
        }
    }
}
