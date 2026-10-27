use std::fmt;

/// One user-facing failure from a SQL builder. Messages used to be plain Go
/// errors, so they stay opaque text: callers show them and never branch on
/// their shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlError {
    message: String,
}

impl SqlError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SqlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SqlError {}

pub type SqlResult<T> = Result<T, SqlError>;
