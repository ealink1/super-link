//! Small path helpers shared by packaging and application code.

use std::path::{Component, Path, PathBuf};

/// Joins a POSIX-style archive member name onto a native path.
pub(crate) fn join_posix(root: &Path, name: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in name.split('/') {
        path.push(part);
    }
    path
}

/// Lexically removes `.` and `..` components without touching the filesystem.
pub(crate) fn clean_path(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                cleaned.pop();
            }
            other => cleaned.push(other.as_os_str()),
        }
    }
    cleaned
}

/// Reports whether a path is absolute and already free of relative components.
pub(crate) fn is_canonical_absolute(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    !path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
}

/// Appends a literal suffix to the last component of a path.
pub(crate) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut text = path.as_os_str().to_os_string();
    text.push(suffix);
    PathBuf::from(text)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{clean_path, is_canonical_absolute, join_posix, with_suffix};

    #[test]
    fn cleans_relative_components() {
        assert_eq!(clean_path(Path::new("/a/b/../c")), PathBuf::from("/a/c"));
        assert_eq!(clean_path(Path::new("/a/./b/")), PathBuf::from("/a/b"));
    }

    #[test]
    fn recognises_canonical_absolute_paths() {
        assert!(is_canonical_absolute(Path::new("/a/b")));
        assert!(!is_canonical_absolute(Path::new("a/b")));
        assert!(!is_canonical_absolute(Path::new("/a/../b")));
    }

    #[test]
    fn appends_suffixes_and_joins_posix_names() {
        assert_eq!(
            with_suffix(Path::new("/a/b"), ".backup-1"),
            PathBuf::from("/a/b.backup-1")
        );
        assert_eq!(
            join_posix(Path::new("/a"), "Contents/MacOS/bin"),
            Path::new("/a").join("Contents").join("MacOS").join("bin")
        );
    }
}
