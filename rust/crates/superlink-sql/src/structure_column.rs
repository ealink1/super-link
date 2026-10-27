//! Column type validation and definition assembly for DDL generation.

use crate::error::{SqlError, SqlResult};
use crate::identifier::{quote_identifier, text_literal};
use crate::model::ColumnDefinition;

const TYPE_MODIFIERS: &[&str] = &["varying", "precision", "with", "without", "time", "zone"];

/// Rejects expressions and enum definitions that would smuggle SQL into DDL.
pub(crate) fn validate_column_type(value: &str) -> SqlResult<()> {
    if value.len() > 128 || !valid_type_text(value) {
        return Err(SqlError::new(
            "unsupported column type; use explicit DDL for expressions or enum definitions",
        ));
    }
    Ok(())
}

fn valid_type_text(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    match bytes.first() {
        Some(first) if first.is_ascii_alphabetic() || *first == b'_' => index += 1,
        _ => return false,
    }
    while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
        index += 1;
    }
    loop {
        let saved = index;
        let mut probe = index;
        while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if probe == index {
            break;
        }
        let word_start = probe;
        while probe < bytes.len() && (bytes[probe].is_ascii_alphanumeric() || bytes[probe] == b'_')
        {
            probe += 1;
        }
        let word = &value[word_start..probe];
        let boundary = probe == bytes.len()
            || bytes[probe].is_ascii_whitespace()
            || bytes[probe] == b'('
            || bytes[probe] == b'[';
        if boundary
            && TYPE_MODIFIERS
                .iter()
                .any(|item| word.eq_ignore_ascii_case(item))
        {
            index = probe;
        } else {
            index = saved;
            break;
        }
    }
    if index < bytes.len() && bytes[index] == b'(' {
        index += 1;
        if !consume_digits(bytes, &mut index) {
            return false;
        }
        let mut probe = index;
        while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if probe < bytes.len() && bytes[probe] == b',' {
            let mut second = probe + 1;
            while second < bytes.len() && bytes[second].is_ascii_whitespace() {
                second += 1;
            }
            index = second;
            if !consume_digits(bytes, &mut index) {
                return false;
            }
        }
        if index >= bytes.len() || bytes[index] != b')' {
            return false;
        }
        index += 1;
    }
    if index < bytes.len() {
        let saved = index;
        let mut probe = index;
        while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if probe > index {
            let word_start = probe;
            while probe < bytes.len() && bytes[probe].is_ascii_alphanumeric() {
                probe += 1;
            }
            if probe > word_start && value[word_start..probe].eq_ignore_ascii_case("unsigned") {
                index = probe;
            } else {
                index = saved;
            }
        }
    }
    if index + 1 < bytes.len() && bytes[index] == b'[' && bytes[index + 1] == b']' {
        index += 2;
    }
    index == bytes.len()
}

fn consume_digits(bytes: &[u8], index: &mut usize) -> bool {
    let start = *index;
    while *index < bytes.len() && bytes[*index].is_ascii_digit() {
        *index += 1;
    }
    *index > start
}

fn is_simple_sql_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Builds one column definition; `old` preserves unchanged defaults verbatim.
pub(crate) fn column_definition(
    kind: &str,
    column: &ColumnDefinition,
    old: &ColumnDefinition,
) -> SqlResult<String> {
    let name = quote_identifier(kind, &column.name)?;
    validate_column_type(&column.data_type)?;
    let mut definition = format!("{name} {}", column.data_type);
    if kind == "mysql" {
        let extra = column.extra.to_lowercase().replace("default_generated", "");
        let extra = extra.trim();
        if !extra.is_empty() && extra != "auto_increment" {
            return Err(SqlError::new(
                "generated or ON UPDATE columns require explicit DDL",
            ));
        }
        for (keyword, value) in [
            ("CHARACTER SET", column.charset.as_str()),
            ("COLLATE", column.collation.as_str()),
        ] {
            if !value.is_empty() {
                if !is_simple_sql_name(value) {
                    return Err(SqlError::new("invalid charset or collation"));
                }
                definition.push(' ');
                definition.push_str(keyword);
                definition.push(' ');
                definition.push_str(value);
            }
        }
    }
    if column.nullable != "YES" && column.nullable != "NO" {
        return Err(SqlError::new("column nullability must be YES or NO"));
    }
    if column.nullable == "NO" {
        definition.push_str(" NOT NULL");
    }
    if column.has_default {
        if let Some(value) = &column.default {
            let value = if old.name.is_empty()
                || column.default != old.default
                || column.has_default != old.has_default
            {
                default_expression(kind, value)?
            } else {
                value.clone()
            };
            definition.push_str(" DEFAULT ");
            definition.push_str(&value);
        }
    }
    if kind == "mysql" {
        if column.extra.to_lowercase().contains("auto_increment") {
            definition.push_str(" AUTO_INCREMENT");
        }
        if !column.comment.is_empty() {
            definition.push_str(" COMMENT ");
            definition.push_str(&text_literal(kind, &column.comment));
        }
    }
    Ok(definition)
}

/// Accepts only literals whose meaning cannot change when replayed.
pub(crate) fn default_expression(kind: &str, value: &str) -> SqlResult<String> {
    let value = value.trim();
    let upper = value.to_uppercase();
    match upper.as_str() {
        "NULL"
        | "TRUE"
        | "FALSE"
        | "CURRENT_TIMESTAMP"
        | "CURRENT_TIMESTAMP()"
        | "CURRENT_DATE"
        | "CURRENT_TIME" => return Ok(upper),
        _ => {}
    }
    if let Some(first) = value.chars().next() {
        if (first == '-' || first.is_ascii_digit()) && is_json_number(value) {
            return Ok(value.to_string());
        }
    }
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'' {
        let inner = &value[1..value.len() - 1];
        let inner_bytes = inner.as_bytes();
        let mut index = 0;
        while index < inner_bytes.len() {
            if inner_bytes[index] == b'\'' {
                if index + 1 >= inner_bytes.len() || inner_bytes[index + 1] != b'\'' {
                    return Err(SqlError::new("invalid default string"));
                }
                index += 2;
            } else {
                index += 1;
            }
        }
        return Ok(text_literal(kind, &inner.replace("''", "'")));
    }
    Err(SqlError::new(
        "default requires a number, quoted string, NULL or a supported timestamp expression",
    ))
}

/// Accepts exactly the JSON number grammar, like the Go `json.Valid` guard.
fn is_json_number(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    if bytes.first() == Some(&b'-') {
        index += 1;
    }
    if index >= bytes.len() {
        return false;
    }
    if bytes[index] == b'0' {
        index += 1;
    } else if bytes[index].is_ascii_digit() {
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
    } else {
        return false;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        index += 1;
        if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    index == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_declared_types_and_rejects_expressions() {
        for valid in [
            "INTEGER",
            "character varying(64)",
            "decimal(10, 2)",
            "timestamp with time zone",
            "BIGINT UNSIGNED",
            "TEXT[]",
            "int(11) unsigned",
        ] {
            assert!(validate_column_type(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "",
            "TEXT); DROP TABLE items; --",
            "int(10 )",
            "int unsigned extra",
            "时间",
            "int(10,2,3)",
        ] {
            assert!(validate_column_type(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn builds_mysql_definitions_with_attributes() {
        let column = ColumnDefinition {
            name: "order".into(),
            data_type: "INT".into(),
            nullable: "NO".into(),
            extra: "auto_increment".into(),
            comment: "it's".into(),
            charset: "utf8mb4".into(),
            collation: "utf8mb4_bin".into(),
            ..Default::default()
        };
        let definition = column_definition("mysql", &column, &ColumnDefinition::default()).unwrap();
        assert_eq!(
            definition,
            "\x60order\x60 INT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL AUTO_INCREMENT COMMENT CONVERT(X'69742773' USING utf8mb4)"
        );
        // PostgreSQL ignores MySQL-only attributes instead of guessing.
        assert_eq!(
            column_definition("postgres", &column, &ColumnDefinition::default()).unwrap(),
            "\"order\" INT NOT NULL"
        );
    }

    #[test]
    fn keeps_unchanged_defaults_verbatim() {
        let old = ColumnDefinition {
            name: "v".into(),
            data_type: "TEXT".into(),
            nullable: "YES".into(),
            default: Some("'a'::text".into()),
            has_default: true,
            ..Default::default()
        };
        let same = old.clone();
        let definition = column_definition("postgres", &same, &old).unwrap();
        assert_eq!(definition, "\"v\" TEXT DEFAULT 'a'::text");
        let mut changed = old.clone();
        changed.default = Some("'a'::text".into());
        changed.nullable = "NO".into();
        assert!(column_definition("postgres", &changed, &old).is_ok());
    }

    #[test]
    fn validates_default_expressions() {
        for valid in ["0", "-1.5", "1e5", "NULL", "current_timestamp()"] {
            assert!(default_expression("postgres", valid).is_ok(), "{valid}");
        }
        assert_eq!(
            default_expression("postgres", "'it''s'").unwrap(),
            "E'it''s'"
        );
        for invalid in [
            "0); DROP TABLE items; --",
            "NOW(); DELETE FROM items",
            "'unclosed",
            "01",
        ] {
            assert!(
                default_expression("postgres", invalid).is_err(),
                "{invalid}"
            );
        }
    }
}
