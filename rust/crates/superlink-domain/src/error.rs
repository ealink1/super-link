use thiserror::Error;

pub type DomainResult<T> = std::result::Result<T, DomainError>;

/// Stable domain failures. Messages are user facing and localized upstream.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("not found")]
    NotFound,
    #[error("connection changed; reload before saving")]
    Conflict,
    #[error("application is closing")]
    Closed,
    #[error("this connection is read-only")]
    ReadOnly,
    #[error("server write outcome is unknown; inspect the server before retrying")]
    UnknownOutcome,
    #[error("another operation is using this session")]
    Busy,
    #[error("saved query changed; reload before saving")]
    QueryConflict,
    #[error("{0}")]
    Validation(String),
    #[error("unknown data source type {0:?}")]
    UnknownDataSource(String),
}

impl DomainError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}
