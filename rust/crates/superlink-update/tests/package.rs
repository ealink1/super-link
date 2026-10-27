mod common;

use std::fs;

use common::*;
use superlink_update::{
    Arch, Os, Platform, UpdateError, read_package, read_package_for, running_root, running_root_for,
};

fn darwin() -> Platform {
    Platform::new(Os::Darwin, Arch::Arm64)
}

#[test]
fn read_package_accepts_a_root_marker() {
    let temp = TempDir::new();
    let root = temp.join("SuperLink");
    create_package(&root, "0.2.0", platform());

    let package = read_package(&root).expect("package");

    assert_eq!(package.version, "0.2.0");
    assert_eq!(package.executable, "bin/superlink");
}

#[test]
fn read_package_accepts_a_macos_resource_marker() {
    let temp = TempDir::new();
    let root = temp.join("SuperLink.app");
    create_bundle(&root, "0.2.0", darwin());

    let package = read_package_for(&root, darwin()).expect("package");

    assert_eq!(package.executable, "Contents/MacOS/superlink");
}

#[test]
fn read_package_rejects_a_platform_mismatch() {
    let temp = TempDir::new();
    let root = temp.join("SuperLink");
    create_package(&root, "0.2.0", darwin());

    let error = read_package_for(&root, Platform::new(Os::Linux, Arch::Arm64))
        .expect_err("platform mismatch");

    assert!(matches!(error, UpdateError::Package(_)));
}

#[test]
fn read_package_rejects_another_bundle_identifier() {
    let temp = TempDir::new();
    let root = temp.join("SuperLink");
    create_package(&root, "0.2.0", platform());
    let marker = common::package_marker("0.2.0", platform(), "bin/superlink", "bin/update-helper")
        .replace("io.github.ealink1.superlink", "io.example.other");
    common::write_file(&root.join("superlink.package.json"), marker.as_bytes());

    let error = read_package(&root).expect_err("identifier mismatch");

    assert!(matches!(error, UpdateError::Package(_)));
}

#[test]
fn read_package_rejects_a_missing_entry_point() {
    let temp = TempDir::new();
    let root = temp.join("SuperLink");
    create_package(&root, "0.2.0", platform());
    fs::remove_file(root.join("bin/update-helper")).expect("remove helper");

    let error = read_package(&root).expect_err("missing helper");

    assert!(matches!(error, UpdateError::Io { .. }));
}

#[test]
fn read_package_rejects_an_oversized_marker() {
    let temp = TempDir::new();
    let root = temp.join("SuperLink");
    create_package(&root, "0.2.0", platform());
    common::write_file(&root.join("superlink.package.json"), &vec![b'x'; 5000]);

    let error = read_package(&root).expect_err("oversized marker");

    assert!(matches!(error, UpdateError::Package(_)));
}

#[test]
fn read_package_rejects_an_invalid_marker() {
    let temp = TempDir::new();
    let root = temp.join("SuperLink");
    create_package(&root, "0.2.0", platform());
    common::write_file(&root.join("superlink.package.json"), b"{ not json");

    let error = read_package(&root).expect_err("invalid marker");

    assert!(matches!(error, UpdateError::Package(_)));
}

#[cfg(unix)]
#[test]
fn read_package_rejects_a_symlinked_root() {
    let temp = TempDir::new();
    let root = temp.join("SuperLink");
    create_package(&root, "0.2.0", platform());
    let link = temp.join("Linked");
    std::os::unix::fs::symlink(&root, &link).expect("symlink root");

    let error = read_package(&link).expect_err("symlinked root");

    assert!(matches!(error, UpdateError::Package(_)));
}

#[cfg(unix)]
#[test]
fn read_package_rejects_a_symlinked_entry_point() {
    let temp = TempDir::new();
    let root = temp.join("SuperLink");
    create_package(&root, "0.2.0", platform());
    let helper = root.join("bin/update-helper");
    fs::remove_file(&helper).expect("remove helper");
    std::os::unix::fs::symlink("/bin/sh", &helper).expect("symlink helper");

    let error = read_package(&root).expect_err("symlinked helper");

    assert!(matches!(error, UpdateError::Package(_)));
}

#[test]
fn running_root_resolves_a_macos_bundle() {
    let temp = TempDir::new();
    let bundle = temp.join("SuperLink.app");
    create_bundle(&bundle, "0.2.0", darwin());
    let executable = bundle.join("Contents/MacOS/superlink");

    let root = running_root_for(&executable, darwin()).expect("running root");

    assert_eq!(root, fs::canonicalize(&bundle).expect("canonical bundle"));
}

#[test]
fn running_root_rejects_a_bare_binary() {
    let temp = TempDir::new();
    let executable = temp.join("superlink");
    common::write_file(&executable, b"binary");

    let error = running_root_for(&executable, darwin()).expect_err("bare binary");

    assert!(matches!(error, UpdateError::NotPackaged));
}

#[test]
fn running_root_rejects_a_foreign_executable() {
    let temp = TempDir::new();
    let bundle = temp.join("SuperLink.app");
    create_bundle(&bundle, "0.2.0", darwin());
    let other = temp.join("SuperLink.app/Contents/MacOS/other");
    common::write_file(&other, b"binary");

    let error = running_root_for(&other, darwin()).expect_err("foreign executable");

    assert!(matches!(error, UpdateError::ExecutableMismatch));
}

#[test]
fn running_root_uses_the_current_platform() {
    let temp = TempDir::new();
    let host = platform();
    let (root, executable) = if host.os == Os::Darwin {
        let bundle = temp.join("SuperLink.app");
        create_bundle(&bundle, "0.2.0", host);
        (bundle.clone(), bundle.join("Contents/MacOS/superlink"))
    } else {
        let root = temp.join("SuperLink");
        create_package(&root, "0.2.0", host);
        (root.clone(), root.join("bin/superlink"))
    };

    let resolved = running_root(&executable).expect("running root");

    assert_eq!(resolved, fs::canonicalize(&root).expect("canonical root"));
}
