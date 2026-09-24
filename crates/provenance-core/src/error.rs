//! Crate-wide error type. User-facing mapping happens at the Tauri boundary.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid input: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("session is {status} and cannot be changed; {action} is blocked")]
    LockedMutation { status: String, action: String },

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl CoreError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// Stable machine-readable code for the Tauri/IPC boundary.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::LockedMutation { .. } => "locked",
            Self::Database(_) => "database",
        }
    }

    /// Safe end-user message. Never leaks SQL or paths.
    #[must_use]
    pub fn user_message(&self) -> String {
        match self {
            Self::Validation(m) | Self::NotFound(m) | Self::Conflict(m) => m.clone(),
            Self::LockedMutation { status, action } => {
                format!("This session is {status}, so {action} is blocked.")
            }
            Self::Database(_) => "Local database unavailable. Try reopening the app.".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable() {
        assert_eq!(CoreError::validation("x").code(), "validation");
        assert_eq!(CoreError::not_found("x").code(), "not_found");
        assert_eq!(
            CoreError::LockedMutation {
                status: "locked".to_string(),
                action: "editing".to_string(),
            }
            .code(),
            "locked"
        );
    }

    #[test]
    fn database_errors_do_not_leak_internals() {
        let err = CoreError::Database(sqlx::Error::RowNotFound);
        assert_eq!(err.code(), "database");
        assert!(!err.user_message().contains("RowNotFound"));
    }
}
