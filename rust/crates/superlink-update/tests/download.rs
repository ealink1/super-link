mod common;

use std::fs;

use common::*;
use superlink_update::{UpdateError, receive_artifact};

#[test]
fn receive_artifact_writes_verified_bytes() {
    let temp = TempDir::new();
    let payload = b"signed application package".to_vec();
    let artifact = app_artifact("0.2.0", "SuperLink.zip", &payload, platform());

    let verified = receive_artifact(&artifact, payload.as_slice(), temp.path()).expect("receive");

    assert_eq!(verified.size, payload.len() as u64);
    assert_eq!(verified.sha256, sha256_hex(&payload));
    assert_eq!(verified.path, temp.join("SuperLink.zip"));
    assert_eq!(fs::read(temp.join("SuperLink.zip")).expect("file"), payload);
    assert_no_temporary_files(temp.path());
}

#[test]
fn receive_artifact_rejects_a_hash_mismatch() {
    let temp = TempDir::new();
    let payload = b"signed application package".to_vec();
    let artifact = app_artifact("0.2.0", "SuperLink.zip", &payload, platform());
    let corrupted = b"signed application packagX".to_vec();

    let error =
        receive_artifact(&artifact, corrupted.as_slice(), temp.path()).expect_err("hash mismatch");

    assert!(matches!(error, UpdateError::HashMismatch { .. }));
    assert!(!temp.join("SuperLink.zip").exists());
    assert_no_temporary_files(temp.path());
}

#[test]
fn receive_artifact_rejects_a_truncated_stream() {
    let temp = TempDir::new();
    let payload = b"signed application package".to_vec();
    let artifact = app_artifact("0.2.0", "SuperLink.zip", &payload, platform());

    let error = receive_artifact(&artifact, &payload[..4], temp.path()).expect_err("truncated");

    assert!(matches!(error, UpdateError::SizeMismatch { .. }));
    assert!(!temp.join("SuperLink.zip").exists());
}

#[test]
fn receive_artifact_rejects_an_oversized_stream() {
    let temp = TempDir::new();
    let payload = b"signed application package".to_vec();
    let artifact = app_artifact("0.2.0", "SuperLink.zip", &payload, platform());
    let mut oversized = payload.clone();
    oversized.extend_from_slice(b"extra");

    let error =
        receive_artifact(&artifact, oversized.as_slice(), temp.path()).expect_err("oversized");

    assert!(matches!(error, UpdateError::SizeMismatch { .. }));
    assert!(!temp.join("SuperLink.zip").exists());
}

#[test]
fn receive_artifact_rejects_an_untrusted_url() {
    let temp = TempDir::new();
    let payload = b"payload";
    let mut artifact = app_artifact("0.2.0", "SuperLink.zip", payload, platform());
    artifact.url = "https://evil.example.com/SuperLink.zip".into();

    let error =
        receive_artifact(&artifact, payload.as_slice(), temp.path()).expect_err("untrusted url");

    assert!(matches!(error, UpdateError::UntrustedUrl(_)));
}

fn assert_no_temporary_files(directory: &std::path::Path) {
    let leftovers: Vec<_> = fs::read_dir(directory)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".download-")
        })
        .collect();
    assert!(leftovers.is_empty(), "temporary files left behind");
}
