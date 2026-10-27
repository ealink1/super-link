//! Safe extraction of application archives into a staging directory.
//!
//! Archives are treated as hostile: absolute paths, parent components,
//! non-canonical names, links, special files, duplicate or case-colliding
//! entries, Windows device names and decompression bombs are all rejected
//! before a single byte is written.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use zip::ZipArchive;

use crate::error::{Result, UpdateError};
use crate::paths::join_posix;

/// Default decompression budget (2 GiB), matching the Go updater.
pub const DEFAULT_MAX_EXTRACTED_BYTES: u64 = 2 << 30;
/// Default entry count limit.
pub const DEFAULT_MAX_ENTRIES: usize = 5_000;

const MAX_NAME_BYTES: usize = 4096;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const FILE_TYPE_MASK: u32 = 0o170_000;
const FILE_TYPE_REGULAR: u32 = 0o100_000;
const FILE_TYPE_DIRECTORY: u32 = 0o040_000;

/// Extraction budgets; tests use smaller values than production.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_extracted_bytes: u64,
    pub max_entries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_extracted_bytes: DEFAULT_MAX_EXTRACTED_BYTES,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Zip,
    TarGz,
}

/// Extracts an archive and returns the single application root inside it.
pub fn stage_archive(archive: &Path, destination: &Path) -> Result<PathBuf> {
    stage_archive_with(archive, destination, Limits::default())
}

/// Extracts an archive with explicit limits.
///
/// The destination is cleared on failure so a partially written package can
/// never be mistaken for a complete one.
pub fn stage_archive_with(archive: &Path, destination: &Path, limits: Limits) -> Result<PathBuf> {
    prepare_destination(destination)?;
    let format = detect_format(archive)?;
    let outcome = match format {
        Format::Zip => extract_zip(archive, destination, limits),
        Format::TarGz => extract_tar_gz(archive, destination, limits),
    };
    if let Err(error) = outcome {
        let _ = fs::remove_dir_all(destination);
        return Err(error);
    }
    single_root(destination)
}

/// Rejects unsafe archive member names and returns the canonical form.
///
/// Mirrors the Go updater: POSIX separators only, no absolute or parent
/// components, no trailing dots or spaces, no Windows device names and no
/// silently normalised input.
pub fn safe_archive_path(name: &str) -> Result<String> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES {
        return Err(UpdateError::UnsafePath(name.to_owned()));
    }
    if name.starts_with('/')
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
        || name.contains("//")
    {
        return Err(UpdateError::UnsafePath(name.to_owned()));
    }
    let clean = clean_posix(name);
    if clean == "." || clean == ".." || clean.starts_with("../") {
        return Err(UpdateError::UnsafePath(name.to_owned()));
    }
    for part in clean.split('/') {
        if part == ".." || part.ends_with('.') || part.ends_with(' ') || is_device_name(part) {
            return Err(UpdateError::UnsafePath(name.to_owned()));
        }
    }
    if name.trim_end_matches('/') != clean {
        return Err(UpdateError::UnsafePath(name.to_owned()));
    }
    Ok(clean)
}

fn is_device_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).to_ascii_uppercase();
    match stem.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => {
            stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit()
                && stem.as_bytes()[3] != b'0'
        }
    }
}

fn clean_posix(name: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => match parts.last() {
                Some(last) if *last != ".." => {
                    parts.pop();
                }
                _ => parts.push(".."),
            },
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn detect_format(archive: &Path) -> Result<Format> {
    let mut file =
        File::open(archive).map_err(UpdateError::io(format!("open {}", archive.display())))?;
    let mut magic = [0u8; 4];
    let read = file
        .read(&mut magic)
        .map_err(UpdateError::io(format!("read {}", archive.display())))?;
    let magic = &magic[..read];
    if magic.starts_with(b"PK\x03\x04")
        || magic.starts_with(b"PK\x05\x06")
        || magic.starts_with(b"PK\x07\x08")
    {
        Ok(Format::Zip)
    } else if magic.starts_with(&[0x1f, 0x8b]) {
        Ok(Format::TarGz)
    } else {
        Err(UpdateError::Archive("unsupported archive format".into()))
    }
}

fn prepare_destination(destination: &Path) -> Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(UpdateError::Archive(
                    "extraction destination must be a real empty directory".into(),
                ));
            }
            let mut entries = fs::read_dir(destination)
                .map_err(UpdateError::io(format!("read {}", destination.display())))?;
            if entries.next().is_some() {
                return Err(UpdateError::Archive(
                    "extraction destination must be empty".into(),
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_dir(destination)?,
        Err(error) => {
            return Err(UpdateError::io(format!(
                "inspect {}",
                destination.display()
            ))(error));
        }
    }
    Ok(())
}

fn single_root(destination: &Path) -> Result<PathBuf> {
    let mut entries = fs::read_dir(destination)
        .map_err(UpdateError::io(format!("read {}", destination.display())))?;
    let first = entries.next();
    if entries.next().is_some() {
        return Err(UpdateError::Archive(
            "package archive must contain one application directory".into(),
        ));
    }
    let entry = first
        .ok_or_else(|| {
            UpdateError::Archive("package archive must contain one application directory".into())
        })?
        .map_err(UpdateError::io(format!("read {}", destination.display())))?;
    let file_type = entry.file_type().map_err(UpdateError::io(format!(
        "inspect {}",
        entry.path().display()
    )))?;
    if !file_type.is_dir() {
        return Err(UpdateError::Archive(
            "package archive must contain one application directory".into(),
        ));
    }
    Ok(entry.path())
}

fn extract_zip(archive: &Path, destination: &Path, limits: Limits) -> Result<()> {
    let file =
        File::open(archive).map_err(UpdateError::io(format!("open {}", archive.display())))?;
    let mut zip = ZipArchive::new(BufReader::new(file))
        .map_err(|error| UpdateError::Archive(error.to_string()))?;
    let count = zip.len();
    if count == 0 || count > limits.max_entries {
        return Err(UpdateError::Archive("invalid archive entry count".into()));
    }
    let mut seen = HashSet::with_capacity(count);
    let mut declared = 0u64;
    for index in 0..count {
        let entry = zip
            .by_index(index)
            .map_err(|error| UpdateError::Archive(error.to_string()))?;
        let name = safe_archive_path(entry.name())?;
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(UpdateError::DuplicateEntry(name));
        }
        if entry.is_dir() {
            continue;
        }
        if entry.is_symlink() {
            return Err(UpdateError::Archive(
                "archive contains a link or special file".into(),
            ));
        }
        if let Some(mode) = entry.unix_mode() {
            let kind = mode & FILE_TYPE_MASK;
            if kind != 0 && kind != FILE_TYPE_REGULAR && kind != FILE_TYPE_DIRECTORY {
                return Err(UpdateError::Archive(
                    "archive contains a link or special file".into(),
                ));
            }
        }
        declared = declared.saturating_add(entry.size());
        if declared > limits.max_extracted_bytes {
            return Err(UpdateError::ExtractionBudget(limits.max_extracted_bytes));
        }
    }

    let mut budget = Budget::new(limits.max_extracted_bytes);
    for index in 0..count {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| UpdateError::Archive(error.to_string()))?;
        let name = safe_archive_path(entry.name())?;
        let target = join_posix(destination, &name);
        if entry.is_dir() {
            create_dir(&target)?;
            continue;
        }
        let executable = entry.unix_mode().is_some_and(|mode| mode & 0o111 != 0);
        write_entry(&mut entry, &target, executable, &mut budget)?;
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, destination: &Path, limits: Limits) -> Result<()> {
    let file =
        File::open(archive).map_err(UpdateError::io(format!("open {}", archive.display())))?;
    let mut tar = tar::Archive::new(GzDecoder::new(BufReader::new(file)));
    let mut budget = Budget::new(limits.max_extracted_bytes);
    let mut seen: HashSet<String> = HashSet::new();
    let mut count = 0usize;
    let mut declared = 0u64;
    let entries = tar
        .entries()
        .map_err(|error| UpdateError::Archive(error.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| UpdateError::Archive(error.to_string()))?;
        count += 1;
        if count > limits.max_entries {
            return Err(UpdateError::Archive("invalid archive entry count".into()));
        }
        let raw = entry.path_bytes().into_owned();
        let name = std::str::from_utf8(&raw)
            .map_err(|_| UpdateError::UnsafePath(String::from_utf8_lossy(&raw).into_owned()))?;
        let Some(name) = strip_root_prefix(name) else {
            continue;
        };
        let name = safe_archive_path(name)?;
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(UpdateError::DuplicateEntry(name));
        }
        let entry_type = entry.header().entry_type();
        let target = join_posix(destination, &name);
        if entry_type.is_dir() {
            create_dir(&target)?;
            continue;
        }
        if !entry_type.is_file() {
            return Err(UpdateError::Archive(
                "archive contains a link or special file".into(),
            ));
        }
        declared = declared.saturating_add(entry.size());
        if declared > limits.max_extracted_bytes {
            return Err(UpdateError::ExtractionBudget(limits.max_extracted_bytes));
        }
        let executable = entry.header().mode().is_ok_and(|mode| mode & 0o111 != 0);
        write_entry(&mut entry, &target, executable, &mut budget)?;
    }
    Ok(())
}

fn strip_root_prefix(name: &str) -> Option<&str> {
    let name = name.strip_prefix("./").unwrap_or(name);
    if name.is_empty() || name == "." {
        None
    } else {
        Some(name)
    }
}

fn write_entry(
    reader: &mut impl Read,
    target: &Path,
    executable: bool,
    budget: &mut Budget,
) -> Result<()> {
    if let Some(parent) = target.parent() {
        create_dir(parent)?;
    }
    let mut file = create_file(target, executable)?;
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(UpdateError::io("read archive entry"))?;
        if read == 0 {
            break;
        }
        budget.consume(read as u64)?;
        file.write_all(&buffer[..read])
            .map_err(UpdateError::io(format!("write {}", target.display())))?;
    }
    file.sync_all()
        .map_err(UpdateError::io(format!("sync {}", target.display())))
}

pub(crate) fn create_dir(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(UpdateError::io(format!("create {}", path.display())))
}

pub(crate) fn create_file(path: &Path, executable: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if executable { 0o700 } else { 0o600 });
    }
    let _ = executable;
    options
        .open(path)
        .map_err(UpdateError::io(format!("create {}", path.display())))
}

struct Budget {
    limit: u64,
    remaining: u64,
}

impl Budget {
    fn new(limit: u64) -> Self {
        Self {
            limit,
            remaining: limit,
        }
    }

    fn consume(&mut self, bytes: u64) -> Result<()> {
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or(UpdateError::ExtractionBudget(self.limit))?;
        Ok(())
    }
}
