#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

use superlink_update::{Arch, Artifact, ArtifactKind, Channel, Manifest, Os, Platform};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Self-cleaning directory under the system temp directory.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new() -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("superlink-update-{}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Platform the tests run on.
pub fn platform() -> Platform {
    Platform::current().expect("tests need a supported release platform")
}

/// A platform that is never the one the tests run on.
pub fn other_platform() -> Platform {
    match platform().os {
        Os::Windows => Platform::new(Os::Darwin, Arch::Arm64),
        _ => Platform::new(Os::Windows, Arch::Amd64),
    }
}

/// Deterministic signing key; no randomness is needed to test verification.
pub fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

pub fn public_key() -> String {
    STANDARD.encode(signing_key().verifying_key().to_bytes())
}

pub fn sign(raw: &[u8]) -> Vec<u8> {
    STANDARD
        .encode(signing_key().sign(raw).to_bytes())
        .into_bytes()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub fn manifest(version: &str, artifacts: Vec<Artifact>) -> Manifest {
    Manifest {
        schema: 1,
        version: version.to_owned(),
        channel: Channel::Stable,
        published_at: Some(Utc::now()),
        artifacts,
    }
}

pub fn app_artifact(version: &str, filename: &str, data: &[u8], platform: Platform) -> Artifact {
    Artifact {
        id: "superlink".to_owned(),
        kind: ArtifactKind::App,
        os: platform.os,
        arch: platform.arch,
        filename: filename.to_owned(),
        url: format!(
            "https://github.com/ealink1/super-link/releases/download/v{version}/{filename}"
        ),
        size: data.len() as u64,
        sha256: sha256_hex(data),
        revision: String::new(),
        protocol: String::new(),
    }
}

pub fn driver_artifact(version: &str, filename: &str, data: &[u8], platform: Platform) -> Artifact {
    Artifact {
        id: "sqlite".to_owned(),
        kind: ArtifactKind::Driver,
        os: platform.os,
        arch: platform.arch,
        filename: filename.to_owned(),
        url: format!(
            "https://github.com/ealink1/SuperLink-DriverAgents/releases/download/v{version}/{filename}"
        ),
        size: data.len() as u64,
        sha256: sha256_hex(data),
        revision: "revision-1".to_owned(),
        protocol: "json-lines-v2".to_owned(),
    }
}

/// Bytes the signing tool would sign for this manifest.
pub fn manifest_bytes(manifest: &Manifest) -> Vec<u8> {
    manifest.to_signed_bytes().expect("encode manifest")
}

pub fn write_file(path: &Path, data: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, data).expect("write file");
}

pub fn package_marker(version: &str, platform: Platform, executable: &str, helper: &str) -> String {
    serde_json::json!({
        "id": "io.github.ealink1.superlink",
        "version": version,
        "os": platform.os.as_str(),
        "arch": platform.arch.as_str(),
        "executable": executable,
        "helper": helper,
    })
    .to_string()
}

/// Creates a portable package layout with the marker at its root.
pub fn create_package(root: &Path, version: &str, platform: Platform) {
    write_file(
        &root.join("bin/superlink"),
        format!("superlink {version}").as_bytes(),
    );
    write_file(&root.join("bin/update-helper"), b"helper");
    write_file(
        &root.join("superlink.package.json"),
        package_marker(version, platform, "bin/superlink", "bin/update-helper").as_bytes(),
    );
}

/// Creates a macOS bundle layout with the marker inside `Contents/Resources`.
pub fn create_bundle(root: &Path, version: &str, platform: Platform) {
    write_file(
        &root.join("Contents/MacOS/superlink"),
        format!("superlink {version}").as_bytes(),
    );
    write_file(&root.join("Contents/MacOS/update-helper"), b"helper");
    write_file(
        &root.join("Contents/Resources/superlink.package.json"),
        package_marker(
            version,
            platform,
            "Contents/MacOS/superlink",
            "Contents/MacOS/update-helper",
        )
        .as_bytes(),
    );
}
