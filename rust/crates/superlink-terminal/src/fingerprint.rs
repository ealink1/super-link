//! OpenSSH SHA-256 host key fingerprints.
//!
//! Pins are stored exactly as OpenSSH prints them ("SHA256:..."), so a value
//! copied from ssh-keygen -lf verifies without rewriting.

use russh::keys::ssh_key::Fingerprint;
use russh::keys::{HashAlg, PublicKey};

use crate::error::{Result, TerminalError};

/// Formats a parsed public key the way OpenSSH prints SHA-256 fingerprints.
pub fn fingerprint_sha256(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Parses an OpenSSH public key line such as "ssh-ed25519 AAAA... comment".
pub fn fingerprint_from_openssh(text: &str) -> Result<String> {
    let key = PublicKey::from_openssh(text.trim())
        .map_err(|_| TerminalError::Validation("SSH 公钥格式无效".into()))?;
    Ok(fingerprint_sha256(&key))
}

/// Parses a raw SSH wire-format public key blob.
pub fn fingerprint_from_bytes(bytes: &[u8]) -> Result<String> {
    let key = PublicKey::from_bytes(bytes)
        .map_err(|_| TerminalError::Validation("SSH 公钥格式无效".into()))?;
    Ok(fingerprint_sha256(&key))
}

/// Compares two fingerprints by decoded digest rather than raw text, so a
/// trailing newline or spacing difference in a stored pin cannot cause a
/// spurious mismatch. Unparsable pins never match.
pub fn fingerprints_match(expected: &str, observed: &str) -> bool {
    let expected = expected.trim();
    let observed = observed.trim();
    if expected.is_empty() || observed.is_empty() {
        return false;
    }
    match (
        expected.parse::<Fingerprint>(),
        observed.parse::<Fingerprint>(),
    ) {
        (Ok(expected), Ok(observed)) => expected == observed,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated with ssh-keygen -t ed25519 and verified with ssh-keygen -lf.
    const PUBLIC_KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICalGvOoDX5YnO1eA4kQc5b/DPw0TEa6O82nqRFXra5e superlink-test";
    const PIN: &str = "SHA256:eLQy6UcSXjIOFdKRqsuK49h6xrNJsFR4cM8QE9TtAxA";

    #[test]
    fn formats_openssh_sha256_fingerprints() {
        assert_eq!(fingerprint_from_openssh(PUBLIC_KEY).unwrap(), PIN);
        assert_eq!(
            fingerprint_from_openssh("  ").unwrap_err().to_string(),
            "SSH 公钥格式无效"
        );
    }

    #[test]
    fn matches_decoded_fingerprints_and_rejects_others() {
        assert!(fingerprints_match(PIN, PIN));
        assert!(fingerprints_match(&format!(" {PIN}\n"), PIN));
        assert!(!fingerprints_match(PIN, "SHA256:changed-pin"));
        assert!(!fingerprints_match("", PIN));
        assert!(!fingerprints_match("not-a-pin", "not-a-pin"));
    }

    #[test]
    fn rejects_truncated_wire_keys() {
        let key = PublicKey::from_openssh(PUBLIC_KEY).unwrap();
        let encoded = key.to_bytes().unwrap();
        assert!(fingerprint_from_bytes(&encoded).is_ok());
        assert!(fingerprint_from_bytes(&encoded[..encoded.len() - 4]).is_err());
    }
}
