use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};

const REF_PREFIX: &str = "local:v1:";
const FILE_HEADER: &[u8] = b"NFCR\x01";
const KEY_NAME: &str = "master.key";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const MAX_BUNDLE: usize = 8 << 20;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("密码不可用，请编辑连接，重新输入密码并勾选“记住密码”")]
    Missing,
    #[error(
        "本地密码文件或密钥损坏，请恢复完整的 credentials 备份；无备份时需重置本地密码存储再重新输入"
    )]
    Corrupt,
    #[error("无法访问本地密码存储，请检查工作区权限")]
    Unavailable,
    #[error("密码和敏感连接参数合计不能超过 8 MiB")]
    TooLarge,
    #[error("凭据引用无效")]
    InvalidReference,
}

pub type SecretResult<T> = Result<T, SecretError>;

/// AES-256-GCM bundle store compatible with the Go vault on disk:
/// a 5-byte header, a random 12-byte nonce, then the sealed payload.
/// The credential reference is authenticated as associated data.
#[derive(Debug, Clone)]
pub struct LocalVault {
    root: PathBuf,
}

impl LocalVault {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn put(&self, reference: &str, data: &[u8]) -> SecretResult<()> {
        if data.len() > MAX_BUNDLE {
            return Err(SecretError::TooLarge);
        }
        let name = file_name(reference)?;
        let dir = self.directory(true)?;
        let key = ensure_key(&dir)?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SecretError::Corrupt)?;
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce).map_err(|_| SecretError::Unavailable)?;
        let sealed = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: data,
                    aad: reference.as_bytes(),
                },
            )
            .map_err(|_| SecretError::Unavailable)?;
        let mut payload = Vec::with_capacity(FILE_HEADER.len() + NONCE_LEN + sealed.len());
        payload.extend_from_slice(FILE_HEADER);
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&sealed);
        write_private(&dir.join(name), &payload)
    }

    pub fn get(&self, reference: &str) -> SecretResult<Vec<u8>> {
        let name = file_name(reference)?;
        let dir = match self.directory(false) {
            Ok(dir) => dir,
            Err(SecretError::Missing) => return Err(SecretError::Missing),
            Err(err) => return Err(err),
        };
        let path = dir.join(name);
        let payload = match read_private(&path) {
            Ok(payload) => payload,
            Err(SecretError::Missing) => return Err(SecretError::Missing),
            Err(_) => return Err(SecretError::Corrupt),
        };
        if !payload.starts_with(FILE_HEADER) || payload.len() < FILE_HEADER.len() + NONCE_LEN {
            return Err(SecretError::Corrupt);
        }
        let nonce_start = FILE_HEADER.len();
        let body_start = nonce_start + NONCE_LEN;
        let key = ensure_key(&dir)?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SecretError::Corrupt)?;
        cipher
            .decrypt(
                Nonce::from_slice(&payload[nonce_start..body_start]),
                Payload {
                    msg: &payload[body_start..],
                    aad: reference.as_bytes(),
                },
            )
            .map_err(|_| SecretError::Corrupt)
    }

    pub fn delete(&self, reference: &str) -> SecretResult<()> {
        let name = file_name(reference)?;
        let dir = match self.directory(false) {
            Ok(dir) => dir,
            Err(SecretError::Missing) => return Ok(()),
            Err(err) => return Err(err),
        };
        match fs::remove_file(dir.join(name)) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SecretError::Unavailable),
        }
    }

    fn directory(&self, create: bool) -> SecretResult<PathBuf> {
        let dir = self.root.join("credentials");
        if create {
            fs::create_dir_all(&dir).map_err(|_| SecretError::Unavailable)?;
            harden_dir(&dir)?;
        }
        match fs::metadata(&dir) {
            Ok(meta) if meta.is_dir() => Ok(dir),
            Ok(_) => Err(SecretError::Unavailable),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(SecretError::Missing),
            Err(_) => Err(SecretError::Unavailable),
        }
    }
}

fn file_name(reference: &str) -> SecretResult<String> {
    let id = reference
        .strip_prefix(REF_PREFIX)
        .ok_or(SecretError::InvalidReference)?;
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| SecretError::InvalidReference)?;
    if parsed.to_string() != id {
        return Err(SecretError::InvalidReference);
    }
    Ok(format!("{id}.cred"))
}

/// Never replaces a lost key while ciphertext still needs recovery.
fn ensure_key(dir: &Path) -> SecretResult<Vec<u8>> {
    let path = dir.join(KEY_NAME);
    match read_private(&path) {
        Ok(key) if key.len() == KEY_LEN => return Ok(key),
        Ok(_) => return Err(SecretError::Corrupt),
        Err(SecretError::Missing) => {}
        Err(_) => return Err(SecretError::Corrupt),
    }
    if has_ciphertext(dir) {
        return Err(SecretError::Corrupt);
    }
    let mut key = [0u8; KEY_LEN];
    getrandom::getrandom(&mut key).map_err(|_| SecretError::Unavailable)?;
    write_private(&path, &key)?;
    Ok(key.to_vec())
}

fn has_ciphertext(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".cred"))
        })
        .unwrap_or(true)
}

fn write_private(path: &Path, data: &[u8]) -> SecretResult<()> {
    let parent = path.parent().ok_or(SecretError::Unavailable)?;
    let temp = parent.join(format!(".tmp-{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|_| SecretError::Unavailable)?;
    file.write_all(data).map_err(|_| SecretError::Unavailable)?;
    file.sync_all().map_err(|_| SecretError::Unavailable)?;
    fs::rename(&temp, path).map_err(|_| {
        let _ = fs::remove_file(&temp);
        SecretError::Unavailable
    })
}

fn read_private(path: &Path) -> SecretResult<Vec<u8>> {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Err(SecretError::Missing),
        Err(_) => return Err(SecretError::Unavailable),
    };
    if !meta.is_file() || meta.len() as usize > MAX_BUNDLE + 64 {
        return Err(SecretError::Corrupt);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(SecretError::Corrupt);
        }
    }
    fs::read(path).map_err(|_| SecretError::Unavailable)
}

#[cfg(unix)]
fn harden_dir(dir: &Path) -> SecretResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .map_err(|_| SecretError::Unavailable)
}

#[cfg(not(unix))]
fn harden_dir(_dir: &Path) -> SecretResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("superlink-vault-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn reference() -> String {
        format!("local:v1:{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn round_trips_a_bundle() {
        let dir = TestDir::new();
        let vault = LocalVault::new(&dir.0);
        let reference = reference();
        vault.put(&reference, b"secret-material").unwrap();
        assert_eq!(vault.get(&reference).unwrap(), b"secret-material");
        vault.delete(&reference).unwrap();
        assert!(matches!(vault.get(&reference), Err(SecretError::Missing)));
    }

    #[test]
    fn ciphertext_is_bound_to_its_reference() {
        let dir = TestDir::new();
        let vault = LocalVault::new(&dir.0);
        let stored = reference();
        vault.put(&stored, b"payload").unwrap();
        // Move the bundle under another reference: the AAD no longer matches.
        let other = reference();
        let credentials = dir.0.join("credentials");
        std::fs::rename(
            credentials.join(file_name(&stored).unwrap()),
            credentials.join(file_name(&other).unwrap()),
        )
        .unwrap();
        assert!(matches!(vault.get(&other), Err(SecretError::Corrupt)));
    }

    #[test]
    fn tampering_is_detected() {
        let dir = TestDir::new();
        let vault = LocalVault::new(&dir.0);
        let reference = reference();
        vault.put(&reference, b"payload").unwrap();
        let path = dir
            .0
            .join("credentials")
            .join(file_name(&reference).unwrap());
        let mut raw = fs::read(&path).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        fs::write(&path, raw).unwrap();
        assert!(matches!(vault.get(&reference), Err(SecretError::Corrupt)));
    }

    #[test]
    fn rejects_a_foreign_reference() {
        let vault = LocalVault::new(std::env::temp_dir());
        assert!(matches!(
            vault.get("keyring:abc"),
            Err(SecretError::InvalidReference)
        ));
    }

    #[test]
    fn never_creates_a_key_next_to_existing_ciphertext() {
        let dir = TestDir::new();
        let vault = LocalVault::new(&dir.0);
        let credentials = dir.0.join("credentials");
        fs::create_dir_all(&credentials).unwrap();
        fs::write(
            credentials.join(format!("{}.cred", uuid::Uuid::new_v4())),
            b"junk",
        )
        .unwrap();
        assert!(matches!(
            vault.put(&reference(), b"x"),
            Err(SecretError::Corrupt)
        ));
    }

    #[test]
    fn bundles_are_bounded() {
        let dir = TestDir::new();
        let vault = LocalVault::new(&dir.0);
        let reference = reference();
        assert!(matches!(
            vault.put(&reference, &vec![0u8; MAX_BUNDLE + 1]),
            Err(SecretError::TooLarge)
        ));
    }
}
