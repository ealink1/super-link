//! Atomic replacement of an installed application with a staged package.
//!
//! Replacement is two renames on one filesystem: the live bundle moves aside,
//! the staged bundle moves in. Any failure after the first rename restores the
//! previous bundle, and an optional first-launch check can trigger the same
//! rollback path.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::copy::copy_file;
use crate::error::{Result, UpdateError};
use crate::package::read_package;
use crate::paths::{is_canonical_absolute, with_suffix};
use crate::prepare::STAGE_PREFIX;
use crate::scratch::Scratch;

/// Local state database copied before the swap and restored on rollback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateRestore {
    pub source: PathBuf,
    pub backup: PathBuf,
}

/// Everything needed to replace one installed bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyRequest {
    pub target: PathBuf,
    pub stage: PathBuf,
    pub backup: PathBuf,
    pub token: String,
    pub version: String,
    pub state: Option<StateRestore>,
}

impl ApplyRequest {
    /// Attaches a state database that must survive a failed upgrade.
    pub fn with_state(mut self, source: impl Into<PathBuf>, backup: impl Into<PathBuf>) -> Self {
        self.state = Some(StateRestore {
            source: source.into(),
            backup: backup.into(),
        });
        self
    }
}

/// Outcome of an attempted replacement; also written to `last-update.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub version: String,
    pub success: bool,
    pub rolled_back: bool,
    pub backup: PathBuf,
    pub message: String,
    pub time: DateTime<Utc>,
}

impl ApplyReport {
    fn new(request: &ApplyRequest) -> Self {
        Self {
            version: request.version.clone(),
            success: false,
            rolled_back: false,
            backup: request.backup.clone(),
            message: String::new(),
            time: Utc::now(),
        }
    }

    /// Writes the report atomically with private permissions.
    pub fn write(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| UpdateError::Request("report path has no parent".into()))?;
        fs::create_dir_all(parent)
            .map_err(UpdateError::io(format!("create {}", parent.display())))?;
        let scratch = Scratch::create(parent, ".report-")?;
        let temporary = scratch.path().join("last-update.json");
        let mut raw = serde_json::to_vec_pretty(self)
            .map_err(|error| UpdateError::Request(error.to_string()))?;
        raw.push(b'\n');
        let mut file = fs::File::create(&temporary)
            .map_err(UpdateError::io(format!("create {}", temporary.display())))?;
        file.write_all(&raw)
            .map_err(UpdateError::io(format!("write {}", temporary.display())))?;
        file.sync_all()
            .map_err(UpdateError::io(format!("sync {}", temporary.display())))?;
        fs::rename(&temporary, path).map_err(UpdateError::io(format!("replace {}", path.display())))
    }
}

/// Validates paths, package identity and versions before anything moves.
pub fn validate_request(request: &ApplyRequest) -> Result<()> {
    for path in [&request.target, &request.stage, &request.backup] {
        if !is_canonical_absolute(path) {
            return Err(UpdateError::Request(format!(
                "{} must be an absolute canonical path",
                path.display()
            )));
        }
    }
    if request.token.is_empty()
        || !request
            .token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(UpdateError::Request("invalid update identity".into()));
    }
    let parent = request
        .target
        .parent()
        .ok_or_else(|| UpdateError::Request("target has no parent directory".into()))?;
    if request.stage.parent() != Some(parent) || request.backup.parent() != Some(parent) {
        return Err(UpdateError::Request(
            "update staging and backup must be siblings of the target".into(),
        ));
    }
    if request.stage == request.target || request.backup == request.target {
        return Err(UpdateError::Request(
            "update staging and backup must be distinct from the target".into(),
        ));
    }
    let stage_name = request
        .stage
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !stage_name.starts_with(STAGE_PREFIX) {
        return Err(UpdateError::Request("invalid update staging name".into()));
    }
    if request.backup != with_suffix(&request.target, &format!(".backup-{}", request.token)) {
        return Err(UpdateError::Request("invalid update backup path".into()));
    }
    if exists(&request.backup)? {
        return Err(UpdateError::Request(
            "update backup path already exists".into(),
        ));
    }
    read_package(&request.target)?;
    let staged = read_package(&request.stage)?;
    if staged.version != request.version {
        return Err(UpdateError::Request("staged version mismatch".into()));
    }
    if let Some(state) = &request.state {
        for path in [&state.source, &state.backup] {
            if !is_canonical_absolute(path) {
                return Err(UpdateError::Request(format!(
                    "{} must be an absolute canonical path",
                    path.display()
                )));
            }
        }
        if state.source == state.backup {
            return Err(UpdateError::Request(
                "state backup must differ from the state database".into(),
            ));
        }
        if exists(&state.backup)? {
            return Err(UpdateError::Request("state backup already exists".into()));
        }
    }
    Ok(())
}

/// Replaces the installed bundle with a validated staged one.
///
/// The previous bundle is kept beside the target as a backup; nothing is
/// deleted automatically.
pub fn apply_staged(request: &ApplyRequest) -> ApplyReport {
    apply_staged_with(request, |_| Ok(()))
}

/// Replaces the bundle and runs `launch`, rolling back if it fails.
///
/// `launch` is expected to start the new build and confirm its health. It must
/// not replace the running executable itself; in tests it is a closure.
pub fn apply_staged_with<F>(request: &ApplyRequest, launch: F) -> ApplyReport
where
    F: FnOnce(&ApplyRequest) -> Result<()>,
{
    let mut report = ApplyReport::new(request);
    if let Err(error) = validate_request(request) {
        report.message = error.to_string();
        return report;
    }
    if let Some(state) = &request.state
        && let Err(error) = backup_state(state)
    {
        report.message = format!("local state backup failed: {error}");
        return report;
    }
    if let Err(error) = fs::rename(&request.target, &request.backup) {
        report.message =
            UpdateError::io(format!("move {} aside", request.target.display()))(error).to_string();
        return report;
    }
    if let Err(error) = fs::rename(&request.stage, &request.target) {
        let rolled_back = fs::rename(&request.backup, &request.target).is_ok();
        report.rolled_back = rolled_back;
        report.message =
            UpdateError::io(format!("move {} into place", request.stage.display()))(error)
                .to_string();
        return report;
    }
    if let Err(error) = launch(request) {
        rollback(request, vec![error.to_string()], &mut report);
        return report;
    }
    report.success = true;
    report.message = "Application replaced; the previous version is kept as a backup".into();
    report
}

fn rollback(request: &ApplyRequest, mut messages: Vec<String>, report: &mut ApplyReport) {
    let mut complete = true;
    if let Some(state) = &request.state
        && let Err(error) = restore_state(state)
    {
        messages.push(error.to_string());
        complete = false;
    }
    let failed = with_suffix(&request.stage, ".failed");
    match fs::rename(&request.target, &failed) {
        Ok(()) => {
            if let Err(error) = fs::rename(&request.backup, &request.target) {
                messages.push(
                    UpdateError::io(format!("restore {}", request.target.display()))(error)
                        .to_string(),
                );
                complete = false;
            }
        }
        Err(error) => {
            messages.push(
                UpdateError::io(format!("move {} aside", request.target.display()))(error)
                    .to_string(),
            );
            complete = false;
        }
    }
    report.rolled_back = complete;
    report.message = messages.join("; ");
}

fn backup_state(state: &StateRestore) -> Result<()> {
    if let Some(parent) = state.backup.parent() {
        fs::create_dir_all(parent)
            .map_err(UpdateError::io(format!("create {}", parent.display())))?;
    }
    copy_file(&state.source, &state.backup)
}

fn restore_state(state: &StateRestore) -> Result<()> {
    let parent = state
        .source
        .parent()
        .ok_or_else(|| UpdateError::Request("state database has no parent".into()))?;
    let scratch = Scratch::create(parent, ".restore-")?;
    let temporary = scratch.path().join("state");
    copy_file(&state.backup, &temporary)?;
    for suffix in ["-wal", "-shm"] {
        match fs::remove_file(with_suffix(&state.source, suffix)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(UpdateError::io(format!(
                    "remove {}",
                    state.source.display()
                ))(error));
            }
        }
    }
    match fs::rename(&temporary, &state.source) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Windows refuses to replace an existing file; the database is
            // closed at this point, so removing it is safe.
            let _ = fs::remove_file(&state.source);
            fs::rename(&temporary, &state.source).map_err(UpdateError::io(format!(
                "restore {}",
                state.source.display()
            )))
        }
    }
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(UpdateError::io(format!("inspect {}", path.display()))(
            error,
        )),
    }
}
