//! Import behaviour: encoding, quoting, budgets and XLSX round trips.

use std::fs;
use std::io::Write;

use superlink_files::import::{Dataset, Encoding, ImportFormat, ReadOptions, read_file};
use superlink_files::value::Cell;
use superlink_files::{CancelFlag, FileError, NeverCancelled};

fn write(dir: &std::path::Path, name: &str, raw: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(raw).unwrap();
    path
}

fn read(path: &std::path::Path, options: &ReadOptions) -> superlink_files::Result<Dataset> {
    read_file(&NeverCancelled, path, options)
}

#[test]
fn csv_import_preserves_null_literal_and_chinese_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let text = "\u{feff}id,name,optional\n9223372036854775807,中文,\n2,NULL,保留\n";
    let path = write(dir.path(), "items.csv", text.as_bytes());
    let options = ReadOptions {
        empty_as_null: true,
        ..ReadOptions::default()
    };
    let data = read(&path, &options).unwrap();
    assert_eq!(data.columns, ["id", "name", "optional"]);
    assert_eq!(data.rows[0][0], Cell::Text("9223372036854775807".into()));
    assert_eq!(data.rows[0][1], Cell::Text("中文".into()));
    assert_eq!(data.rows[0][2], Cell::Null);
    assert_eq!(data.rows[1][1], Cell::Text("NULL".into()));

    let (gbk, _, failed) = encoding_rs::GBK.encode("id,name\n1,中文\n");
    assert!(!failed);
    let path = write(dir.path(), "gbk.csv", &gbk);
    let data = read(
        &path,
        &ReadOptions {
            encoding: Encoding::Gbk,
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(data.rows[0][1], Cell::Text("中文".into()));
}

#[test]
fn csv_quoted_commas_quotes_and_newlines_survive_a_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let quote = '"';
    let newline = '\n';
    let source = format!("a,b\n{quote}x,{quote}{quote}y\nz{quote},1\n");
    let path = write(dir.path(), "quoted.csv", source.as_bytes());
    let data = read(&path, &ReadOptions::default()).unwrap();
    assert_eq!(data.rows.len(), 1);
    let expected = format!("x,{quote}y{newline}z");
    assert_eq!(data.rows[0][0], Cell::Text(expected));
    assert_eq!(data.rows[0][1], Cell::Text("1".into()));

    let output = dir.path().join("round.csv");
    superlink_files::export::save_atomic(
        &NeverCancelled,
        &output,
        true,
        superlink_files::export::ExportOptions::new(superlink_files::export::Format::Csv),
        |encoder| {
            encoder.set_columns(&data.columns)?;
            for row in &data.rows {
                encoder.consume_row_values(row)?;
            }
            Ok(())
        },
    )
    .unwrap();
    let again = read(&output, &ReadOptions::default()).unwrap();
    assert_eq!(again.rows[0][0], data.rows[0][0]);
}

#[test]
fn invalid_utf8_is_rejected_with_an_actionable_message() {
    let dir = tempfile::tempdir().unwrap();
    let raw = [b'a', b'\n', 0xff, b'\n'];
    let path = write(dir.path(), "broken.csv", &raw);
    let error = read(&path, &ReadOptions::default()).unwrap_err();
    assert!(
        error.to_string().contains("UTF-8"),
        "unexpected message: {error}"
    );
}

#[test]
fn json_import_unions_columns_without_losing_integer_precision() {
    let dir = tempfile::tempdir().unwrap();
    let raw =
        r#"[{"id":9223372036854775807,"name":"中文"},{"id":2,"optional":null,"extra":{"x":1}}]"#;
    let path = write(dir.path(), "items.json", raw.as_bytes());
    let data = read(&path, &ReadOptions::default()).unwrap();
    assert_eq!(
        data.columns,
        ["id", "name", "extra", "optional"],
        "columns must be unioned in sorted order"
    );
    let id = data.columns.iter().position(|name| name == "id").unwrap();
    assert_eq!(data.rows[0][id], Cell::Int(i64::MAX));
    let extra = data
        .columns
        .iter()
        .position(|name| name == "extra")
        .unwrap();
    assert_eq!(data.rows[0][extra], Cell::Null);
    assert_eq!(data.rows[1][extra], Cell::Json(serde_json::json!({"x": 1})));
}

#[test]
fn malformed_json_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    for (name, raw) in [
        ("tail.json", r#"[{"a":1}] {"tail":2}"#),
        ("nonobject.json", "[1]"),
        ("duplicate.json", r#"[{"a":1,"a":2}]"#),
        ("empty.json", "[]"),
    ] {
        let path = write(dir.path(), name, raw.as_bytes());
        assert!(
            read(&path, &ReadOptions::default()).is_err(),
            "{name} was accepted"
        );
    }
}

#[test]
fn import_rejects_oversized_files_budgets_and_shapes() {
    let dir = tempfile::tempdir().unwrap();

    let huge = dir.path().join("huge.csv");
    let file = fs::File::create(&huge).unwrap();
    file.set_len(superlink_files::limits::MAX_IMPORT_BYTES + 1)
        .unwrap();
    assert!(matches!(
        read(&huge, &ReadOptions::default()),
        Err(FileError::Invalid(_))
    ));

    let duplicate = write(dir.path(), "duplicate.csv", b"a,a\n1,2\n");
    assert!(read(&duplicate, &ReadOptions::default()).is_err());

    let shape = write(dir.path(), "shape.csv", b"a,b\n1\n");
    assert!(read(&shape, &ReadOptions::default()).is_err());

    let wide = format!("a\n{}\n", "x".repeat((1 << 20) + 1));
    let field = write(dir.path(), "large.csv", wide.as_bytes());
    let error = read(&field, &ReadOptions::default()).unwrap_err();
    assert!(error.to_string().contains("1 MiB"), "{error}");

    let mut rows = String::from("a\n");
    for _ in 0..=superlink_files::limits::MAX_IMPORT_ROWS {
        rows.push_str("1\n");
    }
    let many = write(dir.path(), "many.csv", rows.as_bytes());
    let error = read(&many, &ReadOptions::default()).unwrap_err();
    assert!(error.to_string().contains("100,000 rows"), "{error}");
}

#[test]
fn dataset_enforces_the_cell_budget() {
    let columns: Vec<String> = (0..1024).map(|index| format!("c{index}")).collect();
    let mut data = Dataset::new(columns).unwrap();
    let row = vec![Cell::Null; 1024];
    let mut accepted = 0;
    while data.append_row(row.clone()).is_ok() {
        accepted += 1;
        assert!(accepted < 3_000, "cell budget never engaged");
    }
    assert!(accepted * 1024 <= 2_000_000);
}

#[test]
fn xlsx_round_trip_keeps_text_numbers_and_missing_sheets_fail() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("items.xlsx");
    let columns = vec!["id".to_string(), "name".to_string(), "optional".to_string()];
    superlink_files::export::save_atomic(
        &NeverCancelled,
        &path,
        true,
        superlink_files::export::ExportOptions::new(superlink_files::export::Format::Xlsx),
        |encoder| {
            encoder.set_columns(&columns)?;
            encoder.consume_row_values(&[
                Cell::Text("9223372036854775807".into()),
                Cell::Text("=not-a-formula".into()),
                Cell::Null,
            ])?;
            encoder.consume_row_values(&[
                Cell::Int(7),
                Cell::Text("中文".into()),
                Cell::Bool(true),
            ])?;
            Ok(())
        },
    )
    .unwrap();

    let data = read(
        &path,
        &ReadOptions {
            empty_as_null: true,
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(data.columns, columns);
    assert_eq!(data.rows[0][0], Cell::Text("9223372036854775807".into()));
    assert_eq!(data.rows[0][1], Cell::Text("=not-a-formula".into()));
    assert_eq!(data.rows[0][2], Cell::Null);
    assert_eq!(data.rows[1][0], Cell::Text("7".into()));
    assert_eq!(data.rows[1][1], Cell::Text("中文".into()));

    let missing = ReadOptions {
        sheet: Some("missing".into()),
        ..ReadOptions::default()
    };
    assert!(read(&path, &missing).is_err());

    let first_sheet = ReadOptions {
        format: Some(ImportFormat::Xlsx),
        ..ReadOptions::default()
    };
    assert!(read(&path, &first_sheet).is_ok());
}

#[test]
fn cancelled_import_stops_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "items.csv", b"a\n1\n");
    let flag = CancelFlag::new();
    flag.cancel();
    let error = read_file(&flag, &path, &ReadOptions::default()).unwrap_err();
    assert!(matches!(error, FileError::Cancelled));
}

#[test]
fn truncated_utf8_sequences_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    // 0xE4 0xB8 starts a three byte sequence for 中 but stops early.
    let raw = [b'a', b'\n', 0xe4, 0xb8, b'\n'];
    let path = write(dir.path(), "truncated.csv", &raw);
    let error = read(&path, &ReadOptions::default()).unwrap_err();
    assert!(error.to_string().contains("UTF-8"), "{error}");
}

#[test]
fn tsv_files_use_tab_separators() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "items.tsv", b"a\tb\n1\t2\n");
    let data = read(&path, &ReadOptions::default()).unwrap();
    assert_eq!(data.columns, ["a", "b"]);
    assert_eq!(data.rows[0][1], Cell::Text("2".into()));
}
