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
use crate::domain::employee::{
    Employee, EmployeeDraft, EmployeeFilter, EmployeeId, EmployeeStats,
};
use crate::domain::project::{
    Holiday, HolidayDraft, HolidayId, PortfolioStats, Project, ProjectDraft, ProjectFilter,
    ProjectId, ProjectStats,
};
use crate::error::Result;
use crate::repo::sqlite::{
    SqliteActivityRepository, SqliteEmployeeRepository, SqliteProjectRepository,
};
use crate::repo::{ActivityRepository, AuditEntry, EmployeeRepository, ProjectRepository};

/// How many audit rows the overview's activity list asks for by default.
pub const DEFAULT_ACTIVITY_LIMIT: u32 = 20;

/// Everything a command needs: one handle per storage trait, all over the
/// same pool.
#[derive(Clone)]
pub struct AppState {
    projects: Arc<dyn ProjectRepository>,
    employees: Arc<dyn EmployeeRepository>,
    activity: Arc<dyn ActivityRepository>,
}

impl AppState {
    pub fn new(db: Db) -> Self {
        AppState {
            projects: Arc::new(SqliteProjectRepository::new(db.clone())),
            employees: Arc::new(SqliteEmployeeRepository::new(db.clone())),
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
}
