//! Export behaviour: formats, budgets and atomic publication.

use std::fs;

use superlink_files::export::{Encoder, ExportOptions, Format, csv_text, save_atomic};
use superlink_files::{Cell, FileError, NeverCancelled};

fn encode(format: Format, text: &str) -> Vec<u8> {
    let mut options = ExportOptions::new(format);
    options.dialect = "postgres".to_string();
    options.table = "\"\"items\"\"".to_string();
    options.columns = Some(vec![2, 0, 1]);
    let mut output = Vec::new();
    {
        let mut encoder = Encoder::new(&mut output, &NeverCancelled, options).unwrap();
        encoder
            .set_columns(&["text".into(), "large".into(), "null".into()])
            .unwrap();
        encoder
            .consume_row_values(&[
                Cell::Text(text.to_string()),
                Cell::Int(9223372036854775807),
                Cell::Null,
            ])
            .unwrap();
        encoder.finish().unwrap();
    }
    output
}

#[test]
fn six_formats_preserve_columns_values_and_neutralize_active_content() {
    let newline = char::from(10);
    let text = format!("=HYPERLINK('x') | <script>{newline}O'Reilly");
    for format in Format::ALL {
        let raw = encode(format, &text);
        assert!(!raw.is_empty(), "{format:?} produced nothing");
        let spelling = String::from_utf8_lossy(&raw);
        match format {
            Format::Csv => {
                assert!(spelling.starts_with("null,"), "{spelling}");
                assert!(
                    spelling.contains("'="),
                    "formula prefix missing: {spelling}"
                );
                assert!(spelling.contains("9223372036854775807"));
            }
            Format::Json => {
                let value: serde_json::Value = serde_json::from_str(&spelling).unwrap();
                let rows = value.as_array().unwrap();
                assert!(rows[0]["null"].is_null());
                assert_eq!(rows[0]["large"], serde_json::json!(9223372036854775807i64));
            }
            Format::Html => {
                assert!(!spelling.contains("<script>"));
                assert!(spelling.contains("&lt;script&gt;"));
            }
            Format::Markdown => {
                assert!(spelling.contains("\\|"));
                assert!(spelling.contains("<br>"));
            }
            Format::InsertSql => {
                assert!(spelling.contains("NULL, E'"), "{spelling}");
                assert!(spelling.contains("O''Reilly"));
                assert!(spelling.contains("9223372036854775807"));
            }
            Format::Xlsx => assert!(raw.starts_with(b"PK")),
        }
    }
}

#[test]
fn failed_export_preserves_destination_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.csv");
    fs::write(&path, "original").unwrap();
    let error = save_atomic(
        &NeverCancelled,
        &path,
        true,
        ExportOptions::new(Format::Csv),
        |encoder| {
            encoder.set_columns(&["a".into()])?;
            Err(FileError::invalid("producer failed"))
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("producer failed"), "{error}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    let entries = fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(entries, 1, "temporary export file leaked");
}

#[test]
fn export_refuses_to_replace_an_existing_file_without_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.csv");
    fs::write(&path, "original").unwrap();
    let error = save_atomic(
        &NeverCancelled,
        &path,
        false,
        ExportOptions::new(Format::Csv),
        |encoder| {
            encoder.set_columns(&["a".into()])?;
            encoder.consume_row_values(&[Cell::Int(1)])?;
            Ok(())
        },
    )
    .unwrap_err();
    assert!(matches!(error, FileError::AlreadyExists));
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
}

#[test]
fn export_rejects_oversized_fields_duplicate_columns_and_bad_selection() {
    let mut output = Vec::new();
    {
        let mut encoder = Encoder::new(
            &mut output,
            &NeverCancelled,
            ExportOptions::new(Format::Csv),
        )
        .unwrap();
        assert!(encoder.set_columns(&[]).is_err());
        encoder.set_columns(&["a".into()]).unwrap();
        assert!(encoder.set_columns(&["b".into()]).is_err());
        let huge = "x".repeat((1 << 20) + 1);
        let error = encoder.consume_row_values(&[Cell::Text(huge)]).unwrap_err();
        assert!(error.to_string().contains("1 MiB"), "{error}");
    }

    let mut output = Vec::new();
    let mut options = ExportOptions::new(Format::Json);
    options.columns = Some(vec![5]);
    let mut encoder = Encoder::new(&mut output, &NeverCancelled, options).unwrap();
    assert!(encoder.set_columns(&["a".into()]).is_err());

    let mut output = Vec::new();
    let mut encoder = Encoder::new(
        &mut output,
        &NeverCancelled,
        ExportOptions::new(Format::Json),
    )
    .unwrap();
    encoder.set_columns(&["a".into(), "a".into()]).unwrap();
    assert!(
        encoder
            .consume_row_values(&[Cell::Int(1), Cell::Int(2)])
            .is_err()
    );

    let mut output = Vec::new();
    let mut encoder = Encoder::new(
        &mut output,
        &NeverCancelled,
        ExportOptions::new(Format::Xlsx),
    )
    .unwrap();
    encoder.set_columns(&["a".into()]).unwrap();
    let long = "y".repeat(32768);
    assert!(encoder.consume_row_values(&[Cell::Text(long)]).is_err());
}

#[test]
fn csv_text_neutralizes_formula_prefixes() {
    assert_eq!(csv_text("=1+1"), "'=1+1");
    assert_eq!(csv_text("plain"), "plain");
    assert_eq!(csv_text(" -1"), "' -1");
    assert_eq!(csv_text("@cmd"), "'@cmd");
}

#[test]
fn export_counts_rows_and_reports_bytes() {
    let mut output = Vec::new();
    let mut encoder = Encoder::new(
        &mut output,
        &NeverCancelled,
        ExportOptions::new(Format::Csv),
    )
    .unwrap();
    encoder.set_columns(&["a".into()]).unwrap();
    for value in 0..10 {
        encoder.consume_row_values(&[Cell::Int(value)]).unwrap();
    }
    assert_eq!(encoder.count(), 10);
    assert!(encoder.bytes_written() > 0);
    encoder.finish().unwrap();
    assert!(encoder.finish().is_err());
}

#[test]
fn cancelled_export_stops_and_leaves_destination_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.csv");
    let flag = superlink_files::CancelFlag::new();
    flag.cancel();
    let error = save_atomic(
        &flag,
        &path,
        true,
        ExportOptions::new(Format::Csv),
        |_encoder| Ok(()),
    )
    .unwrap_err();
    assert!(matches!(error, FileError::Cancelled));
    assert!(!path.exists());
}

#[test]
fn csv_options_add_a_bom_and_a_custom_delimiter() {
    let mut options = ExportOptions::new(Format::Csv);
    options.bom = true;
    options.delimiter = Some(b';');
    let mut output = Vec::new();
    {
        let mut encoder = Encoder::new(&mut output, &NeverCancelled, options).unwrap();
        encoder.set_columns(&["a".into(), "b".into()]).unwrap();
        encoder
            .consume_row_values(&[Cell::Int(1), Cell::Int(2)])
            .unwrap();
        encoder.finish().unwrap();
    }
    assert!(output.starts_with(&[0xef, 0xbb, 0xbf]), "missing BOM");
    let body = String::from_utf8(output[3..].to_vec()).unwrap();
    assert_eq!(body, "a;b\n1;2\n");
}
