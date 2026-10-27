//! Byte budget enforced on every export writer.

use std::io::{self, Write};

/// Wraps a writer and fails once the declared budget is spent.
pub(crate) struct QuotaWriter<W> {
    inner: W,
    remaining: u64,
    limit: u64,
    message: &'static str,
}

impl<W> QuotaWriter<W> {
    /// Creates a writer that accepts at most limit bytes.
    pub(crate) fn new(inner: W, limit: u64, message: &'static str) -> Self {
        Self {
            inner,
            remaining: limit,
            limit,
            message,
        }
    }

    /// Bytes written so far.
    pub(crate) fn written(&self) -> u64 {
        self.limit - self.remaining
    }

    /// Borrows the wrapped writer.
    pub(crate) fn inner_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}

impl<W: Write> Write for QuotaWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.len() as u64 > self.remaining {
            return Err(io::Error::other(self.message));
        }
        let written = self.inner.write(buffer)?;
        self.remaining -= written as u64;
        if written < buffer.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "short write while exporting",
            ));
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
