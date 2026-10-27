//! Errors reported by file import, export and SQL document handling.

use std::io;

use thiserror::Error;

/// Result alias used across the crate.
pub type Result<T> = std::result::Result<T, FileError>;

/// One failure mode of reading or writing a data file.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FileError {
    /// The request or the file content is invalid; the message is user facing.
    #[error("{0}")]
    Invalid(String),
    /// The requested export format is not supported.
    #[error("unsupported export format")]
    UnsupportedFormat,
    /// The destination exists and the caller asked not to overwrite it.
    #[error("destination already exists")]
    AlreadyExists,
    /// The caller cancelled the operation.
    #[error("operation cancelled")]
    Cancelled,
    /// An operating-system error with the action that failed.
    #[error("{context}: {source}")]
    Io {
        /// What the crate was doing when the error happened.
        context: String,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// A JSON document could not be parsed.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A CSV record could not be parsed.
    #[error("CSV: {0}")]
    Csv(#[from] csv::Error),
    /// An XLSX workbook could not be read or written.
    #[error("XLSX: {0}")]
    Xlsx(String),
}

impl FileError {
    /// Builds an invalid-input error from anything displayable.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    /// Wraps an I/O error with the action that failed.
    pub fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

impl From<io::Error> for FileError {
    fn from(source: io::Error) -> Self {
        Self::io("file operation failed", source)
    }
}
