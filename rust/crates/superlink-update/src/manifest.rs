//! Signed release manifest and artifact metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Result, UpdateError};
use crate::platform::{Arch, Os};
use crate::trusted_url::trusted_url;
use crate::version::Version;

/// Schema version understood by this crate.
pub const SCHEMA_VERSION: i64 = 1;
/// Largest manifest accepted from the network.
pub const MAX_MANIFEST_BYTES: usize = 1 << 20;
/// Largest artifact declared by a manifest (1 GiB).
pub const MAX_ARTIFACT_BYTES: u64 = 1 << 30;
/// Largest artifact count in one release.
pub const MAX_ARTIFACTS: usize = 200;
const MAX_FILENAME_BYTES: usize = 161;
/// Artifact identifier used by the desktop application package.
pub const APP_ARTIFACT_ID: &str = "superlink";
/// Protocol version required from driver artifacts.
pub const DRIVER_PROTOCOL: &str = "json-lines-v2";

/// What an artifact contains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    App,
    Driver,
}

impl ArtifactKind {
    /// Returns the wire name, e.g. `app`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Driver => "driver",
        }
    }
}

/// Release channel of a manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Preview,
}

impl Channel {
    /// Returns the wire name, e.g. `stable`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Preview => "preview",
        }
    }
}

/// One downloadable file listed in a signed manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: String,
    pub kind: ArtifactKind,
    pub os: Os,
    pub arch: Arch,
    pub filename: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub protocol: String,
}

impl Artifact {
    /// Validates naming, size, digest and download address.
    pub fn validate(&self) -> Result<()> {
        if !is_safe_filename(&self.id) || !is_safe_filename(&self.filename) {
            return Err(UpdateError::Manifest(
                "invalid artifact identifier or filename".into(),
            ));
        }
        if self.size == 0 || self.size > MAX_ARTIFACT_BYTES {
            return Err(UpdateError::Manifest("invalid artifact size".into()));
        }
        if !is_sha256(&self.sha256) {
            return Err(UpdateError::Manifest("invalid artifact SHA256".into()));
        }
        if self.kind == ArtifactKind::Driver
            && (self.revision.is_empty() || self.protocol != DRIVER_PROTOCOL)
        {
            return Err(UpdateError::Manifest(
                "driver compatibility metadata missing".into(),
            ));
        }
        trusted_url(&self.url)?;
        Ok(())
    }

    /// Identity of an artifact across releases; used for duplicate detection.
    pub fn key(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.kind.as_str(),
            self.id,
            self.os.as_str(),
            self.arch.as_str()
        )
    }
}

/// A verified release description.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: i64,
    pub version: String,
    pub channel: Channel,
    #[serde(
        rename = "publishedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub published_at: Option<DateTime<Utc>>,
    pub artifacts: Vec<Artifact>,
}

impl Manifest {
    /// Strictly decodes a manifest.
    ///
    /// Unknown fields and trailing bytes are rejected so a signed payload can
    /// never gain meaning after the fact.
    pub fn parse(raw: &[u8]) -> Result<Self> {
        if raw.len() > MAX_MANIFEST_BYTES {
            return Err(UpdateError::Manifest("manifest exceeds size limit".into()));
        }
        let manifest: Self = serde_json::from_slice(raw)
            .map_err(|error| UpdateError::Manifest(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Encodes the manifest exactly as the signing tool does.
    pub fn to_signed_bytes(&self) -> Result<Vec<u8>> {
        let mut raw = serde_json::to_vec_pretty(self)
            .map_err(|error| UpdateError::Manifest(error.to_string()))?;
        raw.push(b'\n');
        Ok(raw)
    }

    /// Validates every field that the updater relies on.
    pub fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA_VERSION {
            return Err(UpdateError::Manifest(
                "unsupported release manifest schema".into(),
            ));
        }
        if Version::parse(&self.version).is_none() {
            return Err(UpdateError::Manifest("invalid release version".into()));
        }
        if self.artifacts.is_empty() || self.artifacts.len() > MAX_ARTIFACTS {
            return Err(UpdateError::Manifest("invalid artifact count".into()));
        }
        let mut seen = std::collections::HashSet::with_capacity(self.artifacts.len());
        for artifact in &self.artifacts {
            artifact.validate()?;
            if !seen.insert(artifact.key()) {
                return Err(UpdateError::Manifest(format!(
                    "duplicate artifact {}",
                    artifact.key()
                )));
            }
        }
        Ok(())
    }

    /// Finds the artifact matching a kind, identifier and platform.
    pub fn artifact(&self, kind: ArtifactKind, id: &str, os: Os, arch: Arch) -> Result<&Artifact> {
        self.artifacts
            .iter()
            .find(|artifact| {
                artifact.kind == kind
                    && artifact.id == id
                    && artifact.os == os
                    && artifact.arch == arch
            })
            .ok_or(UpdateError::NoPlatformArtifact {
                kind: kind.as_str(),
                os: os.as_str(),
                arch: arch.as_str(),
            })
    }

    /// Reports whether this release is newer than `current`.
    pub fn is_newer_than(&self, current: &str) -> bool {
        crate::version::is_newer(&self.version, current)
    }
}

/// Accepts `[A-Za-z0-9][A-Za-z0-9._-]{0,160}`.
pub(crate) fn is_safe_filename(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_FILENAME_BYTES {
        return false;
    }
    if !bytes[0].is_ascii_alphanumeric() {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Accepts 64 hexadecimal characters.
pub(crate) fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}
