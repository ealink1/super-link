//! Streaming GBK and GB18030 decoding.
//!
//! Invalid byte sequences are rejected instead of being replaced, matching the
//! Go UTF-8 validator that asks the user to choose the correct encoding.

use std::io::{self, Read};

use encoding_rs::{Decoder, DecoderResult, Encoding};

/// Decodes a byte stream into UTF-8 without ever buffering the whole file.
pub struct DecodingReader<R> {
    inner: R,
    decoder: Decoder,
    pending: Vec<u8>,
    offset: usize,
    finished: bool,
}

impl<R: Read> DecodingReader<R> {
    /// Creates a reader that converts the given source encoding to UTF-8.
    pub fn new(inner: R, encoding: &'static Encoding) -> Self {
        Self {
            inner,
            decoder: encoding.new_decoder_without_bom_handling(),
            pending: Vec::new(),
            offset: 0,
            finished: false,
        }
    }
}

impl<R: Read> Read for DecodingReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            if self.offset < self.pending.len() {
                let take = (self.pending.len() - self.offset).min(output.len());
                output[..take].copy_from_slice(&self.pending[self.offset..self.offset + take]);
                self.offset += take;
                if self.offset == self.pending.len() {
                    self.pending.clear();
                    self.offset = 0;
                }
                return Ok(take);
            }
            if self.finished {
                return Ok(0);
            }
            let mut chunk = [0u8; 8192];
            let read = self.inner.read(&mut chunk)?;
            let last = read == 0;
            self.pending.clear();
            self.offset = 0;
            self.pending.resize(read.saturating_mul(4) + 16, 0);
            let (result, _, written) = self.decoder.decode_to_utf8_without_replacement(
                &chunk[..read],
                &mut self.pending,
                last,
            );
            if let DecoderResult::Malformed(_, _) = result {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "file is not valid UTF-8; choose the correct encoding",
                ));
            }
            self.pending.truncate(written);
            if last {
                self.finished = true;
                if written == 0 {
                    return Ok(0);
                }
            }
        }
    }
}
