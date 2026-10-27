//! Reading and atomically saving UTF-8 SQL documents.
//!
//! A document is capped at 1 MiB, must be valid UTF-8 and may not contain NUL
//! characters. Saving publishes a fully written, synced temporary file only
//! after the write succeeded, so a failed save leaves the previous document
//! untouched.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::atomic::AtomicTemp;
use crate::cancel::{self, Cancellation};
use crate::error::{FileError, Result};
use crate::limits::MAX_SQL_BYTES;

const SIZE_MESSAGE: &str = "SQL file must be UTF-8 without NUL characters and no larger than 1 MiB";

fn validate(text: &str) -> Result<()> {
    if text.len() > MAX_SQL_BYTES || text.contains('\0') {
        return Err(FileError::invalid(SIZE_MESSAGE));
    }
    Ok(())
}

/// Loads one SQL document, stripping a leading byte order mark.
pub fn read(cancel: &dyn Cancellation, path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    cancel::check(cancel)?;
    let file = File::open(path)
        .map_err(|error| FileError::io(format!("open {}", path.display()), error))?;
    let metadata = file
        .metadata()
        .map_err(|error| FileError::io(format!("inspect {}", path.display()), error))?;
    if !metadata.is_file() || metadata.len() > MAX_SQL_BYTES as u64 + 3 {
        return Err(FileError::invalid(
            "choose a regular SQL file no larger than 1 MiB",
        ));
    }
    let mut raw = Vec::new();
    file.take(MAX_SQL_BYTES as u64 + 4)
        .read_to_end(&mut raw)
        .map_err(|error| FileError::io(format!("read {}", path.display()), error))?;
    let text = String::from_utf8(raw).map_err(|_| FileError::invalid(SIZE_MESSAGE))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_string();
    validate(&text)?;
    cancel::check(cancel)?;
    Ok(text)
}

/// Stores one SQL document, replacing the destination only when allowed.
pub fn save(
    cancel: &dyn Cancellation,
    path: impl AsRef<Path>,
    text: &str,
    overwrite: bool,
) -> Result<()> {
    validate(text)?;
    cancel::check(cancel)?;
    let path = path.as_ref();
    if !path.is_absolute() {
        return Err(FileError::invalid("SQL file path must be absolute"));
    }
    let directory = crate::atomic::parent_directory(path)?;
    let mut temp = AtomicTemp::create(&directory, ".superlink-sql-")?;
    {
        let file = temp.file_mut()?;
        file.write_all(text.as_bytes())
            .map_err(|error| FileError::io("write SQL file", error))?;
    }
    cancel::check(cancel)?;
    temp.commit(path, overwrite)
}
