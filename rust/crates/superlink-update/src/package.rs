//! Package identity metadata shipped inside every application bundle.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::archive::safe_archive_path;
use crate::error::{Result, UpdateError};
use crate::paths::{clean_path, join_posix};
use crate::platform::{Arch, Os, Platform};

/// Bundle identifier expected in every package marker.
pub const APP_ID: &str = "io.github.ealink1.superlink";
/// Marker file name at the package root or inside `Contents/Resources`.
pub const MARKER: &str = "superlink.package.json";
/// Largest marker file accepted.
pub const MAX_MARKER_BYTES: u64 = 4096;

const MACOS_MARKER: &str = "Contents/Resources/superlink.package.json";

/// Identity of an installed application bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub id: String,
    pub version: String,
    pub os: Os,
    pub arch: Arch,
    pub executable: String,
    pub helper: String,
}

/// Reads the marker of an installed package for the current platform.
pub fn read_package(root: &Path) -> Result<Package> {
    let platform = Platform::current()
        .ok_or_else(|| UpdateError::Request("this platform has no release artifacts".into()))?;
    read_package_for(root, platform)
}

/// Reads the marker of a package and checks it belongs to `platform`.
pub fn read_package_for(root: &Path, platform: Platform) -> Result<Package> {
    let metadata = fs::symlink_metadata(root)
        .map_err(UpdateError::io(format!("inspect {}", root.display())))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UpdateError::Package(
            "application root must be a real directory".into(),
        ));
    }
    let package: Package = serde_json::from_slice(&read_marker(root)?)
        .map_err(|error| UpdateError::Package(error.to_string()))?;
    if package.id != APP_ID
        || package.version.is_empty()
        || !platform.matches(package.os, package.arch)
    {
        return Err(UpdateError::Package(
            "package identity or platform mismatch".into(),
        ));
    }
    for name in [&package.executable, &package.helper] {
        safe_archive_path(name)?;
        let path = join_posix(root, name);
        let info = fs::symlink_metadata(&path)
            .map_err(UpdateError::io(format!("inspect {}", path.display())))?;
        if info.file_type().is_symlink() || !info.is_file() {
            return Err(UpdateError::Package(format!(
                "package entry point {name} must be a regular file"
            )));
        }
    }
    Ok(package)
}

/// Resolves the bundle root of the running executable for this platform.
pub fn running_root(executable: &Path) -> Result<PathBuf> {
    let platform = Platform::current()
        .ok_or_else(|| UpdateError::Request("this platform has no release artifacts".into()))?;
    running_root_for(executable, platform)
}

/// Resolves the bundle root of `executable` and checks it matches its marker.
///
/// A bare development binary has no marker, so callers must treat
/// [`UpdateError::NotPackaged`] as "install the downloaded package manually".
pub fn running_root_for(executable: &Path, platform: Platform) -> Result<PathBuf> {
    let executable = fs::canonicalize(executable)
        .map_err(UpdateError::io(format!("resolve {}", executable.display())))?;
    let parent = executable
        .parent()
        .ok_or(UpdateError::NotPackaged)?
        .to_path_buf();
    let root = if platform.os == Os::Darwin {
        clean_path(&parent.join("..").join(".."))
    } else {
        parent
    };
    let package = read_package_for(&root, platform).map_err(|_| UpdateError::NotPackaged)?;
    let expected = fs::canonicalize(join_posix(&root, &package.executable))
        .map_err(|_| UpdateError::NotPackaged)?;
    if expected != executable {
        return Err(UpdateError::ExecutableMismatch);
    }
    fs::canonicalize(&root).map_err(UpdateError::io(format!("resolve {}", root.display())))
}

fn read_marker(root: &Path) -> Result<Vec<u8>> {
    let mut missing: Option<std::io::Error> = None;
    for candidate in [MARKER, MACOS_MARKER] {
        let path = join_posix(root, candidate);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(UpdateError::Package(
                        "package metadata must be a regular file".into(),
                    ));
                }
                if metadata.len() > MAX_MARKER_BYTES {
                    return Err(UpdateError::Package("package metadata too large".into()));
                }
                let raw =
                    fs::read(&path).map_err(UpdateError::io(format!("read {}", path.display())))?;
                if raw.len() as u64 > MAX_MARKER_BYTES {
                    return Err(UpdateError::Package("package metadata too large".into()));
                }
                return Ok(raw);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing = Some(error),
            Err(error) => {
                return Err(UpdateError::io(format!("inspect {}", path.display()))(
                    error,
                ));
            }
        }
    }
    Err(UpdateError::io(format!(
        "read the {MARKER} marker under {}",
        root.display()
    ))(missing.unwrap_or_else(|| {
        std::io::Error::from(std::io::ErrorKind::NotFound)
    })))
}
