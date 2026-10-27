//! Signature, digest and platform verification.
//!
//! The order is fixed and load bearing: a signature over the raw bytes, then
//! manifest decoding and validation, then the SHA256 of every artifact file,
//! then the platform lookup. Nothing is written to disk before all four pass.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::error::{Result, UpdateError};
use crate::manifest::{APP_ARTIFACT_ID, Artifact, ArtifactKind, MAX_MANIFEST_BYTES, Manifest};
use crate::platform::Platform;

const PUBLIC_KEY_BYTES: usize = 32;
const SIGNATURE_BYTES: usize = 64;
const MAX_SIGNATURE_TEXT: usize = 1024;
const HASH_BUFFER_BYTES: usize = 128 * 1024;

/// A manifest whose signature and artifact files have been checked.
#[derive(Clone, Debug)]
pub struct Verification {
    /// The verified manifest.
    pub manifest: Manifest,
    /// Base64 public key the signature was checked against.
    pub public_key: String,
    /// Platform requested by the caller.
    pub platform: Platform,
    /// Artifact files that were present and hashed.
    pub artifacts: Vec<VerifiedArtifact>,
}

/// One artifact file whose size and digest matched the manifest.
#[derive(Clone, Debug)]
pub struct VerifiedArtifact {
    /// Manifest entry that was checked.
    pub artifact: Artifact,
    /// File that was hashed.
    pub path: PathBuf,
    /// Byte length read from disk.
    pub size: u64,
    /// Lowercase hexadecimal digest read from disk.
    pub sha256: String,
}

impl Verification {
    /// Returns the application package verified for this platform.
    ///
    /// Missing or unverified packages are an error; callers must not fall back
    /// to staging an unverified archive.
    pub fn app(&self) -> Result<&VerifiedArtifact> {
        let expected = self.manifest.artifact(
            ArtifactKind::App,
            APP_ARTIFACT_ID,
            self.platform.os,
            self.platform.arch,
        )?;
        self.artifacts
            .iter()
            .find(|verified| verified.artifact == *expected)
            .ok_or_else(|| UpdateError::ArtifactNotVerified {
                filename: expected.filename.clone(),
            })
    }
}

/// Verifies the signature over the manifest and validates its contents.
pub fn verify_manifest(raw: &[u8], signature: &[u8], public_key: &str) -> Result<Manifest> {
    if raw.len() > MAX_MANIFEST_BYTES {
        return Err(UpdateError::Manifest("manifest exceeds size limit".into()));
    }
    let key = decode_public_key(public_key)?;
    let signature = decode_signature(signature)?;
    key.verify_strict(raw, &signature)
        .map_err(|_| UpdateError::SignatureInvalid)?;
    Manifest::parse(raw)
}

/// Runs the whole verification pipeline for a directory of downloaded files.
///
/// Artifacts whose files are absent are skipped: a release lists every
/// platform, while the updater only downloads one. The application package for
/// `platform` must be present and must verify.
pub fn verify_release(
    raw: &[u8],
    signature: &[u8],
    public_key: &str,
    directory: &Path,
    platform: Platform,
) -> Result<Verification> {
    let manifest = verify_manifest(raw, signature, public_key)?;
    let artifacts = verify_artifacts(&manifest, directory)?;
    let verification = Verification {
        manifest,
        public_key: public_key.trim().to_owned(),
        platform,
        artifacts,
    };
    verification.app()?;
    Ok(verification)
}

/// Checks the size and SHA256 of every manifest artifact present in `directory`.
pub fn verify_artifacts(manifest: &Manifest, directory: &Path) -> Result<Vec<VerifiedArtifact>> {
    let mut verified = Vec::new();
    for artifact in &manifest.artifacts {
        let path = directory.join(&artifact.filename);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(UpdateError::Archive(format!(
                        "artifact {} must be a regular file",
                        artifact.filename
                    )));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(UpdateError::io(format!("inspect {}", path.display()))(
                    error,
                ));
            }
        }
        let (size, sha256) = sha256_file(&path)?;
        if size != artifact.size {
            return Err(UpdateError::SizeMismatch {
                path,
                expected: artifact.size,
                actual: size,
            });
        }
        if !sha256.eq_ignore_ascii_case(&artifact.sha256) {
            return Err(UpdateError::HashMismatch {
                path,
                expected: artifact.sha256.to_ascii_lowercase(),
                actual: sha256,
            });
        }
        verified.push(VerifiedArtifact {
            artifact: artifact.clone(),
            path,
            size,
            sha256,
        });
    }
    Ok(verified)
}

/// Streams a file and returns its byte length and lowercase SHA256 digest.
pub fn sha256_file(path: &Path) -> Result<(u64, String)> {
    let file = File::open(path).map_err(UpdateError::io(format!("open {}", path.display())))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BUFFER_BYTES];
    let mut size = 0u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(UpdateError::io(format!("read {}", path.display())))?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((size, hex::encode(hasher.finalize())))
}

fn decode_public_key(text: &str) -> Result<VerifyingKey> {
    let decoded = STANDARD
        .decode(text.trim())
        .map_err(|_| UpdateError::MissingPublicKey)?;
    let bytes: [u8; PUBLIC_KEY_BYTES] = decoded
        .as_slice()
        .try_into()
        .map_err(|_| UpdateError::MissingPublicKey)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| UpdateError::MissingPublicKey)
}

fn decode_signature(signature: &[u8]) -> Result<Signature> {
    if signature.len() > MAX_SIGNATURE_TEXT {
        return Err(UpdateError::SignatureInvalid);
    }
    let text = std::str::from_utf8(signature).map_err(|_| UpdateError::SignatureInvalid)?;
    let decoded = STANDARD
        .decode(text.trim())
        .map_err(|_| UpdateError::SignatureInvalid)?;
    let bytes: [u8; SIGNATURE_BYTES] = decoded
        .as_slice()
        .try_into()
        .map_err(|_| UpdateError::SignatureInvalid)?;
    Ok(Signature::from_bytes(&bytes))
}
