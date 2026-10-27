//! Streaming CSV and TSV import.

use std::io::Read;

use crate::cancel::{Cancellation, check};
use crate::error::{FileError, Result};
use crate::import::{Dataset, ReadOptions};
use crate::value::Cell;

/// Reads one CSV or TSV document; the first record is the header.
pub(crate) fn read(
    cancel: &dyn Cancellation,
    reader: impl Read,
    options: &ReadOptions,
    tsv: bool,
) -> Result<Dataset> {
    let mut builder = csv::ReaderBuilder::new();
    builder.has_headers(false).flexible(true);
    if let Some(delimiter) = options.delimiter {
        let byte = u8::try_from(delimiter)
            .map_err(|_| FileError::invalid("delimiter must be a single-byte character"))?;
        builder.delimiter(byte);
    } else if tsv {
        builder.delimiter(b'\t');
    }
    let mut reader = builder.from_reader(reader);
    let mut record = csv::StringRecord::new();

    match reader.read_record(&mut record) {
        Ok(true) => {}
        Ok(false) => {
            return Err(FileError::invalid("read CSV headers: file is empty"));
        }
        Err(error) => {
            return Err(FileError::invalid(format!(
                "read CSV headers: {}",
                describe(&error)
            )));
        }
    }

    let mut columns: Vec<String> = record.iter().map(str::to_string).collect();
    if let Some(first) = columns.first_mut() {
        *first = first.trim_start_matches('\u{feff}').to_string();
    }
    let mut data = Dataset::new(columns)?;

    loop {
        check(cancel)?;
        match reader.read_record(&mut record) {
            Ok(false) => break,
            Ok(true) => {
                let mut row = Vec::with_capacity(record.len());
                for field in record.iter() {
                    if options.empty_as_null && field.is_empty() {
                        row.push(Cell::Null);
                    } else {
                        row.push(Cell::Text(field.to_string()));
                    }
                }
                data.append_row(row)?;
            }
            Err(error) => {
                return Err(FileError::invalid(format!(
                    "CSV row {}: {}",
                    data.rows.len() + 2,
                    describe(&error)
                )));
            }
        }
    }
    Ok(data)
}

fn describe(error: &csv::Error) -> String {
    if matches!(error.kind(), csv::ErrorKind::Utf8 { .. }) {
        return "file is not valid UTF-8; choose the correct encoding".to_string();
    }
    error.to_string()
}
