//! Temporary directories removed on drop.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::error::{Result, UpdateError};

/// A private directory that is deleted unless it is explicitly kept.
pub(crate) struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// Creates a unique 0700 directory under `parent`.
    pub(crate) fn create(parent: &Path, prefix: &str) -> Result<Self> {
        for _ in 0..8 {
            let path = parent.join(format!("{prefix}{}", Uuid::new_v4().simple()));
            match create_private_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(UpdateError::io(format!("create {}", path.display()))(error));
                }
            }
        }
        Err(UpdateError::io(format!(
            "create a {prefix} directory under {}",
            parent.display()
        ))(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "every candidate name already exists",
        )))
    }

    /// Directory created for this scratch space.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}
