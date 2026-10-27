//! SQL document storage and streaming statement splitting.

use std::fs;
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;

use superlink_files::query_file;
use superlink_files::sql_stream::statements;
use superlink_files::{CancelFlag, FileError, NeverCancelled};

#[test]
fn sql_document_round_trip_permissions_and_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("查询.sql");
    let text = "SELECT '中文', E'\\n';\n-- 注释\n";
    query_file::save(&NeverCancelled, &path, text, false).unwrap();
    assert!(matches!(
        query_file::save(&NeverCancelled, &path, "replaced", false),
        Err(FileError::AlreadyExists)
    ));
    assert_eq!(query_file::read(&NeverCancelled, &path).unwrap(), text);
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "SQL file is not private");

    let oversized = "x".repeat(superlink_files::limits::MAX_SQL_BYTES + 1);
    assert!(query_file::save(&NeverCancelled, &path, &oversized, true).is_err());
    assert_eq!(query_file::read(&NeverCancelled, &path).unwrap(), text);
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        1,
        "temporary SQL file leaked"
    );
}

#[test]
fn sql_read_accepts_bom_and_rejects_malformed_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("query.sql");

    let mut bom = vec![0xef, 0xbb, 0xbf];
    bom.extend_from_slice(b"SELECT 1");
    fs::write(&path, bom).unwrap();
    assert_eq!(
        query_file::read(&NeverCancelled, &path).unwrap(),
        "SELECT 1"
    );

    fs::write(&path, [0xff, 0xfe, b'S', 0x00]).unwrap();
    assert!(query_file::read(&NeverCancelled, &path).is_err());

    let oversized = "x".repeat(superlink_files::limits::MAX_SQL_BYTES + 4);
    fs::write(&path, oversized.as_bytes()).unwrap();
    assert!(query_file::read(&NeverCancelled, &path).is_err());
}

#[test]
fn cancelled_sql_save_keeps_the_previous_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("query.sql");
    query_file::save(&NeverCancelled, &path, "SELECT 1", false).unwrap();
    let flag = CancelFlag::new();
    flag.cancel();
    let error = query_file::save(&flag, &path, "SELECT 2", true).unwrap_err();
    assert!(matches!(error, FileError::Cancelled));
    assert_eq!(
        query_file::read(&NeverCancelled, &path).unwrap(),
        "SELECT 1"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn streaming_statements_respect_strings_comments_and_dollar_quotes() {
    let backslash = char::from(92);
    let backtick = char::from(96);
    let newline = char::from(10);
    let cases = [
        (
            "mysql",
            format!(
                "-- comment;{newline}SELECT 'a;''b';/* c; */SELECT {backtick}x;y{backtick}; # done;{newline}"
            ),
            2,
        ),
        (
            "postgres",
            format!("SELECT $$a;b$$;SELECT 'a{backslash}';SELECT E'a{backslash}';b';"),
            3,
        ),
        ("sqlserver", "SELECT [a;]]b]; SELECT 2;".to_string(), 2),
    ];
    for (dialect, text, expected) in cases {
        let mut count = 0;
        let hash = statements(
            &NeverCancelled,
            Cursor::new(text),
            dialect,
            |statement, _| {
                count += 1;
                assert!(!statement.is_empty());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(count, expected, "{dialect}");
        assert_eq!(hash.len(), 64, "{dialect}");
    }
}

#[test]
fn streaming_statements_have_no_statement_size_limit() {
    let payload = "x".repeat(2 << 20);
    let text = format!("SELECT '{payload}'; SELECT 2;");
    let mut count = 0;
    statements(&NeverCancelled, Cursor::new(text), "mysql", |_, _| {
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn streaming_statements_reject_malformed_sql_and_cancellation() {
    let newline = char::from(10);
    let nul = char::from(0);
    let invalid = [
        "SELECT 'unclosed".to_string(),
        "SELECT 1; /* open".to_string(),
        format!("SELECT {nul}"),
        "SELECT $$open".to_string(),
        "SELECT /*!40101 1 */".to_string(),
        format!("SELECT 1;{newline}SELECT 2;"),
    ];
    for text in invalid {
        let result = statements(
            &NeverCancelled,
            Cursor::new(text.clone()),
            "postgres",
            |_, _| Ok(()),
        );
        if !text.contains("SELECT 2;") {
            assert!(result.is_err(), "accepted malformed SQL: {text}");
        } else {
            assert!(result.is_ok());
        }
    }

    let flag = CancelFlag::new();
    flag.cancel();
    let error = statements(&flag, Cursor::new("SELECT 1;"), "mysql", |_, _| Ok(())).unwrap_err();
    assert!(matches!(error, FileError::Cancelled));
}

#[test]
fn emitted_statements_are_trimmed_and_report_the_end_offset() {
    let newline = char::from(10);
    let text = format!("  SELECT 1 ;{newline}SELECT 2;");
    let mut seen: Vec<(String, u64)> = Vec::new();
    statements(
        &NeverCancelled,
        Cursor::new(text),
        "mysql",
        |statement, offset| {
            seen.push((statement.to_string(), offset));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(seen[0].0, "SELECT 1");
    assert_eq!(seen[1].0, "SELECT 2");
    assert!(seen[0].1 < seen[1].1);
}
