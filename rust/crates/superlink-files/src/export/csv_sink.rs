//! CSV record formatting for the export path.

use crate::error::{FileError, Result};

/// Buffers formatted CSV records before they reach the quota writer.
pub(crate) struct CsvSink {
    writer: csv::Writer<Vec<u8>>,
    delimiter: Option<u8>,
}

impl CsvSink {
    /// Creates a sink with an optional single-byte delimiter.
    pub(crate) fn new(delimiter: Option<u8>) -> Result<Self> {
        Ok(Self {
            writer: build(delimiter),
            delimiter,
        })
    }

    /// Writes one record into the internal buffer.
    pub(crate) fn write_record<T: AsRef<[u8]>>(&mut self, record: &[T]) -> Result<()> {
        self.writer
            .write_record(record)
            .map_err(|error| FileError::invalid(format!("format CSV row: {error}")))
    }

    /// Flushes the buffered bytes and starts a new record buffer.
    pub(crate) fn drain(&mut self) -> Result<Vec<u8>> {
        let writer = std::mem::replace(&mut self.writer, build(self.delimiter));
        writer
            .into_inner()
            .map_err(|error| FileError::invalid(format!("format CSV row: {}", error.error())))
    }
}

fn build(delimiter: Option<u8>) -> csv::Writer<Vec<u8>> {
    let mut builder = csv::WriterBuilder::new();
    builder.terminator(csv::Terminator::Any(b'\n'));
    if let Some(delimiter) = delimiter {
        builder.delimiter(delimiter);
    }
    builder.from_writer(Vec::new())
}
