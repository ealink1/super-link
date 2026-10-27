mod common;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use common::*;
use superlink_update::{
    ApplyRequest, UpdateError, apply_staged, apply_staged_with, prepare_update, validate_request,
};

fn package_path(temp: &TempDir) -> PathBuf {
    temp.join("SuperLink")
}

fn request(target: &Path, stage: PathBuf, token: &str, version: &str) -> ApplyRequest {
    ApplyRequest {
        backup: target.with_file_name(format!(
            "{}.backup-{token}",
            target.file_name().expect("name").to_string_lossy()
        )),
        target: target.to_path_buf(),
        stage,
        token: token.to_owned(),
        version: version.to_owned(),
        state: None,
    }
}

fn staged(temp: &TempDir, target: &Path, token: &str, version: &str) -> PathBuf {
    let stage = target.with_file_name(format!(".superlink-stage-{token}"));
    create_package(&stage, version, platform());
    let _ = temp;
    stage
}

fn zip_package(root: &Path, prefix: &str, archive: &Path) {
    let file = fs::File::create(archive).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    writer
        .add_directory(format!("{prefix}/"), options)
        .expect("directory");
    for relative in [
        "superlink.package.json",
        "bin/superlink",
        "bin/update-helper",
    ] {
        writer
            .start_file(format!("{prefix}/{relative}"), options)
            .expect("start file");
        writer
            .write_all(&fs::read(root.join(relative)).expect("read source"))
            .expect("write entry");
    }
    writer.finish().expect("finish zip");
}

#[test]
fn apply_staged_replaces_the_application() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let stage = staged(&temp, &target, "abc", "0.2.0");
    let request = request(&target, stage.clone(), "abc", "0.2.0");

    validate_request(&request).expect("request validates");
    let report = apply_staged(&request);

    assert!(report.success, "{}", report.message);
    assert!(!report.rolled_back);
    assert_eq!(report.version, "0.2.0");
    assert!(
        fs::read_to_string(target.join("bin/superlink"))
            .expect("new binary")
            .contains("0.2.0")
    );
    assert!(
        fs::read_to_string(request.backup.join("bin/superlink"))
            .expect("old binary")
            .contains("0.1.0")
    );
    assert!(!stage.exists());
}

#[test]
fn apply_staged_refuses_an_existing_backup() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let stage = staged(&temp, &target, "abc", "0.2.0");
    let request = request(&target, stage, "abc", "0.2.0");
    common::write_file(&request.backup.join("keep.txt"), b"x");

    let report = apply_staged(&request);

    assert!(!report.success);
    assert!(
        report.message.contains("already exists"),
        "{}",
        report.message
    );
    assert!(
        fs::read_to_string(target.join("bin/superlink"))
            .expect("original binary")
            .contains("0.1.0")
    );
}

#[test]
fn apply_staged_refuses_a_stage_outside_the_target_directory() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let stage = temp.join("elsewhere/.superlink-stage-abc");
    create_package(&stage, "0.2.0", platform());
    let request = request(&target, stage, "abc", "0.2.0");

    let error = validate_request(&request).expect_err("siblings required");

    assert!(matches!(error, UpdateError::Request(_)));
}

#[test]
fn apply_staged_refuses_a_version_mismatch() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let stage = staged(&temp, &target, "abc", "0.2.0");
    let request = request(&target, stage, "abc", "0.3.0");

    let error = validate_request(&request).expect_err("version mismatch");

    assert!(matches!(error, UpdateError::Request(_)));
}

#[test]
fn apply_staged_rolls_back_when_the_new_build_fails() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let stage = staged(&temp, &target, "abc", "0.2.0");
    let request = request(&target, stage.clone(), "abc", "0.2.0");

    let report = apply_staged_with(&request, |_| {
        Err(UpdateError::Request(
            "new build exited during startup".into(),
        ))
    });

    assert!(!report.success);
    assert!(report.rolled_back, "{}", report.message);
    assert!(
        report.message.contains("new build exited"),
        "{}",
        report.message
    );
    assert!(
        fs::read_to_string(target.join("bin/superlink"))
            .expect("restored binary")
            .contains("0.1.0")
    );
    assert!(!request.backup.exists());
    let failed = PathBuf::from(format!("{}.failed", stage.display()));
    assert!(failed.join("bin/superlink").exists());
}

#[test]
fn apply_staged_restores_local_state_after_a_failed_launch() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let stage = staged(&temp, &target, "abc", "0.2.0");
    let state = temp.join("data/state.sqlite");
    let state_backup = temp.join("updates/state-abc.sqlite");
    common::write_file(&state, b"old-state");
    let request = request(&target, stage, "abc", "0.2.0").with_state(&state, &state_backup);

    let report = apply_staged_with(&request, |request| {
        let source = &request.state.as_ref().expect("state").source;
        common::write_file(source, b"migrated-state");
        Err(UpdateError::Request("health check failed".into()))
    });

    assert!(report.rolled_back, "{}", report.message);
    assert_eq!(fs::read(&state).expect("state"), b"old-state");
    assert!(
        fs::read_to_string(target.join("bin/superlink"))
            .expect("restored binary")
            .contains("0.1.0")
    );
}

#[test]
fn prepare_update_stages_a_downloaded_archive() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let source = temp.join("build/SuperLink");
    create_package(&source, "0.2.0", platform());
    let archive = temp.join("downloads/SuperLink_0.2.0.zip");
    fs::create_dir_all(archive.parent().expect("parent")).expect("create downloads");
    zip_package(&source, "SuperLink", &archive);
    let updates = temp.join("updates");

    let request = prepare_update(&archive, &updates, &target, "0.2.0").expect("prepare");

    assert_eq!(request.version, "0.2.0");
    assert_eq!(
        fs::canonicalize(&request.target).expect("target"),
        fs::canonicalize(&target).expect("target")
    );
    assert!(request.stage.join("superlink.package.json").exists());
    assert!(!request.backup.exists());
    validate_request(&request).expect("prepared request validates");

    let report = apply_staged(&request);

    assert!(report.success, "{}", report.message);
    assert!(
        fs::read_to_string(target.join("bin/superlink"))
            .expect("updated binary")
            .contains("0.2.0")
    );
}

#[test]
fn prepare_update_rejects_a_version_mismatch() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let source = temp.join("build/SuperLink");
    create_package(&source, "0.2.0", platform());
    let archive = temp.join("downloads/SuperLink_0.2.0.zip");
    fs::create_dir_all(archive.parent().expect("parent")).expect("create downloads");
    zip_package(&source, "SuperLink", &archive);

    let error = prepare_update(&archive, &temp.join("updates"), &target, "0.3.0")
        .expect_err("version mismatch");

    assert!(matches!(error, UpdateError::Request(_)));
}

#[test]
fn prepare_update_rejects_an_archive_with_two_roots() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let archive = temp.join("downloads/bad.zip");
    fs::create_dir_all(archive.parent().expect("parent")).expect("create downloads");
    let file = fs::File::create(&archive).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("a/readme.txt", options).expect("file");
    writer.write_all(b"a").expect("write");
    writer.start_file("b/readme.txt", options).expect("file");
    writer.write_all(b"b").expect("write");
    writer.finish().expect("finish");

    let error =
        prepare_update(&archive, &temp.join("updates"), &target, "0.2.0").expect_err("two roots");

    assert!(matches!(error, UpdateError::Archive(_)));
}

#[test]
fn apply_report_is_written_as_json() {
    let temp = TempDir::new();
    let target = package_path(&temp);
    create_package(&target, "0.1.0", platform());
    let stage = staged(&temp, &target, "abc", "0.2.0");
    let request = request(&target, stage, "abc", "0.2.0");
    let report = apply_staged(&request);
    let path = temp.join("updates/last-update.json");

    report.write(&path).expect("write report");

    let raw = fs::read_to_string(&path).expect("read report");
    assert!(raw.contains("\"rolledBack\""), "{raw}");
    assert!(raw.contains("\"success\": true"), "{raw}");
}
