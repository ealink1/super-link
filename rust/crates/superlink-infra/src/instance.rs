//! A per-workspace process lease. The file is retained so its inode stays stable.
use std::{
    fs::{File, OpenOptions},
    path::Path,
};
pub struct WorkspaceLease {
    _file: File,
}
impl WorkspaceLease {
    pub fn acquire(root: &Path) -> Result<Self, String> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(root.join(".instance.lock"))
            .map_err(|e| format!("无法打开工作区进程锁：{e}"))?;
        file.try_lock()
            .map_err(|_| "此工作区已在另一个 SuperLink 进程中打开".to_string())?;
        Ok(Self { _file: file })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_process_lease_and_release_on_drop() {
        let dir = std::env::temp_dir().join(format!("sl-lease-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let first = WorkspaceLease::acquire(&dir).unwrap();
        assert!(WorkspaceLease::acquire(&dir).is_err());
        drop(first);
        assert!(WorkspaceLease::acquire(&dir).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
