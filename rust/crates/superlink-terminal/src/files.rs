//! Remote file metadata and path validation.
//!
//! Paths are POSIX and lexical: no shell interpretation, no traversal outside
//! the name the user picked.

use crate::error::{Result, TerminalError};

/// Longest accepted remote path.
pub const MAX_REMOTE_PATH_BYTES: usize = 4096;

/// A remote directory entry, without reading file contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    pub name: String,
    /// OpenSSH-style mode string, for example "-rw-r-----" or "drwxr-xr-x".
    pub mode: String,
    pub size: i64,
    /// Modification time in seconds since the Unix epoch.
    pub modified: Option<i64>,
    pub directory: bool,
}

/// Renders SFTP permission bits the way the Go implementation did.
///
/// The type character follows Go: directory "d", symlink "L", otherwise "-".
pub fn format_mode(permissions: u32) -> String {
    let kind = match permissions & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'L',
        _ => '-',
    };
    let mut mode = String::with_capacity(10);
    mode.push(kind);
    for shift in [6u32, 3, 0] {
        mode.push(if permissions & (0o4 << shift) != 0 {
            'r'
        } else {
            '-'
        });
        mode.push(if permissions & (0o2 << shift) != 0 {
            'w'
        } else {
            '-'
        });
        mode.push(if permissions & (0o1 << shift) != 0 {
            'x'
        } else {
            '-'
        });
    }
    mode
}

/// Whether the permission word describes a directory.
pub fn is_directory(permissions: u32) -> bool {
    permissions & 0o170000 == 0o040000
}

/// Rejects empty, oversized, NUL-bearing and traversal-style file paths.
pub fn validate_remote_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > MAX_REMOTE_PATH_BYTES
        || path.contains('\0')
        || base_name(path) == "."
        || base_name(path) == ".."
        || normalize_remote_path(path) == "/"
    {
        return Err(TerminalError::InvalidRemotePath);
    }
    Ok(())
}

/// Lexically cleans a POSIX path the way path.Clean does.
pub fn normalize_remote_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => match parts.last() {
                Some(last) if *last != ".." => {
                    parts.pop();
                }
                _ if !absolute => parts.push(".."),
                _ => {}
            },
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

/// Parent directory of a remote path, in POSIX form.
pub fn remote_parent(path: &str) -> String {
    let normalized = normalize_remote_path(path);
    match normalized.rfind('/') {
        Some(0) => "/".to_owned(),
        Some(index) => normalized[..index].to_owned(),
        None => ".".to_owned(),
    }
}

/// Joins two remote path segments without duplicating separators.
pub fn remote_join(parent: &str, name: &str) -> String {
    if parent.is_empty() || parent == "." {
        name.to_owned()
    } else if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

/// Last element of the path, following path.Base semantics.
pub fn base_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    trimmed.rsplit('/').next().unwrap_or("/")
}

/// Rejects names that must never appear in a directory listing.
pub(crate) fn valid_entry_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_REMOTE_PATH_BYTES
        && !name.contains(['/', '\0'])
        && name != "."
        && name != ".."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_unix_modes_like_the_go_implementation() {
        assert_eq!(format_mode(0o100640), "-rw-r-----");
        assert_eq!(format_mode(0o040755), "drwxr-xr-x");
        assert_eq!(format_mode(0o120777), "Lrwxrwxrwx");
        assert_eq!(format_mode(0), "----------");
        assert!(is_directory(0o040750));
        assert!(!is_directory(0o100750));
    }

    #[test]
    fn normalizes_posix_paths() {
        assert_eq!(normalize_remote_path("/ott/../client"), "/client");
        assert_eq!(normalize_remote_path("a//b/./c/"), "a/b/c");
        assert_eq!(normalize_remote_path("/"), "/");
        assert_eq!(normalize_remote_path("."), ".");
        assert_eq!(normalize_remote_path("../x"), "../x");
        assert_eq!(normalize_remote_path("/../../x"), "/x");
    }

    #[test]
    fn refuses_unsafe_file_paths() {
        for path in ["", "/", ".", "..", "/a/..", "x\0y"] {
            assert!(
                validate_remote_path(path).is_err(),
                "{path} must be rejected"
            );
        }
        for path in ["/file", "file", "dir/file", "/a/b/../c", "dir/"] {
            assert!(
                validate_remote_path(path).is_ok(),
                "{path} must be accepted"
            );
        }
        assert!(validate_remote_path(&format!("/{}", "a".repeat(5000))).is_err());
    }

    #[test]
    fn base_names_follow_posix_rules() {
        assert_eq!(base_name("/a/b"), "b");
        assert_eq!(base_name("/a/b/"), "b");
        assert_eq!(base_name("/"), "/");
        assert_eq!(base_name("a/."), ".");
        assert_eq!(base_name("a/.."), "..");
    }

    #[test]
    fn parent_and_join_keep_absolute_roots() {
        assert_eq!(remote_parent("/a/b"), "/a");
        assert_eq!(remote_parent("/a"), "/");
        assert_eq!(remote_parent("a"), ".");
        assert_eq!(
            remote_join("/a", ".superlink-upload-1"),
            "/a/.superlink-upload-1"
        );
        assert_eq!(remote_join(".", "file"), "file");
    }

    #[test]
    fn listing_names_cannot_escape_their_directory() {
        assert!(valid_entry_name("file"));
        assert!(!valid_entry_name("../outside"));
        assert!(!valid_entry_name("a\0b"));
        assert!(!valid_entry_name(""));
        assert!(!valid_entry_name("."));
    }
}
