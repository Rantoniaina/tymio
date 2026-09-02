//! The one error type that crosses the IPC boundary.

use serde::{Serialize, Serializer};

use crate::domain::calendar::CalendarError;
use crate::domain::ValidationErrors;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The form was wrong. Carries every field that failed, so the UI can
    /// mark them all rather than one at a time.
    #[error("{0}")]
    Validation(#[from] ValidationErrors),

    #[error("no {entity} with id {id}")]
    NotFound { entity: &'static str, id: String },

    /// A uniqueness rule was broken — a second holiday on the same date, say.
    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    Calendar(#[from] CalendarError),

    /// A row in the database does not fit the domain type it should map to.
    /// Means a migration and the code have drifted apart.
    #[error("corrupt {entity} row {id}: {detail}")]
    CorruptRow { entity: &'static str, id: String, detail: String },

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("{0}")]
    Storage(String),
}

impl AppError {
    pub fn not_found(entity: &'static str, id: impl ToString) -> Self {
        AppError::NotFound { entity, id: id.to_string() }
    }

    /// A stable machine-readable tag, so the front end can branch on the kind
    /// of failure without matching on message text.
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "validation",
            AppError::NotFound { .. } => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Calendar(_) => "calendar",
            AppError::CorruptRow { .. } => "corrupt_row",
            AppError::Database(_) | AppError::Migration(_) => "database",
            AppError::Storage(_) => "storage",
        }
    }

    /// Turns SQLite's constraint failures into a domain-shaped error, so the
    /// UI sees "that date already has a holiday" and not an SQLite code.
    pub fn from_sqlx(error: sqlx::Error, on_unique: impl FnOnce() -> String) -> Self {
        match &error {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                AppError::Conflict(on_unique())
            }
            _ => AppError::Database(error),
        }
    }
}

/// Serialised as `{ kind, message, fields }` — `fields` is present only for
/// validation failures, where the UI needs per-field messages.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("AppError", 3)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        match self {
            AppError::Validation(errors) => state.serialize_field("fields", errors)?,
            _ => state.serialize_field("fields", &Vec::<()>::new())?,
        }
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_errors_reach_the_front_end_field_by_field() {
        let mut errors = ValidationErrors::new();
        errors.push("name", "Project name is required");
        let json = serde_json::to_value(AppError::Validation(errors)).expect("serialisable");

        assert_eq!(json["kind"], "validation");
        assert_eq!(json["fields"][0]["field"], "name");
        assert_eq!(json["fields"][0]["message"], "Project name is required");
    }

    #[test]
    fn other_errors_carry_a_kind_and_an_empty_field_list() {
        let json = serde_json::to_value(AppError::not_found("project", "p1")).expect("serialisable");

        assert_eq!(json["kind"], "not_found");
        assert_eq!(json["message"], "no project with id p1");
        assert!(json["fields"].as_array().expect("array").is_empty());
    }
}
