//! The IPC surface for projects.
//!
//! The `#[tauri::command]` functions are deliberately one line each: they
//! forward to `AppState`, which is where validation and defaulting live. That
//! is the layer the tests drive, because a Tauri `State` is not something a
//! unit test can conjure.

use std::sync::Arc;

use chrono::{Local, NaiveDate};
use tauri::State;

use crate::db::Db;
use crate::domain::attendance::{
    AttendanceContext, AttendanceDraft, AttendanceEntry, AttendanceSheet, ValidAttendance,
    WorkedDays, WorkedTime,
};
use crate::domain::calendar::YearMonth;
use crate::domain::contract::{Contract, ContractContext, ContractDraft};
use crate::domain::employee::{
    Employee, EmployeeDraft, EmployeeFilter, EmployeeId, EmployeeStats,
};
use crate::domain::project::{
    Holiday, HolidayDraft, HolidayId, PortfolioStats, Project, ProjectDraft, ProjectFilter,
    ProjectId, ProjectStats,
};
use crate::error::Result;
use crate::repo::sqlite::{
    SqliteActivityRepository, SqliteAttendanceRepository, SqliteContractRepository,
    SqliteEmployeeRepository, SqliteProjectRepository,
};
use crate::repo::{
    ActivityRepository, AttendanceRepository, AuditEntry, ContractRepository, EmployeeRepository,
    ProjectRepository,
};

/// How many audit rows the overview's activity list asks for by default.
pub const DEFAULT_ACTIVITY_LIMIT: u32 = 20;

/// Everything a command needs: one handle per storage trait, all over the
/// same pool.
#[derive(Clone)]
pub struct AppState {
    projects: Arc<dyn ProjectRepository>,
    employees: Arc<dyn EmployeeRepository>,
    attendance: Arc<dyn AttendanceRepository>,
    contracts: Arc<dyn ContractRepository>,
    activity: Arc<dyn ActivityRepository>,
}

impl AppState {
    pub fn new(db: Db) -> Self {
        AppState {
            projects: Arc::new(SqliteProjectRepository::new(db.clone())),
            employees: Arc::new(SqliteEmployeeRepository::new(db.clone())),
            attendance: Arc::new(SqliteAttendanceRepository::new(db.clone())),
            contracts: Arc::new(SqliteContractRepository::new(db.clone())),
            activity: Arc::new(SqliteActivityRepository::new(db)),
        }
    }

    /// Today, as a civil date in the user's own timezone — the date they would
    /// write on a form, not the UTC instant.
    fn today() -> NaiveDate {
        Local::now().date_naive()
    }

    pub async fn create_project(&self, draft: ProjectDraft) -> Result<Project> {
        self.projects.create(draft.validate()?).await
    }

    pub async fn get_project(&self, id: ProjectId) -> Result<Option<Project>> {
        self.projects.get(&id).await
    }

    pub async fn list_projects(&self, filter: Option<ProjectFilter>) -> Result<Vec<Project>> {
        self.projects.list(&filter.unwrap_or_default()).await
    }

    pub async fn update_project(&self, id: ProjectId, draft: ProjectDraft) -> Result<Project> {
        self.projects.update(&id, draft.validate()?).await
    }

    pub async fn delete_project(&self, id: ProjectId) -> Result<Project> {
        self.projects.delete(&id).await
    }

    pub async fn portfolio_stats(&self) -> Result<PortfolioStats> {
        self.projects.portfolio_stats().await
    }

    /// `as_of` is explicit so a screen can look at any month; it falls back to
    /// today rather than being read from the clock deep inside the domain.
    pub async fn project_stats(
        &self,
        id: ProjectId,
        as_of: Option<NaiveDate>,
    ) -> Result<ProjectStats> {
        self.projects.stats(&id, as_of.unwrap_or_else(Self::today)).await
    }

    pub async fn project_holidays(&self, id: ProjectId) -> Result<Vec<Holiday>> {
        self.projects.holidays(&id).await
    }

    pub async fn add_project_holiday(
        &self,
        id: ProjectId,
        holiday: HolidayDraft,
    ) -> Result<Holiday> {
        self.projects.add_holiday(&id, holiday.validate()?).await
    }

    pub async fn remove_project_holiday(
        &self,
        id: ProjectId,
        holiday: HolidayId,
    ) -> Result<()> {
        self.projects.remove_holiday(&id, &holiday).await
    }

    pub async fn recent_activity(&self, limit: Option<u32>) -> Result<Vec<AuditEntry>> {
        self.activity.recent_activity(limit.unwrap_or(DEFAULT_ACTIVITY_LIMIT)).await
    }

    /// Hires someone onto a project. The project is named separately from the
    /// draft because it is set once and never edited afterwards.
    pub async fn create_employee(
        &self,
        project: ProjectId,
        draft: EmployeeDraft,
    ) -> Result<Employee> {
        self.employees.create(&project, draft.validate()?).await
    }

    pub async fn get_employee(&self, id: EmployeeId) -> Result<Option<Employee>> {
        self.employees.get(&id).await
    }

    /// With no filter this lists everyone on the books, across every project.
    pub async fn list_employees(&self, filter: Option<EmployeeFilter>) -> Result<Vec<Employee>> {
        self.employees.list(&filter.unwrap_or_default()).await
    }

    pub async fn update_employee(
        &self,
        id: EmployeeId,
        draft: EmployeeDraft,
    ) -> Result<Employee> {
        self.employees.update(&id, draft.validate()?).await
    }

    pub async fn delete_employee(&self, id: EmployeeId) -> Result<Employee> {
        self.employees.delete(&id).await
    }

    pub async fn employee_stats(
        &self,
        id: EmployeeId,
        as_of: Option<NaiveDate>,
    ) -> Result<EmployeeStats> {
        self.employees.stats(&id, as_of.unwrap_or_else(Self::today)).await
    }

    pub async fn attendance_sheet(
        &self,
        project: ProjectId,
        period: YearMonth,
    ) -> Result<AttendanceSheet> {
        self.attendance.sheet(&project, period).await
    }

    /// Records one employee's month. The hire date has to be read first: it is
    /// what makes "January, for someone hired in February" an error rather
    /// than a month with nothing in it.
    pub async fn record_attendance(
        &self,
        employee: EmployeeId,
        period: YearMonth,
        draft: AttendanceDraft,
    ) -> Result<AttendanceEntry> {
        let person = self.employees.require(&employee).await?;
        let valid = draft.validate(AttendanceContext::new(period, person.hire_date))?;
        self.attendance.record(&employee, period, valid).await
    }

    /// One employee's single month. `None` means nobody has recorded it,
    /// which is not the same as a month recorded as zero.
    pub async fn attendance_entry(
        &self,
        employee: EmployeeId,
        period: YearMonth,
    ) -> Result<Option<AttendanceEntry>> {
        self.attendance.get(&employee, period).await
    }

    pub async fn clear_attendance(
        &self,
        employee: EmployeeId,
        period: YearMonth,
    ) -> Result<Option<AttendanceEntry>> {
        self.attendance.clear(&employee, period).await
    }

    /// "Fill from standard schedule": seeds the whole grid from the project's
    /// work calendar.
    ///
    /// Orchestrated here rather than in a repository because it spans three of
    /// them. The writes go in as one transaction, so a filled grid is never
    /// half-filled.
    pub async fn fill_attendance_from_schedule(
        &self,
        project: ProjectId,
        period: YearMonth,
    ) -> Result<AttendanceSheet> {
        let stored = self.projects.require(&project).await?;
        let holidays = self.projects.holiday_set(&project).await?;
        let employees = self.employees.list(&EmployeeFilter::in_project(&project)).await?;

        let mut seeded = Vec::with_capacity(employees.len());
        for employee in &employees {
            // Nothing to seed for a month before somebody was hired — and
            // recording one would fail validation anyway.
            if period < YearMonth::of(employee.hire_date) {
                continue;
            }

            // Overtime is the one number the calendar cannot know, so an
            // existing value survives the refill.
            let overtime = self
                .attendance
                .get(&employee.id, period)
                .await?
                .map(|entry| entry.overtime)
                .unwrap_or(WorkedTime::ZERO);

            seeded.push((
                employee.id.clone(),
                ValidAttendance::from_standard_schedule(
                    &stored.calendar,
                    &holidays,
                    period,
                    employee.hire_date,
                    // Approved leave belongs here. The leave slice fills it in;
                    // until then nothing is deducted.
                    WorkedDays::ZERO,
                    overtime,
                ),
            ));
        }

        self.attendance.record_many(period, seeded).await?;
        self.attendance.sheet(&project, period).await
    }

    pub async fn employee_attendance(&self, employee: EmployeeId) -> Result<Vec<AttendanceEntry>> {
        self.attendance.history(&employee).await
    }

    /// The terms in force on a date — what payroll asks for.
    pub async fn current_contract(
        &self,
        employee: EmployeeId,
        as_of: Option<NaiveDate>,
    ) -> Result<Option<Contract>> {
        self.contracts.current(&employee, as_of.unwrap_or_else(Self::today)).await
    }

    pub async fn contract_history(&self, employee: EmployeeId) -> Result<Vec<Contract>> {
        self.contracts.history(&employee).await
    }

    /// Writes a new version of an employee's contract.
    ///
    /// The same command creates the first one and records a raise: the
    /// difference is whether there is anything to supersede, which is read
    /// here and handed to validation rather than guessed at by the caller.
    pub async fn amend_contract(
        &self,
        employee: EmployeeId,
        draft: ContractDraft,
    ) -> Result<Contract> {
        let person = self.employees.require(&employee).await?;
        let context = match self.contracts.latest(&employee).await? {
            Some(current) => ContractContext::amending(person.hire_date, &current),
            None => ContractContext::first(person.hire_date),
        };
        self.contracts.amend(&employee, draft.validate(context)?).await
    }

    /// Undoes the most recent amendment.
    pub async fn discard_latest_contract(&self, employee: EmployeeId) -> Result<Contract> {
        self.contracts.discard_latest(&employee).await
    }

    /// The overview's "Contracts ending soon". Defaults to the next 90 days.
    pub async fn contracts_ending(
        &self,
        project: ProjectId,
        from: Option<NaiveDate>,
        within_days: Option<u32>,
    ) -> Result<Vec<Contract>> {
        let from = from.unwrap_or_else(Self::today);
        let to = from + chrono::Duration::days(i64::from(within_days.unwrap_or(90)));
        self.contracts.ending_between(&project, from, to).await
    }
}

#[tauri::command]
pub async fn create_project(state: State<'_, AppState>, draft: ProjectDraft) -> Result<Project> {
    state.create_project(draft).await
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, id: ProjectId) -> Result<Option<Project>> {
    state.get_project(id).await
}

#[tauri::command]
pub async fn list_projects(
    state: State<'_, AppState>,
    filter: Option<ProjectFilter>,
) -> Result<Vec<Project>> {
    state.list_projects(filter).await
}

#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    id: ProjectId,
    draft: ProjectDraft,
) -> Result<Project> {
    state.update_project(id, draft).await
}

#[tauri::command]
pub async fn delete_project(state: State<'_, AppState>, id: ProjectId) -> Result<Project> {
    state.delete_project(id).await
}

#[tauri::command]
pub async fn portfolio_stats(state: State<'_, AppState>) -> Result<PortfolioStats> {
    state.portfolio_stats().await
}

#[tauri::command]
pub async fn project_stats(
    state: State<'_, AppState>,
    id: ProjectId,
    as_of: Option<NaiveDate>,
) -> Result<ProjectStats> {
    state.project_stats(id, as_of).await
}

#[tauri::command]
pub async fn project_holidays(state: State<'_, AppState>, id: ProjectId) -> Result<Vec<Holiday>> {
    state.project_holidays(id).await
}

#[tauri::command]
pub async fn add_project_holiday(
    state: State<'_, AppState>,
    id: ProjectId,
    holiday: HolidayDraft,
) -> Result<Holiday> {
    state.add_project_holiday(id, holiday).await
}

#[tauri::command]
pub async fn remove_project_holiday(
    state: State<'_, AppState>,
    id: ProjectId,
    holiday: HolidayId,
) -> Result<()> {
    state.remove_project_holiday(id, holiday).await
}

#[tauri::command]
pub async fn recent_activity(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<AuditEntry>> {
    state.recent_activity(limit).await
}

#[tauri::command]
pub async fn create_employee(
    state: State<'_, AppState>,
    project: ProjectId,
    draft: EmployeeDraft,
) -> Result<Employee> {
    state.create_employee(project, draft).await
}

#[tauri::command]
pub async fn get_employee(state: State<'_, AppState>, id: EmployeeId) -> Result<Option<Employee>> {
    state.get_employee(id).await
}

#[tauri::command]
pub async fn list_employees(
    state: State<'_, AppState>,
    filter: Option<EmployeeFilter>,
) -> Result<Vec<Employee>> {
    state.list_employees(filter).await
}

#[tauri::command]
pub async fn update_employee(
    state: State<'_, AppState>,
    id: EmployeeId,
    draft: EmployeeDraft,
) -> Result<Employee> {
    state.update_employee(id, draft).await
}

#[tauri::command]
pub async fn delete_employee(state: State<'_, AppState>, id: EmployeeId) -> Result<Employee> {
    state.delete_employee(id).await
}

#[tauri::command]
pub async fn employee_stats(
    state: State<'_, AppState>,
    id: EmployeeId,
    as_of: Option<NaiveDate>,
) -> Result<EmployeeStats> {
    state.employee_stats(id, as_of).await
}

#[tauri::command]
pub async fn attendance_sheet(
    state: State<'_, AppState>,
    project: ProjectId,
    period: YearMonth,
) -> Result<AttendanceSheet> {
    state.attendance_sheet(project, period).await
}

#[tauri::command]
pub async fn record_attendance(
    state: State<'_, AppState>,
    employee: EmployeeId,
    period: YearMonth,
    draft: AttendanceDraft,
) -> Result<AttendanceEntry> {
    state.record_attendance(employee, period, draft).await
}

#[tauri::command]
pub async fn attendance_entry(
    state: State<'_, AppState>,
    employee: EmployeeId,
    period: YearMonth,
) -> Result<Option<AttendanceEntry>> {
    state.attendance_entry(employee, period).await
}

#[tauri::command]
pub async fn clear_attendance(
    state: State<'_, AppState>,
    employee: EmployeeId,
    period: YearMonth,
) -> Result<Option<AttendanceEntry>> {
    state.clear_attendance(employee, period).await
}

#[tauri::command]
pub async fn fill_attendance_from_schedule(
    state: State<'_, AppState>,
    project: ProjectId,
    period: YearMonth,
) -> Result<AttendanceSheet> {
    state.fill_attendance_from_schedule(project, period).await
}

#[tauri::command]
pub async fn current_contract(
    state: State<'_, AppState>,
    employee: EmployeeId,
    as_of: Option<NaiveDate>,
) -> Result<Option<Contract>> {
    state.current_contract(employee, as_of).await
}

#[tauri::command]
pub async fn contract_history(
    state: State<'_, AppState>,
    employee: EmployeeId,
) -> Result<Vec<Contract>> {
    state.contract_history(employee).await
}

#[tauri::command]
pub async fn amend_contract(
    state: State<'_, AppState>,
    employee: EmployeeId,
    draft: ContractDraft,
) -> Result<Contract> {
    state.amend_contract(employee, draft).await
}

#[tauri::command]
pub async fn discard_latest_contract(
    state: State<'_, AppState>,
    employee: EmployeeId,
) -> Result<Contract> {
    state.discard_latest_contract(employee).await
}

#[tauri::command]
pub async fn contracts_ending(
    state: State<'_, AppState>,
    project: ProjectId,
    from: Option<NaiveDate>,
    within_days: Option<u32>,
) -> Result<Vec<Contract>> {
    state.contracts_ending(project, from, within_days).await
}

#[tauri::command]
pub async fn employee_attendance(
    state: State<'_, AppState>,
    employee: EmployeeId,
) -> Result<Vec<AttendanceEntry>> {
    state.employee_attendance(employee).await
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::project::ProjectStatus;
    use crate::error::AppError;

    fn date(s: &str) -> NaiveDate {
        s.parse().expect("test date is well formed")
    }

    async fn state() -> AppState {
        AppState::new(Db::in_memory().await.expect("in-memory database opens"))
    }

    #[tokio::test]
    async fn an_invalid_draft_never_reaches_storage() {
        let state = state().await;

        let error = state
            .create_project(ProjectDraft::new("  ", date("2026-02-01")))
            .await
            .expect_err("a nameless project is rejected");

        match error {
            AppError::Validation(errors) => assert!(errors.has("name")),
            other => panic!("expected validation, got {other:?}"),
        }
        assert!(state.list_projects(None).await.expect("query runs").is_empty());
    }

    #[tokio::test]
    async fn a_missing_filter_means_no_filter() {
        let state = state().await;
        state
            .create_project(ProjectDraft::new("Ambatolampy Solar Farm", date("2026-02-01")))
            .await
            .expect("stored");

        assert_eq!(state.list_projects(None).await.expect("query runs").len(), 1);
    }

    #[tokio::test]
    async fn an_invalid_edit_leaves_the_stored_project_alone() {
        let state = state().await;
        let created = state
            .create_project(ProjectDraft::new("Ambatolampy Solar Farm", date("2026-02-01")))
            .await
            .expect("stored");

        let mut bad = ProjectDraft::new("Renamed", date("2026-02-01"));
        bad.end = Some(date("2026-01-01"));
        assert!(state.update_project(created.id.clone(), bad).await.is_err());

        let unchanged = state
            .get_project(created.id)
            .await
            .expect("query runs")
            .expect("still there");
        assert_eq!(unchanged.name, "Ambatolampy Solar Farm");
    }

    #[tokio::test]
    async fn stats_default_to_today_when_no_date_is_given() {
        let state = state().await;
        let created = state
            .create_project(ProjectDraft::new("Ongoing maintenance", date("2020-01-01")))
            .await
            .expect("stored");

        let stats = state.project_stats(created.id, None).await.expect("query runs");
        assert_eq!(stats.as_of, Local::now().date_naive());
    }

    #[tokio::test]
    async fn a_blank_holiday_name_is_rejected_before_storage() {
        let state = state().await;
        let project = state
            .create_project(ProjectDraft::new("Ambatolampy Solar Farm", date("2026-02-01")))
            .await
            .expect("stored");

        let error = state
            .add_project_holiday(project.id.clone(), HolidayDraft::new(date("2026-06-26"), " "))
            .await
            .expect_err("a holiday needs a name");

        match error {
            AppError::Validation(errors) => assert!(errors.has("name")),
            other => panic!("expected validation, got {other:?}"),
        }
        assert!(state.project_holidays(project.id).await.expect("query runs").is_empty());
    }

    #[tokio::test]
    async fn the_activity_list_has_a_default_length() {
        let state = state().await;
        for n in 0..(DEFAULT_ACTIVITY_LIMIT + 5) {
            state
                .create_project(ProjectDraft::new(format!("Project {n}"), date("2026-02-01")))
                .await
                .expect("stored");
        }

        let default_length = state.recent_activity(None).await.expect("query runs");
        assert_eq!(default_length.len(), DEFAULT_ACTIVITY_LIMIT as usize);

        let explicit = state.recent_activity(Some(3)).await.expect("query runs");
        assert_eq!(explicit.len(), 3);
    }

    #[tokio::test]
    async fn the_whole_project_lifecycle_runs_through_the_command_layer() {
        let state = state().await;

        let mut draft = ProjectDraft::new("Ambatolampy Solar Farm", date("2026-02-01"));
        draft.client = Some("JIRAMA".into());
        draft.end = Some(date("2027-06-30"));
        let created = state.create_project(draft).await.expect("created");

        state
            .add_project_holiday(
                created.id.clone(),
                HolidayDraft::new(date("2026-06-26"), "Independence Day"),
            )
            .await
            .expect("holiday added");

        let mut edit = ProjectDraft::new("Ambatolampy Solar Farm", date("2026-02-01"));
        edit.status = ProjectStatus::Paused;
        edit.end = Some(date("2027-06-30"));
        let paused = state.update_project(created.id.clone(), edit).await.expect("updated");
        assert_eq!(paused.status, ProjectStatus::Paused);

        let portfolio = state.portfolio_stats().await.expect("query runs");
        assert_eq!(portfolio, PortfolioStats { total: 1, active: 0, paused: 1, closed: 0, people: 0 });

        let stats = state
            .project_stats(created.id.clone(), Some(date("2026-06-15")))
            .await
            .expect("query runs");
        assert_eq!(stats.holiday_count, 1);

        state.delete_project(created.id.clone()).await.expect("deleted");
        assert_eq!(state.get_project(created.id).await.expect("query runs"), None);
    }

    mod employees {
        use super::*;

        use crate::domain::employee::EmployeeDraft;

        async fn with_a_project() -> (AppState, ProjectId) {
            let state = state().await;
            let project = state
                .create_project(ProjectDraft::new("Ambatolampy Solar Farm", date("2026-02-01")))
                .await
                .expect("stored");
            (state, project.id)
        }

        #[tokio::test]
        async fn an_invalid_draft_never_reaches_storage() {
            let (state, project) = with_a_project().await;

            let error = state
                .create_employee(
                    project.clone(),
                    EmployeeDraft::new("  ", "Randrianasolo", "", date("2026-02-01")),
                )
                .await
                .expect_err("a nameless, roleless employee is rejected");

            match error {
                AppError::Validation(errors) => {
                    assert!(errors.has("firstName"));
                    assert!(errors.has("role"));
                }
                other => panic!("expected validation, got {other:?}"),
            }
            assert!(state.list_employees(None).await.expect("query runs").is_empty());
        }

        #[tokio::test]
        async fn hiring_onto_a_project_that_is_gone_is_a_not_found() {
            let state = state().await;
            let result = state
                .create_employee(
                    ProjectId::from("ghost"),
                    EmployeeDraft::new("Rakoto", "Randrianasolo", "Site supervisor", date("2026-02-01")),
                )
                .await;

            assert!(matches!(result, Err(AppError::NotFound { entity: "project", .. })));
        }

        #[tokio::test]
        async fn an_invalid_edit_leaves_the_stored_employee_alone() {
            let (state, project) = with_a_project().await;
            let hired = state
                .create_employee(
                    project,
                    EmployeeDraft::new("Rakoto", "Randrianasolo", "Site supervisor", date("2026-02-01")),
                )
                .await
                .expect("hired");

            let mut bad =
                EmployeeDraft::new("Rakoto", "Randrianasolo", "Project manager", date("2026-02-01"));
            bad.birth_date = Some(date("2026-06-01"));
            assert!(state.update_employee(hired.id.clone(), bad).await.is_err());

            let unchanged = state
                .get_employee(hired.id)
                .await
                .expect("query runs")
                .expect("still there");
            assert_eq!(unchanged.role, "Site supervisor");
        }

        #[tokio::test]
        async fn stats_default_to_today_when_no_date_is_given() {
            let (state, project) = with_a_project().await;
            let hired = state
                .create_employee(
                    project,
                    EmployeeDraft::new("Rakoto", "Randrianasolo", "Site supervisor", date("2020-01-06")),
                )
                .await
                .expect("hired");

            let stats = state.employee_stats(hired.id, None).await.expect("query runs");
            assert_eq!(stats.as_of, Local::now().date_naive());
        }

        #[tokio::test]
        async fn the_whole_employee_lifecycle_runs_through_the_command_layer() {
            let (state, project) = with_a_project().await;

            let mut draft =
                EmployeeDraft::new("Rakoto", "Randrianasolo", "Site supervisor", date("2026-02-01"));
            draft.cin = Some("201 021 045".into());
            let hired = state.create_employee(project.clone(), draft).await.expect("hired");
            assert_eq!(hired.cin.as_deref(), Some("201021045"));

            // The project card and the portfolio KPI both see the new hire.
            let stats = state
                .project_stats(project.clone(), Some(date("2026-09-15")))
                .await
                .expect("query runs");
            assert_eq!(stats.headcount, 1);
            assert_eq!(state.portfolio_stats().await.expect("query runs").people, 1);

            let mut promotion =
                EmployeeDraft::new("Rakoto", "Randrianasolo", "Project manager", date("2026-02-01"));
            promotion.cin = Some("201021045".into());
            let promoted =
                state.update_employee(hired.id.clone(), promotion).await.expect("updated");
            assert_eq!(promoted.role, "Project manager");

            state.delete_employee(hired.id.clone()).await.expect("removed");
            assert_eq!(state.get_employee(hired.id).await.expect("query runs"), None);
            assert_eq!(state.portfolio_stats().await.expect("query runs").people, 0);
        }

        #[tokio::test]
        async fn deleting_a_project_removes_the_people_on_it() {
            let (state, project) = with_a_project().await;
            state
                .create_employee(
                    project.clone(),
                    EmployeeDraft::new("Rakoto", "Randrianasolo", "Site supervisor", date("2026-02-01")),
                )
                .await
                .expect("hired");

            state.delete_project(project).await.expect("deleted");

            assert!(state.list_employees(None).await.expect("query runs").is_empty());
            assert_eq!(state.portfolio_stats().await.expect("query runs").people, 0);
        }
    }

    mod attendance {
        use super::*;

        use crate::domain::attendance::{AttendanceSource, WorkedDays, WorkedTime};
        use crate::domain::calendar::{DayLength, WeekdayMask};
        use crate::domain::employee::EmployeeDraft;

        fn september() -> YearMonth {
            YearMonth::new(2026, 9).expect("september")
        }

        /// A project with a Mon–Fri, eight-hour calendar and two long-serving
        /// employees.
        async fn staffed() -> (AppState, ProjectId, Vec<Employee>) {
            let state = state().await;
            let project = state
                .create_project(ProjectDraft::new("Ambatolampy Solar Farm", date("2020-01-01")))
                .await
                .expect("stored");

            let mut people = Vec::new();
            for (first, last) in [("Rakoto", "Randrianasolo"), ("Fara", "Rasoanaivo")] {
                people.push(
                    state
                        .create_employee(
                            project.id.clone(),
                            EmployeeDraft::new(first, last, "Operative", date("2020-01-06")),
                        )
                        .await
                        .expect("hired"),
                );
            }
            (state, project.id, people)
        }

        #[tokio::test]
        async fn an_invalid_row_never_reaches_storage() {
            let (state, project, people) = staffed().await;

            let error = state
                .record_attendance(people[0].id.clone(), september(), AttendanceDraft::of_days(31, 480))
                .await
                .expect_err("September has 30 days");

            match error {
                AppError::Validation(errors) => assert!(errors.has("daysWorked")),
                other => panic!("expected validation, got {other:?}"),
            }

            let sheet = state.attendance_sheet(project, september()).await.expect("query runs");
            assert_eq!(sheet.totals.recorded, 0);
        }

        #[tokio::test]
        async fn a_month_before_the_hire_date_is_rejected_with_the_hire_month_named() {
            let state = state().await;
            let project = state
                .create_project(ProjectDraft::new("Ambatolampy Solar Farm", date("2026-01-01")))
                .await
                .expect("stored");
            let late = state
                .create_employee(
                    project.id,
                    EmployeeDraft::new("Hery", "Rabemananjara", "Crane operator", date("2026-04-01")),
                )
                .await
                .expect("hired");

            let error = state
                .record_attendance(
                    late.id,
                    YearMonth::new(2026, 3).expect("march"),
                    AttendanceDraft::of_days(20, 480),
                )
                .await
                .expect_err("hired in April, recorded in March");

            match error {
                AppError::Validation(errors) => assert!(errors.has("period")),
                other => panic!("expected validation, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn recording_against_somebody_who_does_not_exist_is_a_not_found() {
            let (state, _, _) = staffed().await;
            let result = state
                .record_attendance(
                    EmployeeId::from("ghost"),
                    september(),
                    AttendanceDraft::of_days(20, 480),
                )
                .await;

            assert!(matches!(result, Err(AppError::NotFound { entity: "employee", .. })));
        }

        #[tokio::test]
        async fn filling_from_the_schedule_seeds_the_whole_grid() {
            let (state, project, _) = staffed().await;

            let sheet = state
                .fill_attendance_from_schedule(project, september())
                .await
                .expect("fill runs");

            assert_eq!(sheet.totals.recorded, 2);
            assert_eq!(sheet.totals.missing, 0);
            // September 2026 has 22 weekdays; two people at 8 hours a day.
            assert_eq!(sheet.total_days(), WorkedDays::from_days(44));
            assert_eq!(sheet.totals.hours_worked_minutes, 2 * 176 * 60);
            assert!(sheet
                .rows
                .iter()
                .all(|row| row.entry.as_ref().expect("recorded").source
                    == AttendanceSource::Schedule));
        }

        #[tokio::test]
        async fn filling_follows_the_projects_own_calendar_and_holidays() {
            let state = state().await;
            let mut draft = ProjectDraft::new("Toamasina Port Logistics", date("2020-01-01"));
            draft.working_days = WeekdayMask::MON_SAT;
            draft.day_length = DayLength::from_hours_and_minutes(7, 30).expect("7h30");
            let project = state.create_project(draft).await.expect("stored");

            state
                .add_project_holiday(
                    project.id.clone(),
                    HolidayDraft::new(date("2026-09-07"), "Site shutdown"),
                )
                .await
                .expect("holiday added");
            state
                .create_employee(
                    project.id.clone(),
                    EmployeeDraft::new("Mamy", "Andrianina", "Forklift driver", date("2020-01-06")),
                )
                .await
                .expect("hired");

            let sheet = state
                .fill_attendance_from_schedule(project.id, september())
                .await
                .expect("fill runs");

            // 26 Mon–Sat days less one holiday, at 7h30.
            assert_eq!(sheet.total_days(), WorkedDays::from_days(25));
            assert_eq!(sheet.totals.hours_worked_minutes, 25 * 450);
        }

        #[tokio::test]
        async fn filling_clips_to_the_part_of_the_month_somebody_was_employed_for() {
            let (state, project, _) = staffed().await;
            state
                .create_employee(
                    project.clone(),
                    // Tuesday; 15–30 September has 12 weekdays.
                    EmployeeDraft::new("Nivo", "Rajaonarison", "Carpenter", date("2026-09-15")),
                )
                .await
                .expect("hired");

            let sheet = state
                .fill_attendance_from_schedule(project, september())
                .await
                .expect("fill runs");

            assert_eq!(sheet.totals.recorded, 3);
            assert_eq!(sheet.total_days(), WorkedDays::from_days(22 + 22 + 12));
        }

        #[tokio::test]
        async fn filling_skips_anyone_not_yet_hired_that_month() {
            let (state, project, _) = staffed().await;
            state
                .create_employee(
                    project.clone(),
                    EmployeeDraft::new("Vola", "Rasoamanana", "Customs clerk", date("2026-11-02")),
                )
                .await
                .expect("hired");

            let sheet = state
                .fill_attendance_from_schedule(project, september())
                .await
                .expect("fill runs");

            assert_eq!(sheet.totals.recorded, 2, "the November hire has no September");
            assert_eq!(sheet.totals.missing, 1);
        }

        #[tokio::test]
        async fn filling_twice_is_the_same_as_filling_once() {
            let (state, project, _) = staffed().await;

            let first = state
                .fill_attendance_from_schedule(project.clone(), september())
                .await
                .expect("fill runs");
            let second = state
                .fill_attendance_from_schedule(project, september())
                .await
                .expect("fill runs again");

            assert_eq!(first.totals, second.totals);
            assert_eq!(first.rows.len(), second.rows.len());
        }

        #[tokio::test]
        async fn refilling_keeps_the_overtime_a_person_typed_in() {
            let (state, project, people) = staffed().await;

            // 22 days, 176 hours, plus 9 hours of overtime nobody's calendar
            // could have known about.
            state
                .record_attendance(
                    people[0].id.clone(),
                    september(),
                    AttendanceDraft::new(44, 176 * 60, 9 * 60),
                )
                .await
                .expect("recorded");

            state
                .fill_attendance_from_schedule(project, september())
                .await
                .expect("fill runs");

            let refilled = state
                .attendance_entry(people[0].id.clone(), september())
                .await
                .expect("query runs")
                .expect("recorded");
            assert_eq!(refilled.overtime, WorkedTime::from_hours(9), "overtime survives a refill");
            assert_eq!(refilled.days_worked, WorkedDays::from_days(22));
            assert_eq!(refilled.source, AttendanceSource::Schedule);
        }

        #[tokio::test]
        async fn a_manual_edit_after_a_fill_wins_and_says_so() {
            let (state, project, people) = staffed().await;
            state
                .fill_attendance_from_schedule(project, september())
                .await
                .expect("fill runs");

            let edited = state
                .record_attendance(
                    people[0].id.clone(),
                    september(),
                    AttendanceDraft::new(40, 160 * 60, 0),
                )
                .await
                .expect("recorded");

            assert_eq!(edited.days_worked, WorkedDays::from_days(20));
            assert_eq!(edited.source, AttendanceSource::Manual);
        }

        #[tokio::test]
        async fn clearing_a_month_leaves_it_blank_rather_than_zero() {
            let (state, project, people) = staffed().await;
            state
                .fill_attendance_from_schedule(project.clone(), september())
                .await
                .expect("fill runs");

            state
                .clear_attendance(people[0].id.clone(), september())
                .await
                .expect("clear runs")
                .expect("there was something to clear");

            let sheet = state.attendance_sheet(project, september()).await.expect("query runs");
            assert_eq!(sheet.totals.recorded, 1);
            assert_eq!(sheet.totals.missing, 1);
        }

        #[tokio::test]
        async fn one_persons_history_comes_back_newest_month_first() {
            let (state, _, people) = staffed().await;
            for month in [7, 8, 9] {
                state
                    .record_attendance(
                        people[0].id.clone(),
                        YearMonth::new(2026, month).expect("valid month"),
                        AttendanceDraft::of_days(20, 480),
                    )
                    .await
                    .expect("recorded");
            }

            let history =
                state.employee_attendance(people[0].id.clone()).await.expect("query runs");
            let months: Vec<u32> = history.iter().map(|entry| entry.period.month).collect();
            assert_eq!(months, [9, 8, 7]);
        }

        #[tokio::test]
        async fn filling_a_project_that_is_gone_is_a_not_found() {
            let (state, _, _) = staffed().await;
            let result = state
                .fill_attendance_from_schedule(ProjectId::from("ghost"), september())
                .await;

            assert!(matches!(result, Err(AppError::NotFound { entity: "project", .. })));
        }
    }

    mod contracts {
        use super::*;

        use crate::domain::contract::{ContractDraft, PayType, round_ariary};
        use crate::domain::employee::EmployeeDraft;

        const HIRED: &str = "2026-02-01";

        async fn employed() -> (AppState, ProjectId, EmployeeId) {
            let state = state().await;
            let project = state
                .create_project(ProjectDraft::new("Ambatolampy Solar Farm", date("2026-01-01")))
                .await
                .expect("stored");
            let employee = state
                .create_employee(
                    project.id.clone(),
                    EmployeeDraft::new("Rakoto", "Randrianasolo", "Site supervisor", date(HIRED)),
                )
                .await
                .expect("hired");
            (state, project.id, employee.id)
        }

        #[tokio::test]
        async fn the_first_contract_and_a_raise_go_through_the_same_command() {
            let (state, _, employee) = employed().await;

            let first = state
                .amend_contract(employee.clone(), ContractDraft::monthly("3200000", date(HIRED)))
                .await
                .expect("first contract");
            assert!(first.is_current());

            let raise = state
                .amend_contract(
                    employee.clone(),
                    ContractDraft::monthly("3600000", date("2026-06-01")),
                )
                .await
                .expect("raise");

            assert!(raise.is_current());
            assert_eq!(state.contract_history(employee).await.expect("query runs").len(), 2);
        }

        #[tokio::test]
        async fn an_invalid_draft_never_reaches_storage() {
            let (state, _, employee) = employed().await;

            let error = state
                .amend_contract(employee.clone(), ContractDraft::monthly("0", date(HIRED)))
                .await
                .expect_err("a rate of zero is not a rate");

            match error {
                AppError::Validation(errors) => assert!(errors.has("rate")),
                other => panic!("expected validation, got {other:?}"),
            }
            assert!(state.contract_history(employee).await.expect("query runs").is_empty());
        }

        #[tokio::test]
        async fn the_command_reads_what_is_in_force_rather_than_trusting_the_caller() {
            let (state, _, employee) = employed().await;
            state
                .amend_contract(employee.clone(), ContractDraft::monthly("3200000", date("2026-06-01")))
                .await
                .expect("first contract");

            // Backdating behind the version in force is refused, and the
            // caller never had to say what that version was.
            let error = state
                .amend_contract(employee.clone(), ContractDraft::monthly("3600000", date("2026-03-01")))
                .await
                .expect_err("backdated behind the current version");

            match error {
                AppError::Validation(errors) => assert!(errors.has("effectiveFrom")),
                other => panic!("expected validation, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn terms_cannot_start_before_the_employee_was_hired() {
            let (state, _, employee) = employed().await;

            let error = state
                .amend_contract(employee, ContractDraft::monthly("3200000", date("2026-01-01")))
                .await
                .expect_err("before the hire date");

            match error {
                AppError::Validation(errors) => assert!(errors.has("effectiveFrom")),
                other => panic!("expected validation, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn a_contract_for_somebody_who_does_not_exist_is_a_not_found() {
            let (state, _, _) = employed().await;
            let result = state
                .amend_contract(
                    EmployeeId::from("ghost"),
                    ContractDraft::monthly("3200000", date(HIRED)),
                )
                .await;

            assert!(matches!(result, Err(AppError::NotFound { entity: "employee", .. })));
        }

        #[tokio::test]
        async fn the_current_contract_defaults_to_today() {
            let (state, _, employee) = employed().await;
            state
                .amend_contract(employee.clone(), ContractDraft::monthly("3200000", date("2020-01-01")))
                .await
                .expect_err("before the hire date, so nothing is stored");

            state
                .amend_contract(employee.clone(), ContractDraft::monthly("3200000", date(HIRED)))
                .await
                .expect("first contract");

            let now = state
                .current_contract(employee, None)
                .await
                .expect("query runs")
                .expect("in force today");
            assert_eq!(now.terms.rate.to_string(), "3200000.0000");
        }

        #[tokio::test]
        async fn the_pay_basis_conversions_reach_the_command_layer_intact() {
            let (state, _, employee) = employed().await;
            let mut draft = ContractDraft::monthly("3200000", date(HIRED));
            draft.pay_type = PayType::Monthly;
            state.amend_contract(employee.clone(), draft).await.expect("written");

            let contract = state
                .current_contract(employee, Some(date("2026-03-01")))
                .await
                .expect("query runs")
                .expect("in force");

            // ÷ 26 and ÷ 173, rounded only here.
            assert_eq!(round_ariary(contract.terms.daily_equivalent()).to_string(), "123077");
            assert_eq!(round_ariary(contract.terms.hourly_equivalent()).to_string(), "18497");
        }

        #[tokio::test]
        async fn discarding_an_amendment_puts_the_previous_terms_back_in_force() {
            let (state, _, employee) = employed().await;
            state
                .amend_contract(employee.clone(), ContractDraft::monthly("3200000", date(HIRED)))
                .await
                .expect("first contract");
            state
                .amend_contract(employee.clone(), ContractDraft::monthly("9999999", date("2026-06-01")))
                .await
                .expect("a raise entered by mistake");

            state.discard_latest_contract(employee.clone()).await.expect("undone");

            let july = state
                .current_contract(employee, Some(date("2026-07-01")))
                .await
                .expect("query runs")
                .expect("in force");
            assert_eq!(july.terms.rate.to_string(), "3200000.0000");
        }

        #[tokio::test]
        async fn contracts_ending_soon_defaults_to_the_next_ninety_days() {
            let (state, project, employee) = employed().await;

            let mut soon = ContractDraft::monthly("3200000", date(HIRED));
            soon.end = Some(date(HIRED) + chrono::Duration::days(30));
            state.amend_contract(employee, soon).await.expect("written");

            let ending = state
                .contracts_ending(project.clone(), Some(date(HIRED)), None)
                .await
                .expect("query runs");
            assert_eq!(ending.len(), 1);

            // …and a shorter window leaves it out.
            let narrow = state
                .contracts_ending(project, Some(date(HIRED)), Some(7))
                .await
                .expect("query runs");
            assert!(narrow.is_empty());
        }
    }
}
