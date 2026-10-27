//! One cell value shared by the importer, the exporter and the dumper.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Number, Value};

use crate::error::{FileError, Result};

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// A single field, mirroring the Go any values drivers produce.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// SQL NULL; rendered as the literal text NULL.
    Null,
    /// Boolean, rendered natively by SQL and XLSX exports.
    Bool(bool),
    /// Signed integer.
    Int(i64),
    /// Unsigned integer kept apart so large counters stay exact.
    UInt(u64),
    /// Floating point number.
    Float(f64),
    /// UTF-8 text.
    Text(String),
    /// Binary payload, rendered as base64 in text formats.
    Bytes(Vec<u8>),
    /// Timestamp, rendered as RFC 3339 with nanoseconds.
    Timestamp(DateTime<Utc>),
    /// Nested JSON value kept verbatim.
    Json(Value),
}

impl Cell {
    /// Renders the cell the way text-oriented formats display it.
    pub fn text(&self) -> String {
        match self {
            Self::Null => "NULL".to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::UInt(value) => value.to_string(),
            Self::Float(value) => format_float(*value),
            Self::Text(value) => value.clone(),
            Self::Bytes(value) => base64_encode(value),
            Self::Timestamp(value) => timestamp_text(value),
            Self::Json(value) => value.to_string(),
        }
    }

    /// Byte length of the rendered text without allocating for plain strings.
    pub fn text_len(&self) -> usize {
        match self {
            Self::Text(value) => value.len(),
            other => other.text().len(),
        }
    }

    /// Returns true when the cell is SQL NULL.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Converts the cell back into a JSON value for the JSON export.
    pub fn to_json(&self) -> Result<Value> {
        Ok(match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(*value),
            Self::Int(value) => Value::Number(Number::from(*value)),
            Self::UInt(value) => Value::Number(Number::from(*value)),
            Self::Float(value) => Number::from_f64(*value).map(Value::Number).ok_or_else(|| {
                FileError::invalid("non-finite number cannot be exported to JSON")
            })?,
            Self::Text(value) => Value::String(value.clone()),
            Self::Bytes(value) => Value::String(base64_encode(value)),
            Self::Timestamp(value) => Value::String(timestamp_text(value)),
            Self::Json(value) => value.clone(),
        })
    }
}

impl From<Value> for Cell {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(flag) => Self::Bool(flag),
            Value::Number(number) => {
                if let Some(value) = number.as_i64() {
                    Self::Int(value)
                } else if let Some(value) = number.as_u64() {
                    Self::UInt(value)
                } else {
                    Self::Float(number.as_f64().unwrap_or(0.0))
                }
            }
            Value::String(text) => Self::Text(text),
            other => Self::Json(other),
        }
    }
}

impl From<&str> for Cell {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<String> for Cell {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<i64> for Cell {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f64> for Cell {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<bool> for Cell {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl<T> From<Option<T>> for Cell
where
    T: Into<Cell>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            Some(inner) => inner.into(),
            None => Self::Null,
        }
    }
}

/// Formats a float the way Go's strconv.FormatFloat(v, 'g', -1, 64) does.
pub fn format_float(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let raw = format!("{value:?}");
    if let Some(index) = raw.find('e') {
        let (mantissa, exponent) = raw.split_at(index);
        let exponent = &exponent[1..];
        let sign = if exponent.starts_with('-') { "" } else { "+" };
        return format!("{mantissa}e{sign}{exponent}");
    }
    match raw.strip_suffix(".0") {
        Some(trimmed) => trimmed.to_string(),
        None => raw,
    }
}

/// RFC 3339 rendering with nanoseconds and trailing zeros trimmed.
pub fn timestamp_text(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// Standard base64 with padding, matching Go's base64.StdEncoding.
pub fn base64_encode(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0] as u32;
        let second = chunk.get(1).copied().unwrap_or(0) as u32;
        let third = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (first << 16) | (second << 8) | third;
        output.push(BASE64[(triple >> 18) as usize & 0x3f] as char);
        output.push(BASE64[(triple >> 12) as usize & 0x3f] as char);
        output.push(if chunk.len() > 1 {
            BASE64[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            BASE64[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    output
}
