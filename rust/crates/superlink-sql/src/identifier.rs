//! Identifier quoting, literal escaping and metadata paths.

use crate::error::{SqlError, SqlResult};
use crate::model::Object;

/// Dialects that quote with backticks and address tables through a scope.
const BACKTICK_SCOPE: &[&str] = &[
    "mysql",
    "goldendb",
    "mariadb",
    "oceanbase",
    "diros",
    "starrocks",
    "sphinx",
];

const BACKTICK_QUOTE: &[&str] = &[
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

const HEX_LITERAL: &[&str] = &[
    "mysql",
    "goldendb",
    "mariadb",
    "oceanbase",
    "diros",
    "starrocks",
    "sphinx",
];

const ESCAPE_STRING: &[&str] = &[
    "postgres",
    "kingbase",
    "highgo",
    "vastbase",
    "opengauss",
    "gaussdb",
];

/// Maps a driver key onto the dialect family whose DDL grammar we emit.
pub fn structure_dialect(key: &str) -> &'static str {
    match key {
        "mysql" | "mariadb" | "goldendb" | "oceanbase" => "mysql",
        "postgres" | "kingbase" | "highgo" | "vastbase" | "opengauss" | "gaussdb" => "postgres",
        "sqlite" => "sqlite",
        _ => "",
    }
}

/// Quotes one literal metadata identifier; dots are never treated as syntax.
pub fn quote_identifier(kind: &str, name: &str) -> SqlResult<String> {
    if name.is_empty() || name.contains('\0') {
        return Err(SqlError::new("invalid empty identifier"));
    }
    if BACKTICK_QUOTE.contains(&kind) {
        return Ok(format!("\x60{}\x60", name.replace('\x60', "\x60\x60")));
    }
    if kind == "sqlserver" {
        return Ok(format!("[{}]", name.replace(']', "]]")));
    }
    Ok(format!("\"{}\"", name.replace('"', "\"\"")))
}

/// Quotes the canonical components returned by metadata.
pub fn object_name(kind: &str, object: &Object) -> SqlResult<String> {
    let parts: Vec<&str> = if !object.schema.is_empty() {
        vec![object.schema.as_str(), object.name.as_str()]
    } else if !object.scope.is_empty() && BACKTICK_SCOPE.contains(&kind) {
        vec![object.scope.as_str(), object.name.as_str()]
    } else {
        vec![object.name.as_str()]
    };
    let mut quoted = Vec::with_capacity(parts.len());
    for part in parts {
        quoted.push(quote_identifier(kind, part)?);
    }
    Ok(quoted.join("."))
}

/// Escapes a structured filter value so it cannot leave its SQL literal.
pub fn text_literal(kind: &str, value: &str) -> String {
    if HEX_LITERAL.contains(&kind) {
        return format!("CONVERT(X'{}' USING utf8mb4)", hex_encode(value.as_bytes()));
    }
    if ESCAPE_STRING.contains(&kind) {
        return format!("E'{}'", value.replace('\\', "\\\\").replace('\'', "''"));
    }
    format!("'{}'", value.replace('\'', "''"))
}

/// Decodes the quoted PostgreSQL metadata path without splitting literal dots.
pub fn parse_path(path: &str) -> SqlResult<Vec<String>> {
    let bytes = path.as_bytes();
    let mut parts: Vec<String> = Vec::with_capacity(2);
    let mut part: Vec<u8> = Vec::new();
    let mut quoted = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            if quoted && index + 1 < bytes.len() && bytes[index + 1] == b'"' {
                part.push(b'"');
                index += 2;
                continue;
            }
            quoted = !quoted;
            index += 1;
            continue;
        }
        if byte == b'.' && !quoted {
            if part.is_empty() {
                return Err(SqlError::new("empty metadata path component"));
            }
            parts.push(decode_part(&part)?);
            part.clear();
            index += 1;
            continue;
        }
        part.push(byte);
        index += 1;
    }
    if quoted || part.is_empty() {
        return Err(SqlError::new("invalid quoted metadata path"));
    }
    parts.push(decode_part(&part)?);
    if parts.len() > 2 {
        return Err(SqlError::new("metadata path has too many components"));
    }
    Ok(parts)
}

fn decode_part(bytes: &[u8]) -> SqlResult<String> {
    String::from_utf8(bytes.to_vec()).map_err(|_| SqlError::new("invalid metadata path encoding"))
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_each_dialect_family() {
        assert_eq!(
            quote_identifier("mysql", "a\x60b").unwrap(),
            "\x60a\x60\x60b\x60"
        );
        assert_eq!(quote_identifier("postgres", "a\"b").unwrap(), "\"a\"\"b\"");
        assert_eq!(quote_identifier("sqlserver", "a]b").unwrap(), "[a]]b]");
        assert_eq!(quote_identifier("sqlite", "plain").unwrap(), "\"plain\"");
        assert!(quote_identifier("mysql", "").is_err());
        assert!(quote_identifier("mysql", "a\0b").is_err());
    }

    #[test]
    fn object_name_prefers_schema_then_scope() {
        let object = Object {
            name: "order.details".into(),
            scope: "demo".into(),
            ..Default::default()
        };
        assert_eq!(
            object_name("mysql", &object).unwrap(),
            "\x60demo\x60.\x60order.details\x60"
        );
        assert_eq!(
            object_name("postgres", &object).unwrap(),
            "\"order.details\""
        );
        let schema = Object {
            schema: "a.b".into(),
            name: "c".into(),
            scope: "ignored".into(),
            ..Default::default()
        };
        assert_eq!(
            object_name("mysql", &schema).unwrap(),
            "\x60a.b\x60.\x60c\x60"
        );
    }

    #[test]
    fn literals_stay_inside_their_quotes() {
        assert_eq!(
            text_literal("mysql", "x'; DROP"),
            "CONVERT(X'78273b2044524f50' USING utf8mb4)"
        );
        assert_eq!(text_literal("postgres", "a\\b'c"), "E'a\\\\b''c'");
        assert_eq!(text_literal("sqlite", "it's"), "'it''s'");
    }

    #[test]
    fn parses_quoted_metadata_paths() {
        assert_eq!(parse_path("public.items").unwrap(), vec!["public", "items"]);
        assert_eq!(
            parse_path("\"a.b\".\"c\"\"d\"").unwrap(),
            vec!["a.b", "c\"d"]
        );
        assert_eq!(parse_path("\"a.b\"").unwrap(), vec!["a.b"]);
        for invalid in ["", "a..b", "\"a", "a.b.c"] {
            assert!(parse_path(invalid).is_err(), "{invalid}");
        }
    }
}
