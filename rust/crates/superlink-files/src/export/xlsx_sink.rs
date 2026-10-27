//! XLSX export backed by rust_xlsxwriter in constant-memory mode.

use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};

use crate::error::{FileError, Result};
use crate::limits::{MAX_XLSX_CELL_CHARS, MAX_XLSX_ROWS};
use crate::value::Cell;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Streams rows into a worksheet, spilling them to a temporary workbook file.
pub(crate) struct XlsxSink {
    workbook: Workbook,
    path: PathBuf,
}

impl XlsxSink {
    /// Creates a workbook with a single constant-memory worksheet and header.
    pub(crate) fn new(columns: &[String]) -> Result<Self> {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            ".superlink-xlsx-{}-{sequence}-{stamp}.xlsx",
            std::process::id()
        ));
        let mut workbook = Workbook::new();
        workbook.add_worksheet_with_constant_memory();
        let mut sink = Self { workbook, path };
        let sheet = sink.worksheet()?;
        for (index, name) in columns.iter().enumerate() {
            sheet
                .write_string(0, index as u16, name)
                .map_err(format_error)?;
        }
        Ok(sink)
    }

    /// Writes one data row using its 1-based Excel row number.
    pub(crate) fn write_row(&mut self, excel_row: u64, cells: &[Cell]) -> Result<()> {
        if excel_row > MAX_XLSX_ROWS {
            return Err(FileError::invalid(
                "XLSX exceeds Excel's 1,048,576-row limit; use CSV or JSON",
            ));
        }
        let row = (excel_row - 1) as u32;
        let sheet = self.worksheet()?;
        for (index, cell) in cells.iter().enumerate() {
            let column = index as u16;
            match cell {
                Cell::Null => {}
                Cell::Bool(flag) => {
                    sheet
                        .write_boolean(row, column, *flag)
                        .map_err(format_error)?;
                }
                Cell::Int(value) => {
                    write_number_or_text(sheet, row, column, &value.to_string(), *value as f64)?;
                }
                Cell::UInt(value) => {
                    write_number_or_text(sheet, row, column, &value.to_string(), *value as f64)?;
                }
                Cell::Float(value) if value.is_finite() => {
                    sheet
                        .write_number(row, column, *value)
                        .map_err(format_error)?;
                }
                other => {
                    let text = other.text();
                    if text.chars().count() > MAX_XLSX_CELL_CHARS {
                        return Err(FileError::invalid(
                            "XLSX cell exceeds Excel's 32,767-character limit",
                        ));
                    }
                    sheet
                        .write_string(row, column, &text)
                        .map_err(format_error)?;
                }
            }
        }
        Ok(())
    }

    /// Packages the workbook and streams it into the export writer.
    pub(crate) fn finish(mut self, output: &mut dyn Write) -> Result<()> {
        self.workbook.save(&self.path).map_err(format_error)?;
        let mut file = File::open(&self.path)
            .map_err(|error| FileError::io("open temporary XLSX workbook", error))?;
        io::copy(&mut file, output).map_err(|error| FileError::io("copy XLSX workbook", error))?;
        Ok(())
    }

    fn worksheet(&mut self) -> Result<&mut Worksheet> {
        self.workbook.worksheet_from_index(0).map_err(format_error)
    }
}

impl Drop for XlsxSink {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn write_number_or_text(
    sheet: &mut Worksheet,
    row: u32,
    column: u16,
    text: &str,
    number: f64,
) -> Result<()> {
    if text.trim_start_matches(['+', '-']).len() > 15 {
        sheet
            .write_string(row, column, text)
            .map_err(format_error)?;
        return Ok(());
    }
    sheet
        .write_number(row, column, number)
        .map_err(format_error)?;
    Ok(())
}

fn format_error(error: XlsxError) -> FileError {
    FileError::Xlsx(error.to_string())
}
