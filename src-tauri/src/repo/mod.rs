//! Storage traits. Every database call in the app goes through one of these.
//!
//! The point is not testability alone — it is that "single-user SQLite" is a
//! choice this app should be able to change without rewriting the domain.

pub mod sqlite;

use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::calendar::HolidaySet;
use crate::domain::employee::{
    Employee, EmployeeFilter, EmployeeId, EmployeeStats, ValidEmployee,
};
use crate::domain::project::{
    Holiday, HolidayId, PortfolioStats, Project, ProjectFilter, ProjectId, ProjectStats,
    ValidHoliday, ValidProject,
};
use crate::error::{AppError, Result};

/// Everything the projects screens need from storage.
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn create(&self, draft: ValidProject) -> Result<Project>;

    async fn get(&self, id: &ProjectId) -> Result<Option<Project>>;

    /// `get`, but a missing project is an error rather than `None`. Commands
    /// acting on a named project want this; the UI list wants `get`.
    async fn require(&self, id: &ProjectId) -> Result<Project> {
        self.get(id).await?.ok_or_else(|| AppError::not_found("project", id))
    }

    /// Projects matching the filter, ordered by name.
    async fn list(&self, filter: &ProjectFilter) -> Result<Vec<Project>>;

    /// Replaces every editable field. The project keeps its id and creation time.
    async fn update(&self, id: &ProjectId, draft: ValidProject) -> Result<Project>;

    /// Deletes the project and, by cascade, everything inside it.
    async fn delete(&self, id: &ProjectId) -> Result<Project>;

    async fn portfolio_stats(&self) -> Result<PortfolioStats>;

    async fn stats(&self, id: &ProjectId, as_of: NaiveDate) -> Result<ProjectStats>;

    async fn add_holiday(&self, project: &ProjectId, holiday: ValidHoliday) -> Result<Holiday>;

    async fn holidays(&self, project: &ProjectId) -> Result<Vec<Holiday>>;

    /// The same holidays as a set, ready for the work calendar.
    async fn holiday_set(&self, project: &ProjectId) -> Result<HolidaySet> {
        Ok(self.holidays(project).await?.into_iter().map(|h| h.date).collect())
    }

    async fn remove_holiday(&self, project: &ProjectId, holiday: &HolidayId) -> Result<()>;
}

/// Everything the employees screens need from storage.
#[async_trait]
pub trait EmployeeRepository: Send + Sync {
    /// Hires someone onto a project. The project has to exist; an employee
    /// belongs to exactly one, and never moves.
    async fn create(&self, project: &ProjectId, draft: ValidEmployee) -> Result<Employee>;

    async fn get(&self, id: &EmployeeId) -> Result<Option<Employee>>;

    async fn require(&self, id: &EmployeeId) -> Result<Employee> {
        self.get(id).await?.ok_or_else(|| AppError::not_found("employee", id))
    }

    /// Employees matching the filter, ordered by last name then first name.
    async fn list(&self, filter: &EmployeeFilter) -> Result<Vec<Employee>>;

    /// Replaces every editable field. Identity, project and creation time stay.
    async fn update(&self, id: &EmployeeId, draft: ValidEmployee) -> Result<Employee>;

    async fn delete(&self, id: &EmployeeId) -> Result<Employee>;

    /// How many people are on one project.
    async fn headcount(&self, project: &ProjectId) -> Result<u32>;

    async fn stats(&self, id: &EmployeeId, as_of: NaiveDate) -> Result<EmployeeStats>;
}

/// What happened to a record. The `audit_log` CHECK accepts nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditAction {
    Create,
    Update,
    Delete,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            AuditAction::Create => "create",
            AuditAction::Update => "update",
            AuditAction::Delete => "delete",
        }
    }
}

impl fmt::Display for AuditAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AuditAction {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "create" => Ok(AuditAction::Create),
            "update" => Ok(AuditAction::Update),
            "delete" => Ok(AuditAction::Delete),
            other => Err(format!("{other:?} is not an audit action")),
        }
    }
}

/// One line of the append-only audit log — and one row of the overview's
/// recent-activity list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub entity: String,
    pub entity_id: String,
    pub action: AuditAction,
    /// A JSON snapshot of the record as it was after the change (before it,
    /// for a delete). Opaque to the reader; it exists to answer questions
    /// later, not to be queried.
    pub detail: Option<String>,
}

/// Reading the audit log. Writing to it is not on any trait on purpose — it
/// happens inside the same transaction as the change it records.
#[async_trait]
pub trait ActivityRepository: Send + Sync {
    /// Most recent first.
    async fn recent_activity(&self, limit: u32) -> Result<Vec<AuditEntry>>;

    /// The history of one record, oldest first.
    async fn history(&self, entity: &str, entity_id: &str) -> Result<Vec<AuditEntry>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_actions_round_trip_through_their_stored_spelling() {
        for action in [AuditAction::Create, AuditAction::Update, AuditAction::Delete] {
            assert_eq!(action.as_str().parse::<AuditAction>(), Ok(action));
        }
        assert!("purge".parse::<AuditAction>().is_err());
    }
}
