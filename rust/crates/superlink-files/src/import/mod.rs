//! Reading bounded CSV, TSV, JSON and XLSX files into a column/row dataset.
//!
//! The parser keeps the Go budgets: a regular file of at most 64 MiB, at most
//! 100,000 rows, 2,000,000 cells and 1 MiB per field. Violations return an
//! explicit error instead of growing memory.

pub mod csv_file;
pub mod decode;
pub mod json_file;
pub mod xlsx_file;

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use crate::cancel::{self, Cancellation};
use crate::error::{FileError, Result};
use crate::limits::{
    MAX_FIELD_BYTES, MAX_IMPORT_BYTES, MAX_IMPORT_CELLS, MAX_IMPORT_COLUMN_NAME,
    MAX_IMPORT_COLUMNS, MAX_IMPORT_ROWS,
};
use crate::value::Cell;

/// Import formats understood by read_file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    /// Comma separated values.
    Csv,
    /// Tab separated values.
    Tsv,
    /// JSON array of objects.
    Json,
    /// Excel workbook.
    Xlsx,
}

impl ImportFormat {
    /// Parses the format name used by the settings UI.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_uppercase().as_str() {
            "CSV" => Some(Self::Csv),
            "TSV" => Some(Self::Tsv),
            "JSON" => Some(Self::Json),
            "XLSX" => Some(Self::Xlsx),
            _ => None,
        }
    }

    /// Detects the format from a file extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        Self::parse(extension)
    }
}

/// Character encoding of a text import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// UTF-8, with an optional byte order mark.
    #[default]
    Utf8,
    /// Simplified Chinese GBK.
    Gbk,
    /// Simplified Chinese GB18030.
    Gb18030,
}

/// Options controlling one import.
#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    /// Explicit format; otherwise the file extension decides.
    pub format: Option<ImportFormat>,
    /// Encoding of CSV and JSON input.
    pub encoding: Encoding,
    /// Worksheet name for XLSX input; the first sheet is used when empty.
    pub sheet: Option<String>,
    /// Field separator for CSV; defaults to a comma or a tab.
    pub delimiter: Option<char>,
    /// Stores empty fields as NULL instead of an empty string.
    pub empty_as_null: bool,
}

/// Columns and rows produced by an import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dataset {
    /// Column names in file order.
    pub columns: Vec<String>,
    /// Data rows; every row has exactly as many cells as there are columns.
    pub rows: Vec<Vec<Cell>>,
}

impl Dataset {
    /// Creates an empty dataset with validated column names.
    pub fn new(columns: Vec<String>) -> Result<Self> {
        validate_columns(&columns)?;
        Ok(Self {
            columns,
            rows: Vec::new(),
        })
    }

    /// Appends one row while enforcing the row, cell and field budgets.
    pub fn append_row(&mut self, row: Vec<Cell>) -> Result<()> {
        if (self.rows.len() + 1) * self.columns.len() > MAX_IMPORT_CELLS {
            return Err(FileError::invalid("import exceeds 2,000,000 cells"));
        }
        if self.rows.len() >= MAX_IMPORT_ROWS {
            return Err(FileError::invalid("import exceeds 100,000 rows"));
        }
        if row.len() != self.columns.len() {
            return Err(FileError::invalid(
                "file row and column counts do not match",
            ));
        }
        for cell in &row {
            if cell.text_len() > MAX_FIELD_BYTES {
                return Err(FileError::invalid("import field exceeds 1 MiB"));
            }
        }
        self.rows.push(row);
        Ok(())
    }
}

/// Checks the column budget and requires distinct non-empty names.
pub fn validate_columns(columns: &[String]) -> Result<()> {
    if columns.is_empty() || columns.len() > MAX_IMPORT_COLUMNS {
        return Err(FileError::invalid("import requires 1-1024 columns"));
    }
    let mut seen = std::collections::HashSet::with_capacity(columns.len());
    for name in columns {
        if name.is_empty() || name.len() > MAX_IMPORT_COLUMN_NAME || !seen.insert(name.as_str()) {
            return Err(FileError::invalid(
                "file columns must have distinct non-empty names",
            ));
        }
    }
    Ok(())
}

/// Reads a bounded data file into memory.
pub fn read_file(
    cancel: &dyn Cancellation,
    path: impl AsRef<Path>,
    options: &ReadOptions,
) -> Result<Dataset> {
    let path = path.as_ref();
    cancel::check(cancel)?;
    let metadata = fs::metadata(path)
        .map_err(|error| FileError::io(format!("open {}", path.display()), error))?;
    if !metadata.is_file() || metadata.len() > MAX_IMPORT_BYTES {
        return Err(FileError::invalid(
            "import requires a regular file no larger than 64 MiB",
        ));
    }
    let format = options
        .format
        .or_else(|| ImportFormat::from_path(path))
        .ok_or_else(|| FileError::invalid("import supports CSV, JSON and XLSX"))?;
    match format {
        ImportFormat::Xlsx => xlsx_file::read(cancel, path, options),
        ImportFormat::Json => {
            let reader = open_text_reader(cancel, path, options)?;
            json_file::read(cancel, reader)
        }
        ImportFormat::Csv | ImportFormat::Tsv => {
            let reader = open_text_reader(cancel, path, options)?;
            csv_file::read(cancel, reader, options, format == ImportFormat::Tsv)
        }
    }
}

fn open_text_reader<'a>(
    cancel: &'a dyn Cancellation,
    path: &Path,
    options: &ReadOptions,
) -> Result<Box<dyn Read + 'a>> {
    let file = File::open(path)
        .map_err(|error| FileError::io(format!("open {}", path.display()), error))?;
    let limited = LimitedReader {
        inner: file,
        remaining: MAX_IMPORT_BYTES + 1,
        cancel,
    };
    match options.encoding {
        Encoding::Utf8 => Ok(Box::new(limited)),
        Encoding::Gbk => Ok(Box::new(decode::DecodingReader::new(
            limited,
            encoding_rs::GBK,
        ))),
        Encoding::Gb18030 => Ok(Box::new(decode::DecodingReader::new(
            limited,
            encoding_rs::GB18030,
        ))),
    }
}

/// Stops reading once the declared import budget is spent.
struct LimitedReader<'a> {
    inner: File,
    remaining: u64,
    cancel: &'a dyn Cancellation,
}

impl Read for LimitedReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "operation cancelled",
            ));
        }
        if self.remaining == 0 {
            return Ok(0);
        }
        let allowed = buffer.len().min(self.remaining as usize);
        let read = self.inner.read(&mut buffer[..allowed])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}
