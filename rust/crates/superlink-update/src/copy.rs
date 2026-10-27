//! Copying trees without ever following or preserving links.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use crate::archive::{create_dir, create_file};
use crate::error::{Result, UpdateError};

const COPY_BUFFER_BYTES: usize = 64 * 1024;

/// Copies every regular file under `source` into `target`.
///
/// Symbolic links, sockets, devices and FIFOs are rejected: a staged package
/// must contain only files and directories.
pub(crate) fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];
    while let Some((from, to)) = pending.pop() {
        let entries =
            fs::read_dir(&from).map_err(UpdateError::io(format!("read {}", from.display())))?;
        for entry in entries {
            let entry = entry.map_err(UpdateError::io(format!("read {}", from.display())))?;
            let file_type = entry.file_type().map_err(UpdateError::io(format!(
                "inspect {}",
                entry.path().display()
            )))?;
            let destination = to.join(entry.file_name());
            if file_type.is_symlink() {
                return Err(UpdateError::Package("application contains symlinks".into()));
            }
            if file_type.is_dir() {
                create_dir(&destination)?;
                pending.push((entry.path(), destination));
            } else if file_type.is_file() {
                copy_file(&entry.path(), &destination)?;
            } else {
                return Err(UpdateError::Package(
                    "application contains a special file".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Copies one regular file, preserving only its executable bit.
pub(crate) fn copy_file(source: &Path, target: &Path) -> Result<()> {
    let metadata =
        fs::metadata(source).map_err(UpdateError::io(format!("inspect {}", source.display())))?;
    if !metadata.is_file() {
        return Err(UpdateError::Package(format!(
            "{} is not a regular file",
            source.display()
        )));
    }
    let mut input =
        fs::File::open(source).map_err(UpdateError::io(format!("open {}", source.display())))?;
    let mut output = create_file(target, is_executable(&metadata))?;
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(UpdateError::io(format!("read {}", source.display())))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(UpdateError::io(format!("write {}", target.display())))?;
    }
    output
        .sync_all()
        .map_err(UpdateError::io(format!("sync {}", target.display())))
}

fn is_executable(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}
