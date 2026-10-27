//! Streaming export of query results to six formats.
//!
//! The encoder enforces the Go budgets: at most 5,000,000 rows, 16,384
//! columns, 1 MiB per field and 8 GiB of output. Rows are written as they
//! arrive so a large result never has to be materialised first.

pub mod budget;
mod csv_sink;
mod text_formats;
mod xlsx_sink;

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde_json::{Map, Value};

use crate::atomic::{self, AtomicTemp};
use crate::cancel::{self, Cancellation};
use crate::dialect::{quote_identifier, sql_literal};
use crate::error::{FileError, Result};
use crate::limits::{MAX_EXPORT_BYTES, MAX_EXPORT_COLUMNS, MAX_EXPORT_ROWS, MAX_FIELD_BYTES};
use crate::value::Cell;

use self::budget::QuotaWriter;
use self::csv_sink::CsvSink;
use self::text_formats::{HTML_FOOTER, HTML_HEADER, html_escape, markdown_header, markdown_row};
use self::xlsx_sink::XlsxSink;

/// Output formats offered by the export dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Excel workbook.
    Xlsx,
    /// Comma separated values.
    Csv,
    /// JSON array of objects.
    Json,
    /// Markdown table.
    Markdown,
    /// Standalone HTML table.
    Html,
    /// INSERT SQL statements.
    InsertSql,
}

impl Format {
    /// Every offered format in dialog order.
    pub const ALL: [Self; 6] = [
        Self::Xlsx,
        Self::Csv,
        Self::Json,
        Self::Markdown,
        Self::Html,
        Self::InsertSql,
    ];

    /// Parses the display name stored in settings.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim() {
            "XLSX" => Some(Self::Xlsx),
            "CSV" => Some(Self::Csv),
            "JSON" => Some(Self::Json),
            "Markdown" => Some(Self::Markdown),
            "HTML" => Some(Self::Html),
            "INSERT SQL" => Some(Self::InsertSql),
            _ => None,
        }
    }

    /// Display name, identical to the Go constants.
    pub fn name(self) -> &'static str {
        match self {
            Self::Xlsx => "XLSX",
            Self::Csv => "CSV",
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
            Self::Html => "HTML",
            Self::InsertSql => "INSERT SQL",
        }
    }

    /// File extension, including the leading dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => ".xlsx",
            Self::Csv => ".csv",
            Self::Json => ".json",
            Self::Markdown => ".md",
            Self::Html => ".html",
            Self::InsertSql => ".sql",
        }
    }
}

/// Options controlling one export.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Output format.
    pub format: Format,
    /// Column indices to export; every column when empty.
    pub columns: Option<Vec<usize>>,
    /// SQL dialect used by the INSERT SQL format.
    pub dialect: String,
    /// Destination table used by the INSERT SQL format.
    pub table: String,
    /// Prefixes CSV output with a UTF-8 byte order mark.
    pub bom: bool,
    /// Single-byte CSV delimiter; a comma when absent.
    pub delimiter: Option<u8>,
}

impl ExportOptions {
    /// Creates options for one format with the default dialect and delimiter.
    pub fn new(format: Format) -> Self {
        Self {
            format,
            columns: None,
            dialect: String::new(),
            table: String::new(),
            bom: false,
            delimiter: None,
        }
    }
}

/// Streaming writer for one export.
pub struct Encoder<'a> {
    writer: QuotaWriter<Box<dyn Write + 'a>>,
    cancel: &'a dyn Cancellation,
    format: Format,
    options: ExportOptions,
    columns: Vec<String>,
    indices: Vec<usize>,
    insert_columns: Vec<String>,
    count: u64,
    started: bool,
    finished: bool,
    csv: Option<CsvSink>,
    xlsx: Option<XlsxSink>,
}

impl<'a> Encoder<'a> {
    /// Wraps an output writer with the export byte budget.
    pub fn new<W: Write + 'a>(
        output: W,
        cancel: &'a dyn Cancellation,
        options: ExportOptions,
    ) -> Result<Self> {
        let format = options.format;
        Ok(Self {
            writer: QuotaWriter::new(
                Box::new(output) as Box<dyn Write + 'a>,
                MAX_EXPORT_BYTES,
                "export exceeds 8 GiB",
            ),
            cancel,
            format,
            options,
            columns: Vec::new(),
            indices: Vec::new(),
            insert_columns: Vec::new(),
            count: 0,
            started: false,
            finished: false,
            csv: None,
            xlsx: None,
        })
    }

    /// Declares the result columns; must be called exactly once before rows.
    pub fn set_columns(&mut self, columns: &[String]) -> Result<()> {
        cancel::check(self.cancel)?;
        if self.started || columns.is_empty() || columns.len() > MAX_EXPORT_COLUMNS {
            return Err(FileError::invalid("invalid or repeated export columns"));
        }
        let selection = self
            .options
            .columns
            .clone()
            .unwrap_or_else(|| (0..columns.len()).collect());
        let mut seen = std::collections::HashSet::with_capacity(selection.len());
        let mut indices = Vec::with_capacity(selection.len());
        for index in selection {
            if index >= columns.len() || !seen.insert(index) {
                return Err(FileError::invalid("invalid export column selection"));
            }
            indices.push(index);
        }
        if indices.is_empty() {
            return Err(FileError::invalid("select at least one export column"));
        }
        self.indices = indices;
        self.columns = self
            .indices
            .iter()
            .map(|index| columns[*index].clone())
            .collect();
        self.started = true;
        self.start()
    }

    /// Writes one row; values are addressed by column index.
    pub fn consume_row_values(&mut self, values: &[Cell]) -> Result<()> {
        cancel::check(self.cancel)?;
        if !self.started || self.finished {
            return Err(FileError::invalid("export encoder is not accepting rows"));
        }
        if self.count >= MAX_EXPORT_ROWS {
            return Err(FileError::invalid("export exceeds 5,000,000 rows"));
        }
        let mut row = Vec::with_capacity(self.indices.len());
        for (column, index) in self.indices.iter().enumerate() {
            let value = values.get(*index).ok_or_else(|| {
                FileError::invalid("export row has fewer fields than its columns")
            })?;
            if value.text_len() > MAX_FIELD_BYTES {
                return Err(FileError::invalid(format!(
                    "column {} exceeds 1 MiB; export stopped without replacing the destination",
                    self.columns[column]
                )));
            }
            row.push(value.clone());
        }
        self.write_row(&row)?;
        self.count += 1;
        Ok(())
    }

    /// Closes the document; no further rows are accepted.
    pub fn finish(&mut self) -> Result<()> {
        cancel::check(self.cancel)?;
        if !self.started || self.finished {
            return Err(FileError::invalid(
                "export has no columns or is already finished",
            ));
        }
        self.finished = true;
        match self.format {
            Format::Csv => self.flush_csv()?,
            Format::Json => self.write_all(b"\n]\n")?,
            Format::Html => self.write_all(HTML_FOOTER.as_bytes())?,
            Format::Xlsx => {
                let sink = self
                    .xlsx
                    .take()
                    .ok_or_else(|| FileError::invalid("XLSX export is not initialised"))?;
                sink.finish(self.writer.inner_mut().as_mut())?;
            }
            Format::Markdown | Format::InsertSql => {}
        }
        self.writer
            .flush()
            .map_err(|error| FileError::io("flush export", error))
    }

    /// Releases format-specific resources without publishing anything.
    pub fn close(&mut self) {
        self.xlsx = None;
    }

    /// Number of rows written so far.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Bytes written so far by this encoder.
    pub fn bytes_written(&self) -> u64 {
        self.writer.written()
    }

    fn start(&mut self) -> Result<()> {
        match self.format {
            Format::Csv => {
                if self.options.bom {
                    self.write_all(b"\xef\xbb\xbf")?;
                }
                let mut sink = CsvSink::new(self.options.delimiter)?;
                let header: Vec<String> = self.columns.iter().map(|name| csv_text(name)).collect();
                sink.write_record(&header)?;
                self.csv = Some(sink);
                self.flush_csv()?;
                Ok(())
            }
            Format::Json => self.write_all(b"[\n"),
            Format::Markdown => {
                let line = markdown_header(&self.columns);
                self.write_all(line.as_bytes())
            }
            Format::Html => {
                self.write_all(HTML_HEADER.as_bytes())?;
                let mut head = String::new();
                for column in &self.columns {
                    head.push_str(&format!("<th>{}</th>", html_escape(column)));
                }
                head.push_str("</tr></thead><tbody>\n");
                self.write_all(head.as_bytes())
            }
            Format::InsertSql => {
                if self.options.table.is_empty() {
                    return Err(FileError::invalid(
                        "INSERT SQL requires a destination table",
                    ));
                }
                let mut quoted = Vec::with_capacity(self.columns.len());
                for column in &self.columns {
                    quoted.push(quote_identifier(&self.options.dialect, column)?);
                }
                self.insert_columns = quoted;
                Ok(())
            }
            Format::Xlsx => {
                self.xlsx = Some(XlsxSink::new(&self.columns)?);
                Ok(())
            }
        }
    }

    fn write_row(&mut self, row: &[Cell]) -> Result<()> {
        match self.format {
            Format::Csv => self.write_csv_row(row),
            Format::Json => self.write_json_row(row),
            Format::Markdown => {
                let values: Vec<String> = row.iter().map(Cell::text).collect();
                let line = markdown_row(&values);
                self.write_all(line.as_bytes())
            }
            Format::Html => self.write_html_row(row),
            Format::InsertSql => self.write_insert_row(row),
            Format::Xlsx => {
                let excel_row = self.count + 2;
                let sink = self
                    .xlsx
                    .as_mut()
                    .ok_or_else(|| FileError::invalid("XLSX export is not initialised"))?;
                sink.write_row(excel_row, row)
            }
        }
    }

    fn write_csv_row(&mut self, row: &[Cell]) -> Result<()> {
        let mut values: Vec<String> = row.iter().map(Cell::text).collect();
        for (index, cell) in row.iter().enumerate() {
            if matches!(cell, Cell::Text(_)) {
                values[index] = csv_text(&values[index]);
            }
        }
        if let Some(sink) = self.csv.as_mut() {
            sink.write_record(&values)?;
        }
        self.flush_csv()
    }

    fn write_json_row(&mut self, row: &[Cell]) -> Result<()> {
        let mut object = Map::new();
        for (index, column) in self.columns.iter().enumerate() {
            if object.contains_key(column) {
                return Err(FileError::invalid(
                    "JSON export requires distinct column names; alias duplicate columns",
                ));
            }
            object.insert(column.clone(), row[index].to_json()?);
        }
        let raw = serde_json::to_string(&Value::Object(object))?;
        if self.count > 0 {
            self.write_all(b",\n")?;
        }
        self.write_all(raw.as_bytes())
    }

    fn write_html_row(&mut self, row: &[Cell]) -> Result<()> {
        let mut line = String::from("<tr>");
        for cell in row {
            let class = if cell.is_null() {
                " class=\"null\""
            } else {
                ""
            };
            line.push_str(&format!("<td{class}>{}</td>", html_escape(&cell.text())));
        }
        line.push_str("</tr>\n");
        self.write_all(line.as_bytes())
    }

    fn write_insert_row(&mut self, row: &[Cell]) -> Result<()> {
        let mut values = Vec::with_capacity(row.len());
        for cell in row {
            values.push(sql_literal(&self.options.dialect, cell)?);
        }
        let statement = format!(
            "INSERT INTO {} ({}) VALUES ({});\n",
            self.options.table,
            self.insert_columns.join(", "),
            values.join(", ")
        );
        self.write_all(statement.as_bytes())
    }

    fn flush_csv(&mut self) -> Result<()> {
        let bytes = match self.csv.as_mut() {
            Some(sink) => sink.drain()?,
            None => return Ok(()),
        };
        if bytes.is_empty() {
            return Ok(());
        }
        self.write_all(&bytes)
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer
            .write_all(bytes)
            .map_err(|error| FileError::io("write export", error))
    }
}

/// Keeps spreadsheet applications from executing exported text as a formula.
pub fn csv_text(value: &str) -> String {
    let trimmed = value.trim_start_matches([' ', '\t', '\r', '\n']);
    let dangerous = value.starts_with(['\t', '\r', '\n'])
        || trimmed
            .chars()
            .next()
            .is_some_and(|first| "=+-@".contains(first));
    if dangerous {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

/// Writes a complete export and only then publishes it at the destination.
///
/// The existing destination is preserved until every row has been written,
/// flushed and synced; a failure removes the temporary file.
pub fn save_atomic<F>(
    cancel: &dyn Cancellation,
    path: impl AsRef<Path>,
    overwrite: bool,
    options: ExportOptions,
    produce: F,
) -> Result<u64>
where
    F: FnOnce(&mut Encoder<'_>) -> Result<()>,
{
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(FileError::invalid("choose an export file"));
    }
    let destination = atomic::absolute(path)?;
    if !overwrite {
        match fs::symlink_metadata(&destination) {
            Ok(_) => return Err(FileError::AlreadyExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(FileError::io(
                    format!("inspect {}", destination.display()),
                    error,
                ));
            }
        }
    }
    cancel::check(cancel)?;
    let directory = atomic::parent_directory(&destination)?;
    let mut temp = AtomicTemp::create(&directory, ".superlink-export-")?;
    let count = {
        let mut encoder = Encoder::new(temp.file_mut()?, cancel, options)?;
        produce(&mut encoder)?;
        encoder.finish()?;
        encoder.count()
    };
    temp.commit(&destination, overwrite)?;
    Ok(count)
}
