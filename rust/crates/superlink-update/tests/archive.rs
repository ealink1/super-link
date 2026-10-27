mod common;

use std::fs;
use std::io::Write;
use std::path::Path;

use common::TempDir;
use superlink_update::{Limits, UpdateError, safe_archive_path, stage_archive, stage_archive_with};

fn write_zip(path: &Path, entries: &[(&str, &[u8], u32)]) {
    let file = fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    for (name, data, mode) in entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(*mode);
        if name.ends_with('/') {
            writer.add_directory(*name, options).expect("add directory");
        } else {
            writer.start_file(*name, options).expect("start file");
            writer.write_all(data).expect("write entry");
        }
    }
    writer.finish().expect("finish zip");
}

struct TarEntry<'a> {
    name: &'a str,
    mode: u32,
    data: &'a [u8],
    kind: u8,
    link: Option<&'a str>,
}

fn regular<'a>(name: &'a str, data: &'a [u8], mode: u32) -> TarEntry<'a> {
    TarEntry {
        name,
        mode,
        data,
        kind: b'0',
        link: None,
    }
}

/// Writes a tar header verbatim so hostile names reach the extractor.
fn write_tar_gz(path: &Path, entries: &[TarEntry<'_>]) {
    let mut raw = Vec::new();
    for entry in entries {
        let mut header = [0u8; 512];
        copy_text(&mut header[0..100], entry.name);
        write_octal(&mut header[100..108], entry.mode as u64);
        write_octal(&mut header[108..116], 0);
        write_octal(&mut header[116..124], 0);
        write_octal(&mut header[124..136], entry.data.len() as u64);
        write_octal(&mut header[136..148], 0);
        header[148..156].fill(b' ');
        header[156] = entry.kind;
        if let Some(link) = entry.link {
            copy_text(&mut header[157..257], link);
        }
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        write_checksum(&mut header[148..156], checksum);
        raw.extend_from_slice(&header);
        raw.extend_from_slice(entry.data);
        let padding = (512 - entry.data.len() % 512) % 512;
        raw.extend(std::iter::repeat_n(0u8, padding));
    }
    raw.extend(std::iter::repeat_n(0u8, 1024));
    let file = fs::File::create(path).expect("create tar");
    let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    encoder.write_all(&raw).expect("write tar");
    encoder.finish().expect("finish tar");
}

fn copy_text(slot: &mut [u8], text: &str) {
    let bytes = text.as_bytes();
    let length = bytes.len().min(slot.len() - 1);
    slot[..length].copy_from_slice(&bytes[..length]);
}

fn write_octal(slot: &mut [u8], value: u64) {
    let digits = slot.len() - 1;
    let text = format!("{value:0digits$o}");
    slot[..digits].copy_from_slice(&text.as_bytes()[text.len() - digits..]);
    slot[digits] = 0;
}

fn write_checksum(slot: &mut [u8], checksum: u32) {
    let text = format!("{checksum:06o}");
    slot[..6].copy_from_slice(text.as_bytes());
    slot[6] = 0;
    slot[7] = b' ';
}

#[test]
fn stage_archive_extracts_a_single_root() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            (
                "SuperLink.app/Contents/MacOS/superlink",
                b"binary",
                0o100755,
            ),
            (
                "SuperLink.app/Contents/Resources/readme.txt",
                b"readme",
                0o100644,
            ),
        ],
    );
    let destination = temp.join("stage");

    let root = stage_archive(&archive, &destination).expect("extract");

    assert_eq!(root, destination.join("SuperLink.app"));
    assert_eq!(
        fs::read(root.join("Contents/MacOS/superlink")).expect("read binary"),
        b"binary"
    );
    assert_eq!(
        fs::read(root.join("Contents/Resources/readme.txt")).expect("read readme"),
        b"readme"
    );
}

#[cfg(unix)]
#[test]
fn stage_archive_preserves_only_executable_bits() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            ("SuperLink.app/run", b"binary", 0o104755),
            ("SuperLink.app/readme", b"text", 0o100644),
        ],
    );
    let destination = temp.join("stage");

    let root = stage_archive(&archive, &destination).expect("extract");

    let run = fs::metadata(root.join("run")).expect("run metadata");
    assert_eq!(run.permissions().mode() & 0o777, 0o700);
    let readme = fs::metadata(root.join("readme")).expect("readme metadata");
    assert_eq!(readme.permissions().mode() & 0o777, 0o600);
}

#[test]
fn stage_archive_rejects_parent_traversal() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            ("../escape.txt", b"escape", 0o100644),
        ],
    );
    let destination = temp.join("stage");

    let error = stage_archive(&archive, &destination).expect_err("traversal must fail");

    assert!(matches!(error, UpdateError::UnsafePath(_)));
    assert!(!temp.join("escape.txt").exists());
    assert!(!destination.exists());
}

#[test]
fn stage_archive_rejects_parent_traversal_inside_a_directory() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            ("SuperLink.app/../../escape.txt", b"escape", 0o100644),
        ],
    );

    let error = stage_archive(&archive, &temp.join("stage")).expect_err("traversal must fail");

    assert!(matches!(error, UpdateError::UnsafePath(_)));
}

#[test]
fn stage_archive_rejects_absolute_paths() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            ("/etc/passwd", b"x", 0o100644),
        ],
    );

    let error = stage_archive(&archive, &temp.join("stage")).expect_err("absolute path");

    assert!(matches!(error, UpdateError::UnsafePath(_)));
}

#[test]
fn stage_archive_rejects_symlinks() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    let file = fs::File::create(&archive).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    writer
        .add_directory("SuperLink.app/", options)
        .expect("directory");
    writer
        .add_symlink("SuperLink.app/link", "/etc/passwd", options)
        .expect("symlink entry");
    writer.finish().expect("finish zip");

    let error = stage_archive(&archive, &temp.join("stage")).expect_err("symlink");

    assert!(matches!(error, UpdateError::Archive(_)));
}

#[test]
fn stage_archive_rejects_case_colliding_duplicates() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            ("SuperLink.app/Readme.txt", b"one", 0o100644),
            ("SuperLink.app/README.txt", b"two", 0o100644),
        ],
    );

    let error = stage_archive(&archive, &temp.join("stage")).expect_err("duplicate");

    assert!(matches!(error, UpdateError::DuplicateEntry(_)));
}

#[test]
fn stage_archive_rejects_windows_device_names() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            ("SuperLink.app/CON.txt", b"x", 0o100644),
        ],
    );

    let error = stage_archive(&archive, &temp.join("stage")).expect_err("device name");

    assert!(matches!(error, UpdateError::UnsafePath(_)));
}

#[test]
fn stage_archive_rejects_non_canonical_names() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            ("SuperLink.app/./readme.txt", b"x", 0o100644),
        ],
    );

    let error = stage_archive(&archive, &temp.join("stage")).expect_err("non canonical");

    assert!(matches!(error, UpdateError::UnsafePath(_)));
}

#[test]
fn stage_archive_requires_exactly_one_root() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("A/readme.txt", b"one", 0o100644),
            ("B/readme.txt", b"two", 0o100644),
        ],
    );

    let error = stage_archive(&archive, &temp.join("stage")).expect_err("two roots");

    assert!(matches!(error, UpdateError::Archive(_)));
}

#[test]
fn stage_archive_rejects_empty_and_unknown_archives() {
    let temp = TempDir::new();
    let empty = temp.join("empty.zip");
    write_zip(&empty, &[]);
    assert!(matches!(
        stage_archive(&empty, &temp.join("stage-a")).expect_err("empty"),
        UpdateError::Archive(_)
    ));

    let unknown = temp.join("app.bin");
    common::write_file(&unknown, b"not an archive at all");
    assert!(matches!(
        stage_archive(&unknown, &temp.join("stage-b")).expect_err("unknown"),
        UpdateError::Archive(_)
    ));
}

#[test]
fn stage_archive_enforces_the_extraction_budget() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    let payload = vec![b'a'; 4096];
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            ("SuperLink.app/big.bin", &payload, 0o100644),
        ],
    );
    let destination = temp.join("stage");
    let limits = Limits {
        max_extracted_bytes: 64,
        max_entries: 100,
    };

    let error = stage_archive_with(&archive, &destination, limits).expect_err("budget");

    assert!(matches!(error, UpdateError::ExtractionBudget(64)));
    assert!(!destination.exists());
}

#[test]
fn stage_archive_enforces_the_entry_count() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(
        &archive,
        &[
            ("SuperLink.app/", b"", 0o40755),
            ("SuperLink.app/a.txt", b"a", 0o100644),
            ("SuperLink.app/b.txt", b"b", 0o100644),
        ],
    );
    let limits = Limits {
        max_extracted_bytes: 1024,
        max_entries: 2,
    };

    let error = stage_archive_with(&archive, &temp.join("stage"), limits).expect_err("count");

    assert!(matches!(error, UpdateError::Archive(_)));
}

#[test]
fn stage_archive_requires_an_empty_destination() {
    let temp = TempDir::new();
    let archive = temp.join("app.zip");
    write_zip(&archive, &[("SuperLink.app/", b"", 0o40755)]);
    let destination = temp.join("stage");
    common::write_file(&destination.join("leftover.txt"), b"x");

    let error = stage_archive(&archive, &destination).expect_err("non empty destination");

    assert!(matches!(error, UpdateError::Archive(_)));
}

#[test]
fn stage_archive_accepts_dot_prefixed_tar_gz() {
    let temp = TempDir::new();
    let archive = temp.join("app.tgz");
    write_tar_gz(
        &archive,
        &[
            TarEntry {
                name: "./",
                mode: 0o755,
                data: b"",
                kind: b'5',
                link: None,
            },
            TarEntry {
                name: "./SuperLink.app/",
                mode: 0o755,
                data: b"",
                kind: b'5',
                link: None,
            },
            regular("./SuperLink.app/superlink", b"binary", 0o755),
        ],
    );
    let destination = temp.join("stage");

    let root = stage_archive(&archive, &destination).expect("extract tar.gz");

    assert_eq!(root, destination.join("SuperLink.app"));
    assert_eq!(
        fs::read(root.join("superlink")).expect("read binary"),
        b"binary"
    );
}

#[test]
fn stage_archive_rejects_tar_links() {
    let temp = TempDir::new();
    let archive = temp.join("app.tgz");
    write_tar_gz(
        &archive,
        &[
            TarEntry {
                name: "SuperLink.app/",
                mode: 0o755,
                data: b"",
                kind: b'5',
                link: None,
            },
            TarEntry {
                name: "SuperLink.app/link",
                mode: 0o777,
                data: b"",
                kind: b'2',
                link: Some("/etc/passwd"),
            },
        ],
    );
    let destination = temp.join("stage");

    let error = stage_archive(&archive, &destination).expect_err("tar symlink");

    assert!(matches!(error, UpdateError::Archive(_)));
    assert!(!destination.exists());
}

#[test]
fn stage_archive_rejects_tar_traversal() {
    let temp = TempDir::new();
    let archive = temp.join("app.tgz");
    write_tar_gz(&archive, &[regular("../escape.txt", b"escape", 0o644)]);

    let error = stage_archive(&archive, &temp.join("stage")).expect_err("tar traversal");

    assert!(matches!(error, UpdateError::UnsafePath(_)));
}

#[test]
fn safe_archive_path_accepts_release_layouts() {
    assert_eq!(
        safe_archive_path("SuperLink.app/Contents/MacOS/superlink").expect("valid"),
        "SuperLink.app/Contents/MacOS/superlink"
    );
    assert_eq!(
        safe_archive_path("SuperLink/").expect("valid dir"),
        "SuperLink"
    );
    assert_eq!(
        safe_archive_path("drivers/sqlite-agent").expect("valid"),
        "drivers/sqlite-agent"
    );
}

#[test]
fn safe_archive_path_rejects_hostile_names() {
    for name in [
        "",
        "/etc/passwd",
        "../escape",
        "a/../../escape",
        "a//b",
        "a/./b",
        "C:\\Windows\\system32",
        "a\\b",
        "SuperLink.app/readme.",
        "SuperLink.app/readme ",
        "SuperLink.app/NUL",
        "SuperLink.app/LPT9.log",
        "SuperLink.app/COM3",
    ] {
        assert!(
            safe_archive_path(name).is_err(),
            "{name:?} should be rejected"
        );
    }
}
