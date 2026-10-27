//! Signed release manifests, archive staging and atomic application of updates.
//!
//! The module is a native port of the Go updater in `internal/infra/update`,
//! the manifest format from `internal/infra/release` and the signing rules in
//! `cmd/release-sign`. Manifest JSON stays byte-compatible so the same signed
//! releases keep verifying.
//!
//! Verification is ordered: signature, manifest contents, per-file SHA256,
//! platform match, and only then does anything reach the filesystem.
//!
//! The intended flow is:
//!
//! 1. [`verify_release`] checks a signed manifest and the files already on disk,
//!    or [`receive_artifact`] streams a download into place after checking it.
//! 2. [`prepare_update`] extracts the verified archive beside the installed
//!    bundle and returns an [`ApplyRequest`].
//! 3. [`apply_staged`] swaps the bundles with two renames, or
//!    [`apply_staged_with`] additionally rolls back when the new build fails
//!    its first-launch check.
//!
//! Transport, process launching and workspace locking stay with the caller so
//! this crate remains free of I/O beyond the filesystem.

mod apply;
mod archive;
mod copy;
mod download;
mod error;
mod manifest;
mod package;
mod paths;
mod platform;
mod prepare;
mod scratch;
mod trusted_url;
mod verify;
mod version;

pub use apply::{
    ApplyReport, ApplyRequest, StateRestore, apply_staged, apply_staged_with, validate_request,
};
pub use archive::{Limits, safe_archive_path, stage_archive, stage_archive_with};
pub use download::receive_artifact;
pub use error::{Result, UpdateError};
pub use manifest::{
    APP_ARTIFACT_ID, Artifact, ArtifactKind, Channel, DRIVER_PROTOCOL, Manifest, SCHEMA_VERSION,
};
pub use package::{
    APP_ID, MARKER, Package, read_package, read_package_for, running_root, running_root_for,
};
pub use platform::{Arch, Os, Platform};
pub use prepare::prepare_update;
pub use trusted_url::trusted_url;
pub use verify::{
    Verification, VerifiedArtifact, sha256_file, verify_artifacts, verify_manifest, verify_release,
};
pub use version::{Version, is_newer};
