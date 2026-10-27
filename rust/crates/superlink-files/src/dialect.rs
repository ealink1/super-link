//! Dialect-aware identifier quoting and SQL literals.
//!
//! Mirrors internal/sqlworkbench.Quote, ObjectName and TextLiteral plus
//! datafile.SQLLiteral so the INSERT SQL export stays byte compatible.

use superlink_domain::Object;

use crate::error::{FileError, Result};
use crate::value::Cell;

/// Dialects that address tables through their database scope.
const MYSQL_SCOPE: &[&str] = &[
    "mysql",
    "goldendb",
    "mariadb",
    "oceanbase",
    "diros",
    "starrocks",
    "sphinx",
];

/// Dialects that quote identifiers with backticks.
const MYSQL_QUOTE: &[&str] = &[
    "mysql",
    "goldendb",
    "mariadb",
    "oceanbase",
    "diros",
    "starrocks",
    "sphinx",
    "clickhouse",
    "tdengine",
    "iotdb",
];

/// Dialects with a native BOOLEAN literal.
const NATIVE_BOOLEAN: &[&str] = &[
    "postgres",
    "kingbase",
    "highgo",
    "vastbase",
    "opengauss",
    "gaussdb",
    "duckdb",
    "trino",
];

/// Dialects that decode hex through decode(..., 'hex').
const BINARY_DECODE: &[&str] = &[
    "postgres",
    "kingbase",
    "highgo",
    "vastbase",
    "opengauss",
    "gaussdb",
];

/// Dialects that decode hex through HEXTORAW.
const BINARY_HEXTORAW: &[&str] = &["oracle", "dameng"];

/// Lowercases a dialect name and folds the common driver aliases.
pub fn normalize(dialect: &str) -> String {
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

fn is_mysql_quote(dialect: &str) -> bool {
    MYSQL_QUOTE.contains(&dialect)
}

fn is_mysql_scope(dialect: &str) -> bool {
    MYSQL_SCOPE.contains(&dialect)
}

/// Quotes one metadata identifier; dots inside the name are never treated as SQL.
pub fn quote_identifier(dialect: &str, name: &str) -> Result<String> {
    if name.is_empty() || name.contains('\0') {
        return Err(FileError::invalid("invalid empty identifier"));
    }
    let dialect = normalize(dialect);
    if is_mysql_quote(&dialect) {
        let doubled = name.replace('\u{60}', "\u{60}\u{60}");
        return Ok(format!("\u{60}{doubled}\u{60}"));
    }
    if dialect == "sqlserver" {
        return Ok(format!("[{}]", name.replace(']', "]]")));
    }
    let doubled = name.replace('"', "\"\"");
    Ok(format!("\"{doubled}\""))
}

/// Quotes every component of a canonical object name returned by metadata.
pub fn object_name(dialect: &str, object: &Object) -> Result<String> {
    let dialect = normalize(dialect);
    let mut parts: Vec<&str> = vec![object.name.as_str()];
    if !object.schema.is_empty() {
        parts = vec![object.schema.as_str(), object.name.as_str()];
    } else if !object.scope.is_empty() && is_mysql_scope(&dialect) {
        parts = vec![object.scope.as_str(), object.name.as_str()];
    }
    let mut quoted = Vec::with_capacity(parts.len());
    for part in parts {
        quoted.push(quote_identifier(&dialect, part)?);
    }
    Ok(quoted.join("."))
}

/// Quotes a structured text value without relying on backslash escaping.
pub fn text_literal(dialect: &str, value: &str) -> String {
    let dialect = normalize(dialect);
    if MYSQL_SCOPE.contains(&dialect.as_str()) {
        return format!(
            "CONVERT(X'{}' USING utf8mb4)",
            hex::encode(value.as_bytes())
        );
    }
    if BINARY_DECODE.contains(&dialect.as_str()) {
        return format!("E'{}'", value.replace('\\', "\\\\").replace('\'', "''"));
    }
    format!("'{}'", value.replace('\'', "''"))
}

/// Renders one cell as a SQL literal for the given dialect.
pub fn sql_literal(dialect: &str, cell: &Cell) -> Result<String> {
    let dialect = normalize(dialect);
    Ok(match cell {
        Cell::Null => "NULL".to_string(),
        Cell::Bool(flag) => {
            if NATIVE_BOOLEAN.contains(&dialect.as_str()) {
                if *flag { "TRUE" } else { "FALSE" }.to_string()
            } else if *flag {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        Cell::Int(value) => value.to_string(),
        Cell::UInt(value) => value.to_string(),
        Cell::Float(value) => {
            if !value.is_finite() {
                return Err(FileError::invalid(
                    "non-finite number cannot be exported to SQL",
                ));
            }
            crate::value::format_float(*value)
        }
        Cell::Bytes(bytes) => {
            let encoded = hex::encode(bytes);
            if BINARY_DECODE.contains(&dialect.as_str()) {
                format!("decode('{encoded}', 'hex')")
            } else if BINARY_HEXTORAW.contains(&dialect.as_str()) {
                format!("HEXTORAW('{encoded}')")
            } else if dialect == "sqlserver" {
                format!("0x{encoded}")
            } else {
                format!("X'{encoded}'")
            }
        }
        other => text_literal(&dialect, &other.text()),
    })
}
