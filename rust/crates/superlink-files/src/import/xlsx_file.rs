//! XLSX import backed by calamine.
//!
//! Only the first used range of one worksheet is read; every cell is rendered
//! as text so numbers keep their spreadsheet spelling instead of being coerced
//! into a floating point value.

use std::path::Path;

use calamine::{Data, Reader, open_workbook_auto};

use crate::cancel::{Cancellation, check};
use crate::error::{FileError, Result};
use crate::import::{Dataset, ReadOptions};
use crate::value::{Cell, format_float};

/// Reads one worksheet into a dataset.
pub(crate) fn read(
    cancel: &dyn Cancellation,
    path: &Path,
    options: &ReadOptions,
) -> Result<Dataset> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|error| FileError::invalid(format!("open XLSX: {error}")))?;
    let names = workbook.sheet_names().to_vec();
    let sheet = match options.sheet.as_deref() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => names
            .first()
            .cloned()
            .ok_or_else(|| FileError::invalid("XLSX has no worksheets"))?,
    };
    let range = workbook
        .worksheet_range(&sheet)
        .map_err(|error| FileError::invalid(format!("read worksheet {sheet}: {error}")))?;
    let mut rows = range.rows();
    let header = rows
        .next()
        .ok_or_else(|| FileError::invalid("XLSX worksheet has no header"))?;
    let columns: Vec<String> = header
        .iter()
        .map(|value| cell_text(value).unwrap_or_default())
        .collect();
    let mut data = Dataset::new(columns)?;
    for row in rows {
        check(cancel)?;
        if row.len() > data.columns.len() {
            return Err(FileError::invalid("XLSX row has fields beyond its header"));
        }
        let mut values = Vec::with_capacity(data.columns.len());
        for index in 0..data.columns.len() {
            let text = row.get(index).and_then(cell_text).unwrap_or_default();
            if options.empty_as_null && text.is_empty() {
                values.push(Cell::Null);
            } else {
                values.push(Cell::Text(text));
            }
        }
        data.append_row(values)?;
    }
    Ok(data)
}

/// Renders one spreadsheet cell the way a raw cell value would appear.
fn cell_text(value: &Data) -> Option<String> {
    match value {
        Data::Empty => None,
        Data::String(text) => Some(text.clone()),
        Data::Int(number) => Some(number.to_string()),
        Data::Float(number) => Some(format_float(*number)),
        Data::Bool(flag) => Some(if *flag { "1" } else { "0" }.to_string()),
        Data::DateTime(stamp) => Some(format_float(stamp.as_f64())),
        Data::DateTimeIso(text) | Data::DurationIso(text) => Some(text.clone()),
        Data::Error(error) => Some(error.to_string()),
    }
}
