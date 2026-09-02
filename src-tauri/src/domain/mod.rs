//! Domain types and rules. Nothing in here knows about SQL, Tauri or the UI.

pub mod calendar;
pub mod employee;
pub mod project;

use std::fmt;

use serde::Serialize;

/// One thing wrong with one field of a form.
///
/// Validation collects every problem rather than stopping at the first, so a
/// half-filled form comes back with all of its errors at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationError {
    pub field: &'static str,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: &'static str, message: impl Into<String>) -> Self {
        ValidationError { field, message: message.into() }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// Every problem with one submission. Never empty when returned as an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(transparent)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, field: &'static str, message: impl Into<String>) {
        self.0.push(ValidationError::new(field, message));
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ValidationError> {
        self.0.iter()
    }

    /// True when something is wrong with this specific field.
    pub fn has(&self, field: &str) -> bool {
        self.0.iter().any(|e| e.field == field)
    }

    /// `Ok(value)` when nothing went wrong, the errors otherwise.
    pub fn into_result<T>(self, value: T) -> Result<T, ValidationErrors> {
        if self.is_empty() {
            Ok(value)
        } else {
            Err(self)
        }
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let joined: Vec<String> = self.0.iter().map(ToString::to_string).collect();
        write!(f, "{}", joined.join("; "))
    }
}

impl std::error::Error for ValidationErrors {}

/// Trims a free-text field, folding blank into "not given".
pub(crate) fn normalise_optional(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Declares an opaque string-newtype identifier.
macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// A fresh random identifier.
            #[allow(clippy::new_without_default)]
            pub fn new() -> Self {
                $name(uuid::Uuid::new_v4().to_string())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                $name(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                $name(value.to_owned())
            }
        }
    };
}

pub(crate) use id_type;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_errors_pass_the_value_through() {
        assert_eq!(ValidationErrors::new().into_result(7), Ok(7));
    }

    #[test]
    fn errors_are_collected_not_short_circuited() {
        let mut errors = ValidationErrors::new();
        errors.push("name", "is required");
        errors.push("end", "is before the start");

        assert_eq!(errors.len(), 2);
        assert!(errors.has("name"));
        assert!(errors.has("end"));
        assert!(!errors.has("client"));
        assert_eq!(errors.clone().into_result(()), Err(errors));
    }

    #[test]
    fn optional_text_is_trimmed_and_blank_becomes_none() {
        assert_eq!(normalise_optional(Some("  JIRAMA ".into())), Some("JIRAMA".into()));
        assert_eq!(normalise_optional(Some("   ".into())), None);
        assert_eq!(normalise_optional(Some(String::new())), None);
        assert_eq!(normalise_optional(None), None);
    }
}
