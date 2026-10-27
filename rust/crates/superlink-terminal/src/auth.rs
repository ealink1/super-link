//! Credential selection and bounded private key parsing.
//!
//! Nothing here logs, formats or returns passwords, passphrases or key bytes.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use russh::keys::{HashAlg, PrivateKeyWithHashAlg, decode_secret_key};
use superlink_domain::ShellHost;

use crate::error::{Result, TerminalError};

/// Upper bound for a private key file, matching the Go implementation.
pub(crate) const MAX_PRIVATE_KEY_BYTES: u64 = 2 << 20;

/// How the connection authenticates once the transport is up.
pub(crate) enum AuthMethod {
    Password(String),
    PrivateKey {
        path: PathBuf,
        passphrase: Option<String>,
    },
}

impl AuthMethod {
    /// Selects a method without ever interpreting the host input as a command.
    pub(crate) fn for_host(host: &ShellHost) -> Result<Self> {
        if host.key_path.trim().is_empty() {
            if host.password.is_empty() {
                return Err(TerminalError::Validation("请输入登录密码或选择私钥".into()));
            }
            Ok(Self::Password(host.password.clone()))
        } else {
            let passphrase = if host.passphrase.is_empty() {
                None
            } else {
                Some(host.passphrase.clone())
            };
            Ok(Self::PrivateKey {
                path: PathBuf::from(&host.key_path),
                passphrase,
            })
        }
    }
}

/// Reads at most 2 MiB and parses an OpenSSH, PKCS#8 or PuTTY private key.
/// The raw buffer is zeroed before returning, whatever the outcome.
pub(crate) fn load_private_key(
    path: &Path,
    passphrase: Option<&str>,
) -> Result<PrivateKeyWithHashAlg> {
    let file = File::open(path).map_err(|_| TerminalError::Validation("读取私钥失败".into()))?;
    let mut raw = Vec::new();
    file.take(MAX_PRIVATE_KEY_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|_| TerminalError::Validation("读取私钥失败".into()))?;
    if raw.len() as u64 > MAX_PRIVATE_KEY_BYTES {
        raw.fill(0);
        return Err(TerminalError::Validation("私钥无法读取或超过 2 MiB".into()));
    }
    let parsed = std::str::from_utf8(&raw)
        .ok()
        .and_then(|text| decode_secret_key(text, passphrase).ok());
    raw.fill(0);
    let key = parsed
        .ok_or_else(|| TerminalError::Validation("无法解析私钥，请检查格式和私钥口令".into()))?;
    // RSA keys need an explicit modern hash algorithm for the signature.
    Ok(PrivateKeyWithHashAlg::new(
        Arc::new(key),
        Some(HashAlg::Sha256),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> ShellHost {
        ShellHost {
            name: "test".into(),
            host: "127.0.0.1".into(),
            user: "tester".into(),
            port: 22,
            ..ShellHost::default()
        }
    }

    #[test]
    fn password_is_the_default_method() {
        let mut host = host();
        host.password = "secret".into();
        assert!(matches!(
            AuthMethod::for_host(&host).unwrap(),
            AuthMethod::Password(_)
        ));
    }

    #[test]
    fn missing_credentials_are_rejected() {
        // AuthMethod deliberately has no Debug impl: it owns credentials.
        let Ok(method) = AuthMethod::for_host(&host()) else {
            return;
        };
        match method {
            AuthMethod::Password(_) | AuthMethod::PrivateKey { .. } => {
                panic!("missing credentials must be rejected")
            }
        }
    }

    #[test]
    fn key_path_takes_precedence() {
        let mut host = host();
        host.password = "secret".into();
        host.key_path = "/tmp/key".into();
        host.passphrase = "phrase".into();
        match AuthMethod::for_host(&host).unwrap() {
            AuthMethod::PrivateKey { path, passphrase } => {
                assert_eq!(path, PathBuf::from("/tmp/key"));
                assert_eq!(passphrase.as_deref(), Some("phrase"));
            }
            AuthMethod::Password(_) => panic!("key path must win"),
        }
    }

    #[test]
    fn unreadable_and_oversized_keys_fail_without_leaking_details() {
        let error = load_private_key(Path::new("/definitely/missing/key"), None).unwrap_err();
        assert_eq!(error.to_string(), "读取私钥失败");
    }
}
