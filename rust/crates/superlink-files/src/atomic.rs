//! Crash-safe file publication.
//!
//! A complete document is written to a private temporary file in the
//! destination directory, flushed to disk and only then linked or renamed into
//! place. A failed write never damages an existing destination and never
//! leaves a temporary file behind.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{FileError, Result};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A private temporary file that is removed unless it is committed.
#[derive(Debug)]
pub struct AtomicTemp {
    path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl AtomicTemp {
    /// Creates a temporary file next to its future destination.
    pub fn create(directory: &Path, prefix: &str) -> Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        for _ in 0..32 {
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let name = format!("{prefix}{}-{sequence}-{stamp}", std::process::id());
            let path = directory.join(name);
            match open_private(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                        committed: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(FileError::io(
                        format!("create temporary file in {}", directory.display()),
                        error,
                    ));
                }
            }
        }
        Err(FileError::invalid(
            "could not create a unique temporary file",
        ))
    }

    /// Path of the temporary file, still hidden from the destination.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writable handle used by the encoders.
    pub fn file_mut(&mut self) -> Result<&mut File> {
        self.file
            .as_mut()
            .ok_or_else(|| FileError::invalid("temporary file is already closed"))
    }

    /// Flushes the file contents to the storage device.
    pub fn sync(&mut self) -> Result<()> {
        let file = self.file_mut()?;
        file.sync_all()
            .map_err(|error| FileError::io("flush temporary file", error))
    }

    /// Closes the file without publishing it.
    pub fn close(&mut self) {
        self.file = None;
    }

    /// Publishes the completed file at the destination.
    ///
    /// With overwrite the destination is atomically replaced. Without it a hard
    /// link fails when the destination already exists, so an existing file is
    /// never truncated or replaced.
    pub fn commit(mut self, destination: &Path, overwrite: bool) -> Result<()> {
        self.sync()?;
        self.close();
        if overwrite {
            fs::rename(&self.path, destination).map_err(|error| {
                FileError::io(format!("publish {}", destination.display()), error)
            })?;
        } else {
            match fs::hard_link(&self.path, destination) {
                Ok(()) => {
                    let _ = fs::remove_file(&self.path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    return Err(FileError::AlreadyExists);
                }
                Err(error) => {
                    return Err(FileError::io(
                        format!("publish {}", destination.display()),
                        error,
                    ));
                }
            }
        }
        self.committed = true;
        Ok(())
    }
}

impl Drop for AtomicTemp {
    fn drop(&mut self) {
        if !self.committed {
            self.file = None;
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn open_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Returns the parent directory a temporary file must live in.
pub fn parent_directory(path: &Path) -> Result<PathBuf> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => Ok(parent.to_path_buf()),
        _ => std::env::current_dir()
            .map_err(|error| FileError::io("resolve current directory", error)),
    }
}

/// Makes a path absolute without requiring it to exist yet.
pub fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).map_err(|error| FileError::io("resolve destination path", error))
}
