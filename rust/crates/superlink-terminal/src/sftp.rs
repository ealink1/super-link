//! SFTP operations over the verified SSH transport.
//!
//! Every transfer is streamed, bounded by [TransferOptions::max_bytes] and
//! cancellable between chunks. Uploads write to a staging name and only rename
//! into place, so an interrupted transfer never leaves a partial file behind.

use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use tokio::runtime::Runtime;
use tokio::time::{Duration, timeout};

use crate::error::{Result, TerminalError};
use crate::files::{
    MAX_REMOTE_PATH_BYTES, RemoteFile, base_name, format_mode, remote_join, remote_parent,
    valid_entry_name, validate_remote_path,
};
use crate::runtime::runtime;
use crate::ssh::SshClient;
use crate::transfer::{CancelToken, TransferOptions, copy_limited};

/// A single directory listing never exceeds this many entries.
pub const MAX_DIRECTORY_ENTRIES: usize = 10_000;
/// Prefix for in-flight uploads; never shown as a finished file.
const UPLOAD_STAGING_PREFIX: &str = ".superlink-upload-";
/// Budget for opening the SFTP subsystem.
const SUBSYSTEM_TIMEOUT: Duration = Duration::from_secs(15);

/// One SFTP subsystem channel. Open one per concurrent transfer.
pub struct SftpClient {
    session: SftpSession,
    runtime: &'static Runtime,
}

impl SftpClient {
    /// Opens a dedicated SFTP subsystem over an authenticated client.
    pub fn open(client: &SshClient) -> Result<Self> {
        let runtime = runtime()?;
        let session = runtime.block_on(async {
            let channel = client
                .handle()
                .channel_open_session()
                .await
                .map_err(TerminalError::channel)?;
            let subsystem = channel.request_subsystem(true, "sftp");
            timeout(SUBSYSTEM_TIMEOUT, subsystem)
                .await
                .map_err(|_| TerminalError::Timeout)?
                .map_err(TerminalError::channel)?;
            SftpSession::new(channel.into_stream())
                .await
                .map_err(TerminalError::sftp)
        })?;
        Ok(Self { session, runtime })
    }

    fn run<T>(
        &self,
        cancel: &CancelToken,
        operation: impl std::future::Future<
            Output = std::result::Result<T, russh_sftp::client::error::Error>,
        >,
    ) -> Result<T> {
        self.runtime.block_on(async {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(TerminalError::Cancelled),
                result = operation => result.map_err(TerminalError::sftp),
            }
        })
    }

    /// Lists a directory, refusing names that could escape it.
    ///
    /// The SFTP library reads the whole directory before returning, so the
    /// [MAX_DIRECTORY_ENTRIES] cap bounds the result, not the wire traffic.
    pub fn list(&self, directory: &str, cancel: &CancelToken) -> Result<Vec<RemoteFile>> {
        validate_directory(directory)?;
        let entries = self.run(cancel, self.session.read_dir(directory))?;
        let mut files = Vec::new();
        for entry in entries {
            if files.len() >= MAX_DIRECTORY_ENTRIES {
                return Err(TerminalError::Validation(
                    "目录超过 10,000 项，请输入更具体的路径".into(),
                ));
            }
            let name = entry.file_name();
            if !valid_entry_name(&name) {
                return Err(TerminalError::Validation("SFTP 文件名称无效".into()));
            }
            let metadata = entry.metadata();
            files.push(remote_file(name, &metadata));
        }
        Ok(files)
    }

    /// Reads the metadata of one remote path without following symlinks.
    pub fn stat(&self, path: &str, cancel: &CancelToken) -> Result<RemoteFile> {
        validate_directory(path)?;
        let metadata = self.run(cancel, self.session.symlink_metadata(path))?;
        Ok(remote_file(base_name(path).to_owned(), &metadata))
    }

    /// Streams a remote file into a new local file, removing it on failure.
    pub fn download(
        &self,
        source: &str,
        destination: &Path,
        options: &TransferOptions,
    ) -> Result<u64> {
        validate_remote_path(source)?;
        options.cancel.check()?;
        self.runtime.block_on(async {
            let mut remote = self
                .session
                .open(source)
                .await
                .map_err(TerminalError::sftp)?;
            let size = remote.metadata().await.map_err(TerminalError::sftp)?.len();
            if size > options.max_bytes {
                return Err(TerminalError::TransferTooLarge {
                    limit: options.max_bytes,
                });
            }
            let mut local = create_local_file(destination).await?;
            let copied = copy_limited(&mut remote, &mut local, options).await;
            match copied {
                Ok(total) => {
                    local.sync_all().await?;
                    Ok(total)
                }
                Err(error) => {
                    drop(local);
                    let _ = tokio::fs::remove_file(destination).await;
                    Err(error)
                }
            }
        })
    }

    /// Streams a local file to a new remote name without overwriting.
    pub fn upload(
        &self,
        source: &Path,
        destination: &str,
        options: &TransferOptions,
    ) -> Result<u64> {
        validate_remote_path(destination)?;
        options.cancel.check()?;
        let local = std::fs::File::open(source)?;
        let metadata = local.metadata()?;
        if !metadata.is_file() {
            return Err(TerminalError::Validation("只支持上传普通文件".into()));
        }
        if metadata.len() > options.max_bytes {
            return Err(TerminalError::TransferTooLarge {
                limit: options.max_bytes,
            });
        }
        let staging = remote_join(
            &remote_parent(destination),
            &format!(
                "{UPLOAD_STAGING_PREFIX}{}-{}",
                process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or_default()
            ),
        );
        self.runtime.block_on(async {
            if self
                .session
                .try_exists(destination)
                .await
                .map_err(TerminalError::sftp)?
            {
                return Err(TerminalError::RemoteExists);
            }
            let mut remote = self
                .session
                .open_with_flags(
                    &staging,
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                )
                .await
                .map_err(TerminalError::sftp)?;
            let mut local = tokio::fs::File::from_std(local);
            let copied = copy_limited(&mut local, &mut remote, options).await;
            let closed = remote.close().await.map_err(TerminalError::Io);
            let outcome = copied.and_then(|total| closed.map(|()| total));
            match outcome {
                Ok(total) => {
                    self.session
                        .rename(&staging, destination)
                        .await
                        .map_err(TerminalError::sftp)?;
                    Ok(total)
                }
                Err(error) => {
                    let _ = self.session.remove_file(&staging).await;
                    Err(error)
                }
            }
        })
    }

    /// Resolves a relative path and symlinks to an absolute POSIX path.
    pub fn absolute_path(&self, directory: &str, cancel: &CancelToken) -> Result<String> {
        validate_directory(directory)?;
        let absolute = self.run(cancel, self.session.canonicalize(directory))?;
        if !absolute.starts_with('/')
            || absolute.len() > MAX_REMOTE_PATH_BYTES
            || absolute.contains('\0')
        {
            return Err(TerminalError::Validation("SFTP 返回的绝对路径无效".into()));
        }
        Ok(crate::files::normalize_remote_path(&absolute))
    }

    /// Renames without silently replacing an existing target.
    pub fn rename_file(&self, source: &str, destination: &str, cancel: &CancelToken) -> Result<()> {
        validate_remote_path(source)?;
        validate_remote_path(destination)?;
        if self.run(cancel, self.session.try_exists(destination))? {
            return Err(TerminalError::Validation("目标名称已存在".into()));
        }
        self.run(cancel, self.session.rename(source, destination))
    }

    /// Applies permission bits (000 to 777).
    pub fn set_permissions(&self, path: &str, mode: u32, cancel: &CancelToken) -> Result<()> {
        validate_remote_path(path)?;
        validate_mode(mode)?;
        let mut attributes = FileAttributes::empty();
        attributes.permissions = Some(mode);
        self.run(cancel, self.session.set_metadata(path, attributes))
    }

    /// Removes a file, or an empty directory, without following symlinks.
    pub fn remove(&self, path: &str, cancel: &CancelToken) -> Result<()> {
        validate_remote_path(path)?;
        let metadata = self.run(cancel, self.session.symlink_metadata(path))?;
        if metadata.is_dir() {
            self.run(cancel, self.session.remove_dir(path))
        } else {
            self.run(cancel, self.session.remove_file(path))
        }
    }

    /// Closes the subsystem channel.
    pub fn close(self) -> Result<()> {
        self.runtime
            .block_on(async { self.session.close().await.map_err(TerminalError::sftp) })
    }
}

impl SshClient {
    /// Opens an SFTP subsystem for this connection.
    pub fn open_sftp(&self) -> Result<SftpClient> {
        SftpClient::open(self)
    }
}

fn validate_mode(mode: u32) -> Result<()> {
    if mode & !0o777 != 0 {
        return Err(TerminalError::Validation("权限必须为 000 至 777".into()));
    }
    Ok(())
}

fn validate_directory(directory: &str) -> Result<()> {
    if directory.is_empty() || directory.len() > MAX_REMOTE_PATH_BYTES || directory.contains('\0') {
        return Err(TerminalError::Validation("远程目录路径无效".into()));
    }
    Ok(())
}

fn remote_file(name: String, metadata: &FileAttributes) -> RemoteFile {
    RemoteFile {
        name,
        mode: format_mode(metadata.permissions.unwrap_or_default()),
        size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
        modified: metadata.mtime.map(i64::from),
        directory: metadata.is_dir(),
    }
}

async fn create_local_file(destination: &Path) -> Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    match options.open(destination).await {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(TerminalError::LocalExists)
        }
        Err(error) => Err(TerminalError::Io(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_paths_are_validated() {
        for directory in ["", "bad\0path"] {
            assert!(validate_directory(directory).is_err());
        }
        assert!(validate_directory("/").is_ok());
        assert!(validate_directory(&"a".repeat(MAX_REMOTE_PATH_BYTES + 1)).is_err());
    }

    #[test]
    fn metadata_maps_to_displayable_entries() {
        let mut attributes = FileAttributes::empty();
        attributes.size = Some(42);
        attributes.permissions = Some(0o100640);
        attributes.mtime = Some(20);
        let file = remote_file("file".into(), &attributes);
        assert_eq!(file.mode, "-rw-r-----");
        assert_eq!(file.size, 42);
        assert_eq!(file.modified, Some(20));
        assert!(!file.directory);
    }

    #[test]
    fn staging_names_stay_inside_the_destination_directory() {
        let staging = remote_join(&remote_parent("/tmp/file"), ".superlink-upload-1");
        assert_eq!(staging, "/tmp/.superlink-upload-1");
        assert!(valid_entry_name(".superlink-upload-1"));
    }

    #[test]
    fn permissions_outside_the_supported_range_are_refused() {
        let error = validate_mode(0o1000).unwrap_err();
        assert_eq!(error.to_string(), "权限必须为 000 至 777");
        assert!(validate_mode(0o640).is_ok());
    }
}
