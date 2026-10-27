mod common;

use common::*;
use superlink_update::{
    ArtifactKind, Manifest, Os, Platform, UpdateError, verify_manifest, verify_release,
};

fn signed(manifest: &Manifest) -> (Vec<u8>, Vec<u8>, String) {
    let raw = manifest_bytes(manifest);
    let signature = sign(&raw);
    (raw, signature, public_key())
}

#[test]
fn verify_manifest_accepts_a_signed_payload() {
    let payload = b"application package".to_vec();
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact("0.2.0", "SuperLink.zip", &payload, platform())],
    );
    let (raw, signature, key) = signed(&manifest);

    let verified = verify_manifest(&raw, &signature, &key).expect("signature must verify");

    assert_eq!(verified.version, "0.2.0");
    assert_eq!(verified.artifacts.len(), 1);
    assert_eq!(verified.artifacts[0].sha256, sha256_hex(&payload));
}

#[test]
fn verify_manifest_rejects_a_tampered_payload() {
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact(
            "0.2.0",
            "SuperLink.zip",
            b"payload",
            platform(),
        )],
    );
    let (mut raw, signature, key) = signed(&manifest);
    let position = raw.len() / 2;
    raw[position] ^= 0x01;

    let error = verify_manifest(&raw, &signature, &key).expect_err("tampering must fail");

    assert!(matches!(error, UpdateError::SignatureInvalid));
}

#[test]
fn verify_manifest_rejects_a_tampered_signature() {
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact(
            "0.2.0",
            "SuperLink.zip",
            b"payload",
            platform(),
        )],
    );
    let (raw, mut signature, key) = signed(&manifest);
    signature[0] = if signature[0] == b'A' { b'B' } else { b'A' };

    let error = verify_manifest(&raw, &signature, &key).expect_err("tampering must fail");

    assert!(matches!(error, UpdateError::SignatureInvalid));
}

#[test]
fn verify_manifest_rejects_a_signature_from_another_key() {
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact(
            "0.2.0",
            "SuperLink.zip",
            b"payload",
            platform(),
        )],
    );
    let (raw, signature, _) = signed(&manifest);
    let other_key = base64_encode(&[9u8; 32]);

    let error = verify_manifest(&raw, &signature, &other_key).expect_err("wrong key must fail");

    assert!(matches!(error, UpdateError::SignatureInvalid));
}

#[test]
fn verify_manifest_rejects_a_malformed_public_key() {
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact(
            "0.2.0",
            "SuperLink.zip",
            b"payload",
            platform(),
        )],
    );
    let (raw, signature, _) = signed(&manifest);

    for key in ["", "not base64!!", &base64_encode(&[1u8; 31])] {
        let error = verify_manifest(&raw, &signature, key).expect_err("bad key must fail");
        assert!(matches!(error, UpdateError::MissingPublicKey), "{key:?}");
    }
}

#[test]
fn verify_manifest_rejects_trailing_data() {
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact(
            "0.2.0",
            "SuperLink.zip",
            b"payload",
            platform(),
        )],
    );
    let mut raw = manifest_bytes(&manifest);
    raw.extend_from_slice(b"\n{}");
    let signature = sign(&raw);

    let error = verify_manifest(&raw, &signature, &public_key()).expect_err("trailing data");

    assert!(matches!(error, UpdateError::Manifest(_)));
}

#[test]
fn verify_manifest_rejects_unknown_fields() {
    let mut value: serde_json::Value = serde_json::from_slice(&manifest_bytes(&manifest(
        "0.2.0",
        vec![app_artifact(
            "0.2.0",
            "SuperLink.zip",
            b"payload",
            platform(),
        )],
    )))
    .expect("manifest json");
    value["extra"] = serde_json::json!(true);
    let raw = serde_json::to_vec(&value).expect("encode");
    let signature = sign(&raw);

    let error = verify_manifest(&raw, &signature, &public_key()).expect_err("unknown field");

    assert!(matches!(error, UpdateError::Manifest(_)));
}

#[test]
fn manifest_validation_rejects_structural_problems() {
    let payload = b"payload";
    let base = app_artifact("0.2.0", "SuperLink.zip", payload, platform());

    let mut bad_schema = manifest("0.2.0", vec![base.clone()]);
    bad_schema.schema = 2;
    assert!(bad_schema.validate().is_err());

    let mut bad_version = manifest("not-a-version", vec![base.clone()]);
    assert!(bad_version.validate().is_err());
    bad_version.version = "0.2.0".into();
    assert!(bad_version.validate().is_ok());

    let no_artifacts = manifest("0.2.0", Vec::new());
    assert!(no_artifacts.validate().is_err());

    let duplicate = manifest("0.2.0", vec![base.clone(), base.clone()]);
    assert!(duplicate.validate().is_err());

    let mut driver_without_protocol = base.clone();
    driver_without_protocol.kind = ArtifactKind::Driver;
    driver_without_protocol.protocol = String::new();
    assert!(
        manifest("0.2.0", vec![driver_without_protocol])
            .validate()
            .is_err()
    );

    let mut zero_size = base.clone();
    zero_size.size = 0;
    assert!(manifest("0.2.0", vec![zero_size]).validate().is_err());

    let mut bad_hash = base.clone();
    bad_hash.sha256 = "zz".repeat(32);
    assert!(manifest("0.2.0", vec![bad_hash]).validate().is_err());

    let mut bad_filename = base.clone();
    bad_filename.filename = "../SuperLink.zip".into();
    assert!(manifest("0.2.0", vec![bad_filename]).validate().is_err());
}

#[test]
fn manifest_validation_rejects_untrusted_urls() {
    let payload = b"payload";
    let mut artifact = app_artifact("0.2.0", "SuperLink.zip", payload, platform());

    for url in [
        "http://github.com/ealink1/super-link/releases/download/v0.2.0/SuperLink.zip",
        "https://evil.example.com/SuperLink.zip",
        "https://github.com/other/repo/releases/download/v0.2.0/SuperLink.zip",
        "https://user:secret@github.com/ealink1/super-link/releases/download/v0.2.0/x.zip",
    ] {
        artifact.url = url.into();
        assert!(
            manifest("0.2.0", vec![artifact.clone()])
                .validate()
                .is_err(),
            "{url} should be rejected"
        );
    }
}

#[test]
fn verify_release_accepts_a_matching_download() {
    let temp = TempDir::new();
    let payload = b"application package".to_vec();
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact("0.2.0", "SuperLink.zip", &payload, platform())],
    );
    write_file(&temp.join("SuperLink.zip"), &payload);
    let (raw, signature, key) = signed(&manifest);

    let verification =
        verify_release(&raw, &signature, &key, temp.path(), platform()).expect("release verifies");

    let app = verification.app().expect("app artifact");
    assert_eq!(app.size, payload.len() as u64);
    assert_eq!(app.sha256, sha256_hex(&payload));
    assert_eq!(app.path, temp.join("SuperLink.zip"));
}

#[test]
fn verify_release_rejects_a_hash_mismatch() {
    let temp = TempDir::new();
    let payload = b"application package".to_vec();
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact("0.2.0", "SuperLink.zip", &payload, platform())],
    );
    write_file(&temp.join("SuperLink.zip"), b"application packagX");
    let (raw, signature, key) = signed(&manifest);

    let error = verify_release(&raw, &signature, &key, temp.path(), platform())
        .expect_err("hash mismatch must fail");

    assert!(matches!(error, UpdateError::HashMismatch { .. }));
}

#[test]
fn verify_release_rejects_a_size_mismatch() {
    let temp = TempDir::new();
    let payload = b"application package".to_vec();
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact("0.2.0", "SuperLink.zip", &payload, platform())],
    );
    write_file(&temp.join("SuperLink.zip"), b"short");
    let (raw, signature, key) = signed(&manifest);

    let error = verify_release(&raw, &signature, &key, temp.path(), platform())
        .expect_err("size mismatch must fail");

    assert!(matches!(error, UpdateError::SizeMismatch { .. }));
}

#[test]
fn verify_release_rejects_a_missing_platform_artifact() {
    let temp = TempDir::new();
    let payload = b"application package".to_vec();
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact(
            "0.2.0",
            "SuperLink.zip",
            &payload,
            other_platform(),
        )],
    );
    write_file(&temp.join("SuperLink.zip"), &payload);
    let (raw, signature, key) = signed(&manifest);

    let error = verify_release(&raw, &signature, &key, temp.path(), platform())
        .expect_err("platform mismatch must fail");

    assert!(matches!(error, UpdateError::NoPlatformArtifact { .. }));
}

#[test]
fn verify_release_skips_artifacts_for_other_platforms() {
    let temp = TempDir::new();
    let payload = b"application package".to_vec();
    let manifest = manifest(
        "0.2.0",
        vec![
            app_artifact("0.2.0", "SuperLink.zip", &payload, platform()),
            driver_artifact("0.1.0", "sqlite-agent", b"driver", other_platform()),
        ],
    );
    write_file(&temp.join("SuperLink.zip"), &payload);
    let (raw, signature, key) = signed(&manifest);

    let verification =
        verify_release(&raw, &signature, &key, temp.path(), platform()).expect("release verifies");

    assert_eq!(verification.artifacts.len(), 1);
    assert_eq!(verification.artifacts[0].artifact.kind, ArtifactKind::App);
}

#[cfg(unix)]
#[test]
fn verify_release_rejects_a_symlinked_artifact() {
    let temp = TempDir::new();
    let payload = b"application package".to_vec();
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact("0.2.0", "SuperLink.zip", &payload, platform())],
    );
    let target = temp.join("real.zip");
    write_file(&target, &payload);
    std::os::unix::fs::symlink(&target, temp.join("SuperLink.zip")).expect("symlink");
    let (raw, signature, key) = signed(&manifest);

    let error = verify_release(&raw, &signature, &key, temp.path(), platform())
        .expect_err("symlinked artifact must fail");

    assert!(matches!(error, UpdateError::Archive(_)));
}

#[test]
fn manifests_report_newer_versions() {
    let manifest = manifest(
        "0.3.0",
        vec![app_artifact(
            "0.3.0",
            "SuperLink.zip",
            b"payload",
            platform(),
        )],
    );

    assert!(manifest.is_newer_than("0.2.9"));
    assert!(!manifest.is_newer_than("0.3.0"));
    assert!(!manifest.is_newer_than("0.3.1"));
}

#[test]
fn artifact_lookup_is_platform_aware() {
    let manifest = manifest(
        "0.2.0",
        vec![app_artifact(
            "0.2.0",
            "SuperLink.zip",
            b"payload",
            platform(),
        )],
    );
    let found = manifest
        .artifact(
            ArtifactKind::App,
            "superlink",
            platform().os,
            platform().arch,
        )
        .expect("artifact");
    assert_eq!(found.filename, "SuperLink.zip");

    let missing = manifest.artifact(ArtifactKind::App, "superlink", Os::Linux, platform().arch);
    assert!(matches!(
        missing,
        Err(UpdateError::NoPlatformArtifact { .. })
    ));
}

#[test]
fn artifacts_reject_empty_urls() {
    let mut artifact = app_artifact("0.2.0", "SuperLink.zip", b"payload", platform());
    artifact.url = String::new();
    assert!(artifact.validate().is_err());
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[test]
fn platform_names_match_release_manifests() {
    assert_eq!(Os::parse("darwin"), Some(Os::Darwin));
    assert_eq!(Os::parse("plan9"), None);
    assert_eq!(
        Platform::new(Os::Linux, superlink_update::Arch::Arm64).to_string(),
        "linux/arm64"
    );
}
