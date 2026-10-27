//! Staging a verified archive next to the installed application.

use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::apply::ApplyRequest;
use crate::archive::{create_dir, stage_archive};
use crate::copy::copy_tree;
use crate::error::{Result, UpdateError};
use crate::package::read_package;
use crate::paths::with_suffix;
use crate::scratch::Scratch;

/// Prefix shared with the Go updater for sibling staging directories.
pub(crate) const STAGE_PREFIX: &str = ".superlink-stage-";
const EXTRACT_PREFIX: &str = ".extract-";

/// Extracts a downloaded package and stages it beside the installed one.
///
/// `archive` must already have passed [`crate::verify_release`] or
/// [`crate::receive_artifact`]; this function does not re-check the signature.
/// The staging directory lives on the same filesystem as the target so the
/// later swap is a single `rename`. The caller owns the returned request and
/// passes it to [`crate::apply_staged`] once the application can be replaced.
pub fn prepare_update(
    archive: &Path,
    updates: &Path,
    target: &Path,
    version: &str,
) -> Result<ApplyRequest> {
    let target = fs::canonicalize(target)
        .map_err(UpdateError::io(format!("resolve {}", target.display())))?;
    read_package(&target)?;
    fs::create_dir_all(updates)
        .map_err(UpdateError::io(format!("create {}", updates.display())))?;
    let extraction = Scratch::create(updates, EXTRACT_PREFIX)?;
    let root = stage_archive(archive, extraction.path())?;
    let package = read_package(&root)?;
    if package.version != version {
        return Err(UpdateError::Request(
            "package and signed manifest versions differ".into(),
        ));
    }
    let token = Uuid::new_v4().simple().to_string();
    let parent = target
        .parent()
        .ok_or_else(|| UpdateError::Request("target has no parent directory".into()))?;
    let stage = parent.join(format!("{STAGE_PREFIX}{token}"));
    create_dir(&stage)?;
    if let Err(error) = copy_tree(&root, &stage) {
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }
    Ok(ApplyRequest {
        backup: with_suffix(&target, &format!(".backup-{token}")),
        stage,
        target,
        token,
        version: version.to_owned(),
        state: None,
    })
}
