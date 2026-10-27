//! Receiving a downloaded artifact with exact size and digest checks.
//!
//! Transport is left to the caller: this module only accepts a byte stream so
//! tests never need the network. Bytes land in a private temporary file that is
//! renamed into place only after it matches the signed manifest.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::error::{Result, UpdateError};
use crate::manifest::Artifact;
use crate::verify::VerifiedArtifact;
use sha2::{Digest, Sha256};

const COPY_BUFFER_BYTES: usize = 128 * 1024;

/// Streams an artifact into `directory` and verifies it against the manifest.
///
/// The file is written under a dot-prefixed temporary name and only renamed to
/// the published filename once the byte count and SHA256 both match.
pub fn receive_artifact(
    artifact: &Artifact,
    reader: impl Read,
    directory: &Path,
) -> Result<VerifiedArtifact> {
    artifact.validate()?;
    fs::create_dir_all(directory)
        .map_err(UpdateError::io(format!("create {}", directory.display())))?;
    let temporary = directory.join(format!(".download-{}", Uuid::new_v4().simple()));
    let (size, sha256) = match write_verified(artifact, reader, &temporary) {
        Ok(verified) => verified,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    };
    let path = directory.join(&artifact.filename);
    fs::rename(&temporary, &path).map_err(UpdateError::io(format!(
        "move {} into place",
        path.display()
    )))?;
    Ok(VerifiedArtifact {
        artifact: artifact.clone(),
        path,
        size,
        sha256,
    })
}

fn write_verified(
    artifact: &Artifact,
    mut reader: impl Read,
    temporary: &Path,
) -> Result<(u64, String)> {
    let mut file = File::create(temporary)
        .map_err(UpdateError::io(format!("create {}", temporary.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    let mut size = 0u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(UpdateError::io("read artifact stream"))?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > artifact.size {
            return Err(UpdateError::SizeMismatch {
                path: PathBuf::from(&artifact.filename),
                expected: artifact.size,
                actual: size,
            });
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(UpdateError::io(format!("write {}", temporary.display())))?;
    }
    file.sync_all()
        .map_err(UpdateError::io(format!("sync {}", temporary.display())))?;
    if size != artifact.size {
        return Err(UpdateError::SizeMismatch {
            path: PathBuf::from(&artifact.filename),
            expected: artifact.size,
            actual: size,
        });
    }
    let sha256 = hex::encode(hasher.finalize());
    if !sha256.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(UpdateError::HashMismatch {
            path: PathBuf::from(&artifact.filename),
            expected: artifact.sha256.to_ascii_lowercase(),
            actual: sha256,
        });
    }
    Ok((size, sha256))
}
