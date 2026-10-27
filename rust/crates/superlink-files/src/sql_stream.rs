//! Streaming statement splitter for SQL documents.
//!
//! Statements are emitted one at a time, so memory is proportional to a single
//! statement rather than to the whole file. Strings, dollar-quoted bodies,
//! quoted identifiers and comments are respected for each supported dialect.

use std::io::{self, BufRead, BufReader, Read};

use sha2::{Digest, Sha256};

use crate::cancel::{Cancellation, check};
use crate::error::{FileError, Result};

const INVALID_TEXT: &str = "SQL 文件必须是 UTF-8 文本，且不能包含 NUL 字符";
const UNCLOSED: &str = "SQL 文件包含未闭合的字符串或注释";
const EXECUTABLE_COMMENT: &str = "暂不支持 MySQL 可执行注释，请使用普通 SQL 语句";
const UNCLOSED_STRING: &str = "SQL 字符串未闭合";

/// Lexer behaviour derived from the SQL dialect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanOptions {
    /// Treats a leading hash as a line comment (MySQL family).
    pub hash_comments: bool,
    /// Treats a backslash inside a string as an escape (MySQL family).
    pub backslash_escapes: bool,
    /// Recognises dollar-quoted strings (PostgreSQL family).
    pub dollar_quotes: bool,
}

/// Maps a dialect name onto the lexer options it needs.
pub fn options_for_dialect(dialect: &str) -> ScanOptions {
    let normalized = normalize_dialect(dialect);
    let mut options = ScanOptions {
        backslash_escapes: matches!(
            normalized.as_str(),
            "mysql"
                | "mariadb"
                | "oceanbase"
                | "diros"
                | "starrocks"
                | "goldendb"
                | "sphinx"
                | "tidb"
        ),
        ..ScanOptions::default()
    };
    if normalized.is_empty() {
        options.hash_comments = true;
        options.dollar_quotes = true;
        return options;
    }
    match normalized.as_str() {
        "mysql" | "mariadb" | "oceanbase" | "diros" | "starrocks" | "goldendb" | "sphinx"
        | "tidb" | "clickhouse" => options.hash_comments = true,
        "postgres" | "opengauss" | "gaussdb" | "kingbase" | "highgo" | "vastbase" => {
            options.dollar_quotes = true;
        }
        _ => {}
    }
    options
}

fn normalize_dialect(dialect: &str) -> String {
    let normalized = dialect.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "postgresql" | "pg" | "pq" | "pgx" => "postgres".to_string(),
        "doris" => "diros".to_string(),
        "open_gauss" | "open-gauss" => "opengauss".to_string(),
        "gauss_db" | "gauss-db" => "gaussdb".to_string(),
        "kingbase8" | "kingbasees" | "kingbasev8" => "kingbase".to_string(),
        "greatdb" | "gdb" => "goldendb".to_string(),
        _ => normalized,
    }
}

/// Emits every statement found in the input and returns the SHA-256 of the file.
pub fn statements<F>(
    cancel: &dyn Cancellation,
    input: impl Read,
    dialect: &str,
    mut use_statement: F,
) -> Result<String>
where
    F: FnMut(&str, u64) -> Result<()>,
{
    let options = options_for_dialect(dialect);
    let sqlserver = normalize_dialect(dialect) == "sqlserver";
    let mysql = options.hash_comments;
    let mut source = Source::new(input);
    let mut state = LexState::default();
    let mut previous = 0u8;

    loop {
        if source.offset & 4095 == 0 {
            check(cancel)?;
        }
        let Some(byte) = source.next_byte()? else {
            break;
        };
        let mut look = [0u8; 2];
        let look_len = source.peek(&mut look)?;
        let next = &look[..look_len];

        if state.consume(byte, next, &mut source)? {
            continue;
        }

        if byte == b'-' && next.first() == Some(&b'-') {
            let mut two = [0u8; 2];
            let two_len = source.peek(&mut two)?;
            if !mysql || two_len < 2 || two[1] <= b' ' {
                source.next_byte()?;
                state.line_comment = true;
                state.statement.push(b' ');
                continue;
            }
        }
        if mysql && byte == b'#' {
            state.line_comment = true;
            state.statement.push(b' ');
            continue;
        }
        if byte == b'/' && next.first() == Some(&b'*') {
            let mut two = [0u8; 2];
            let two_len = source.peek(&mut two)?;
            if two_len > 1 && two[1] == b'!' {
                return Err(FileError::invalid(EXECUTABLE_COMMENT));
            }
            source.next_byte()?;
            state.depth = 1;
            state.statement.push(b' ');
            continue;
        }
        if byte == b';' {
            state.emit(source.offset, &mut use_statement)?;
            continue;
        }
        if byte == b'\'' || byte == b'"' || byte == b'\x60' {
            state.quote = byte;
            state.escaped_quote = options.backslash_escapes
                || (byte == b'\''
                    && options.dollar_quotes
                    && (previous == b'e' || previous == b'E'));
        }
        if byte == b'[' && sqlserver {
            state.quote = b']';
        }
        if byte == b'$' && options.dollar_quotes {
            state.open_dollar(&mut source)?;
            if !state.dollar.is_empty() {
                continue;
            }
        }
        state.statement.push(byte);
        previous = byte;
    }
    state.finish(source.offset, &mut use_statement)?;
    Ok(hex::encode(source.hasher.finalize()))
}

/// Buffered reader that hashes every consumed byte.
struct Source<R: Read> {
    reader: BufReader<R>,
    hasher: Sha256,
    offset: u64,
}

impl<R: Read> Source<R> {
    fn new(input: R) -> Self {
        Self {
            reader: BufReader::with_capacity(64 << 10, input),
            hasher: Sha256::new(),
            offset: 0,
        }
    }

    fn next_byte(&mut self) -> io::Result<Option<u8>> {
        let buffer = self.reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(None);
        }
        let byte = buffer[0];
        self.reader.consume(1);
        self.hasher.update([byte]);
        self.offset += 1;
        Ok(Some(byte))
    }

    fn peek(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let buffer = self.reader.fill_buf()?;
        let length = buffer.len().min(output.len());
        output[..length].copy_from_slice(&buffer[..length]);
        Ok(length)
    }
}

#[derive(Default)]
struct LexState {
    statement: Vec<u8>,
    quote: u8,
    dollar: String,
    depth: usize,
    line_comment: bool,
    escaped_quote: bool,
}

impl LexState {
    fn consume<R: Read>(&mut self, byte: u8, next: &[u8], source: &mut Source<R>) -> Result<bool> {
        if self.line_comment {
            if byte == b'\n' {
                self.line_comment = false;
                self.statement.push(b' ');
            }
            return Ok(true);
        }
        if self.depth > 0 {
            if byte == b'/' && next.first() == Some(&b'*') {
                source.next_byte()?;
                self.depth += 1;
            }
            if byte == b'*' && next.first() == Some(&b'/') {
                source.next_byte()?;
                self.depth -= 1;
                if self.depth == 0 {
                    self.statement.push(b' ');
                }
            }
            return Ok(true);
        }
        if !self.dollar.is_empty() {
            self.statement.push(byte);
            if byte == b'$' {
                let tail: Vec<u8> = self.dollar.as_bytes()[1..].to_vec();
                let mut peeked = vec![0u8; tail.len()];
                let length = source.peek(&mut peeked)?;
                if length == tail.len() && peeked == tail {
                    for _ in 0..tail.len() {
                        if let Some(consumed) = source.next_byte()? {
                            self.statement.push(consumed);
                        }
                    }
                    self.dollar.clear();
                }
            }
            return Ok(true);
        }
        if self.quote != 0 {
            self.statement.push(byte);
            if byte == b'\\' && self.escaped_quote && self.quote != b'\x60' && self.quote != b']' {
                return match source.next_byte()? {
                    Some(escaped) => {
                        self.statement.push(escaped);
                        Ok(true)
                    }
                    None => Err(FileError::invalid(UNCLOSED_STRING)),
                };
            }
            if byte == self.quote {
                if next.first() == Some(&self.quote) {
                    if let Some(consumed) = source.next_byte()? {
                        self.statement.push(consumed);
                    }
                } else {
                    self.quote = 0;
                }
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn open_dollar<R: Read>(&mut self, source: &mut Source<R>) -> Result<()> {
        let mut size = 1usize;
        loop {
            let mut peeked = vec![0u8; size];
            let length = source.peek(&mut peeked)?;
            if length < size {
                return Ok(());
            }
            let character = peeked[size - 1];
            if character == b'$' {
                let tag = String::from_utf8_lossy(&peeked).into_owned();
                self.dollar = String::from("$");
                self.dollar.push_str(&tag);
                let opened = self.dollar.clone();
                self.statement.extend_from_slice(opened.as_bytes());
                for _ in 0..size {
                    source.next_byte()?;
                }
                return Ok(());
            }
            let allowed = character == b'_'
                || character.is_ascii_lowercase()
                || character.is_ascii_uppercase()
                || (size > 1 && character.is_ascii_digit());
            if !allowed {
                return Ok(());
            }
            size += 1;
        }
    }

    fn emit<F>(&mut self, offset: u64, use_statement: &mut F) -> Result<()>
    where
        F: FnMut(&str, u64) -> Result<()>,
    {
        let raw = std::mem::take(&mut self.statement);
        let text = String::from_utf8(raw).map_err(|_| FileError::invalid(INVALID_TEXT))?;
        let text = text
            .strip_prefix('\u{feff}')
            .unwrap_or(&text)
            .trim()
            .to_string();
        if text.is_empty() {
            return Ok(());
        }
        if text.contains('\0') {
            return Err(FileError::invalid(INVALID_TEXT));
        }
        use_statement(&text, offset)
    }

    fn finish<F>(&mut self, offset: u64, use_statement: &mut F) -> Result<()>
    where
        F: FnMut(&str, u64) -> Result<()>,
    {
        if self.quote != 0 || !self.dollar.is_empty() || self.depth != 0 {
            return Err(FileError::invalid(UNCLOSED));
        }
        self.emit(offset, use_statement)
    }
}
