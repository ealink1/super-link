//! Error type shared by verification, staging and application.

use std::io;
use std::path::PathBuf;

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, UpdateError>;

/// Every failure mode of the update pipeline.
///
/// Messages are stable English text because they are surfaced in the UI and
/// written to the update report file.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("release signing public key is not configured")]
    MissingPublicKey,
    #[error("release manifest signature invalid")]
    SignatureInvalid,
    #[error("release manifest is invalid: {0}")]
    Manifest(String),
    #[error("artifact URL is not a trusted release address: {0}")]
    UntrustedUrl(String),
    #[error("artifact {path} is {actual} bytes, the signed manifest says {expected}")]
    SizeMismatch {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    #[error("artifact {path} has SHA256 {actual}, the signed manifest says {expected}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("artifact {filename} was not downloaded or did not verify")]
    ArtifactNotVerified { filename: String },
    #[error("this release has no matching {kind} artifact for {os}/{arch}")]
    NoPlatformArtifact {
        kind: &'static str,
        os: &'static str,
        arch: &'static str,
    },
    #[error("unsafe archive path {0:?}")]
    UnsafePath(String),
    #[error("archive is invalid: {0}")]
    Archive(String),
    #[error("archive entry {0:?} appears more than once")]
    DuplicateEntry(String),
    #[error("archive exceeds the {0} byte extraction budget")]
    ExtractionBudget(u64),
    #[error("application package is invalid: {0}")]
    Package(String),
    #[error(
        "in-place updates require a packaged application; install the downloaded package manually for this development build"
    )]
    NotPackaged,
    #[error("running executable differs from package metadata")]
    ExecutableMismatch,
    #[error("update request is invalid: {0}")]
    Request(String),
}

impl UpdateError {
    /// Builds an I/O error carrying the operation that failed.
    pub(crate) fn io(context: impl Into<String>) -> impl FnOnce(io::Error) -> Self {
        let context = context.into();
        move |source| Self::Io { context, source }
    }
}
