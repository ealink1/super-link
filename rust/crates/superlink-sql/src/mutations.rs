//! Parameterized row mutations located by the original primary key.

use serde_json::Value;

use crate::error::{SqlError, SqlResult};
use crate::identifier::{object_name, quote_identifier};
use crate::model::{ArgValue, RowChange, Statement, TableChanges, TableInfo};
use crate::value::convert_value;

const MAX_ROWS: usize = 1000;
const MAX_JSON_BYTES: usize = 16 << 20;

/// Validates metadata identities and creates parameterized mutations.
pub fn build_changes(
    kind: &str,
    changes: &TableChanges,
    info: &TableInfo,
) -> SqlResult<Vec<Statement>> {
    if changes.rows.is_empty() || changes.rows.len() > MAX_ROWS {
        return Err(SqlError::new("submit between 1 and 1,000 staged rows"));
    }
    let encoded = serde_json::to_vec(&changes.rows)
        .map_err(|_| SqlError::new("staged changes contain unsupported values"))?;
    if encoded.len() > MAX_JSON_BYTES {
        return Err(SqlError::new(
            "staged changes exceed the 16 MiB budget or contain unsupported values",
        ));
    }
    let name = object_name(kind, &changes.object)?;
    let mut columns: Vec<&str> = Vec::with_capacity(info.columns.len());
    let mut keys: Vec<&str> = Vec::new();
    for column in &info.columns {
        columns.push(&column.name);
        if column.key == "PRI" {
            keys.push(&column.name);
        }
    }
    let mut statements: Vec<Statement> = Vec::with_capacity(changes.rows.len());
    for row in &changes.rows {
        for field in row.values.keys() {
            if !columns.contains(&field.as_str()) {
                return Err(SqlError::new(format!("unknown field {field:?}")));
            }
        }
        for field in row.original.keys() {
            if !columns.contains(&field.as_str()) {
                return Err(SqlError::new(format!("unknown original field {field:?}")));
            }
        }
        let statement = build_row_with_keys(kind, &name, row, info, &row.kind, &keys)?;
        if !statement.sql.is_empty() {
            statements.push(statement);
        }
    }
    if statements.is_empty() {
        return Err(SqlError::new("no changed values to submit"));
    }
    Ok(statements)
}

/// Builds one row statement of any kind (`insert`, `update` or `delete`).
pub fn build_row(
    kind: &str,
    name: &str,
    row: &RowChange,
    info: &TableInfo,
) -> SqlResult<Statement> {
    let keys = primary_keys(info);
    build_row_with_keys(kind, name, row, info, &row.kind, &keys)
}

/// Builds an update regardless of the row's declared kind.
pub fn build_row_update(
    kind: &str,
    name: &str,
    row: &RowChange,
    info: &TableInfo,
) -> SqlResult<Statement> {
    let keys = primary_keys(info);
    build_row_with_keys(kind, name, row, info, "update", &keys)
}

/// Builds a delete regardless of the row's declared kind.
pub fn build_row_delete(
    kind: &str,
    name: &str,
    row: &RowChange,
    info: &TableInfo,
) -> SqlResult<Statement> {
    let keys = primary_keys(info);
    build_row_with_keys(kind, name, row, info, "delete", &keys)
}

/// Builds an insert regardless of the row's declared kind.
pub fn build_row_insert(
    kind: &str,
    name: &str,
    row: &RowChange,
    info: &TableInfo,
) -> SqlResult<Statement> {
    let keys = primary_keys(info);
    build_row_with_keys(kind, name, row, info, "insert", &keys)
}

fn primary_keys(info: &TableInfo) -> Vec<&str> {
    info.columns
        .iter()
        .filter(|column| column.key == "PRI")
        .map(|column| column.name.as_str())
        .collect()
}

fn build_row_with_keys(
    kind: &str,
    name: &str,
    row: &RowChange,
    info: &TableInfo,
    row_kind: &str,
    keys: &[&str],
) -> SqlResult<Statement> {
    let mut statement = Statement {
        require_one: true,
        ..Default::default()
    };
    let mut fields: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();
    let mut sets: Vec<String> = Vec::new();
    for column in &info.columns {
        let Some(value) = row.values.get(&column.name) else {
            continue;
        };
        if !column.writable() {
            return Err(SqlError::new(format!(
                "column {:?} is computed and cannot be assigned",
                column.name
            )));
        }
        if row_kind == "update" {
            let Some(original) = row.original.get(&column.name) else {
                return Err(SqlError::new("original row is incomplete"));
            };
            if original == value {
                continue;
            }
        }
        let quoted = quote_identifier(kind, &column.name).unwrap_or_default();
        fields.push(quoted.clone());
        let position = push_arg(&mut statement, kind, ArgValue::from_json(value));
        values.push(position.clone());
        sets.push(format!("{quoted} = {position}"));
    }
    if row_kind == "insert" {
        if fields.is_empty() {
            return Err(SqlError::new("insert requires an explicit value"));
        }
        statement.sql = format!(
            "INSERT INTO {name} ({}) VALUES ({})",
            fields.join(", "),
            values.join(", ")
        );
        return Ok(statement);
    }
    if row_kind != "update" && row_kind != "delete" {
        return Err(SqlError::new("invalid row change kind"));
    }
    if keys.is_empty() {
        return Err(SqlError::new(
            "editing existing rows requires a primary key",
        ));
    }
    if row_kind == "update" && sets.is_empty() {
        return Ok(Statement::default());
    }
    // Locate by the original primary key, including when the key itself is edited.
    let mut where_parts: Vec<String> = Vec::new();
    for column in &info.columns {
        if column.key != "PRI" {
            continue;
        }
        let Some(original) = row.original.get(&column.name) else {
            return Err(SqlError::new(format!(
                "original primary key field {:?} is missing",
                column.name
            )));
        };
        let value = convert_value(&column.data_type, original).map_err(|error| {
            SqlError::new(format!(
                "original field {:?} cannot be bound: {error}",
                column.name
            ))
        })?;
        let quoted = quote_identifier(kind, &column.name).unwrap_or_default();
        if value == ArgValue::Null {
            where_parts.push(format!("{quoted} IS NULL"));
        } else {
            let position = push_arg(&mut statement, kind, value);
            where_parts.push(format!("{quoted} = {position}"));
        }
    }
    if where_parts.is_empty() {
        return Err(SqlError::new("row locator is empty"));
    }
    statement.sql = if row_kind == "delete" {
        format!("DELETE FROM {name}")
    } else {
        format!("UPDATE {name} SET {}", sets.join(", "))
    };
    statement.sql.push_str(" WHERE ");
    statement.sql.push_str(&where_parts.join(" AND "));
    Ok(statement)
}

fn push_arg(statement: &mut Statement, kind: &str, value: ArgValue) -> String {
    statement.args.push(value);
    placeholder(kind, statement.args.len())
}

/// Binds the Nth argument with the dialect's placeholder syntax.
pub fn placeholder(kind: &str, position: usize) -> String {
    match kind {
        "postgres" | "kingbase" | "highgo" | "vastbase" | "opengauss" | "gaussdb" => {
            format!("{}{position}", "$")
        }
        "oracle" | "dameng" => format!(":{position}"),
        "sqlserver" => format!("@p{position}"),
        _ => "?".to_string(),
    }
}

/// Shows SQL and bound values without executing or persisting them.
pub fn preview_changes(statements: &[Statement]) -> String {
    let mut output = String::new();
    for (index, statement) in statements.iter().enumerate() {
        output.push_str(&format!("-- {}\n{};\n", index + 1, statement.sql));
        let values: Vec<Value> = statement.args.iter().map(ArgValue::to_json).collect();
        let rendered = if values.is_empty() {
            "null".to_string()
        } else {
            serde_json::to_string(&values).unwrap_or_else(|_| "null".to_string())
        };
        output.push_str(&format!("-- parameters: {rendered}\n\n"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ColumnDefinition, Object};
    use serde_json::{Map, Value, json};

    fn test_info() -> TableInfo {
        TableInfo {
            columns: vec![
                ColumnDefinition {
                    name: "id".into(),
                    key: "PRI".into(),
                    data_type: "INTEGER".into(),
                    ..Default::default()
                },
                ColumnDefinition {
                    name: "name".into(),
                    data_type: "TEXT".into(),
                    ..Default::default()
                },
                ColumnDefinition {
                    name: "optional".into(),
                    data_type: "TEXT".into(),
                    nullable: "YES".into(),
                    ..Default::default()
                },
            ],
            indexes: Vec::new(),
        }
    }

    fn map(entries: &[(&str, Value)]) -> Map<String, Value> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn uses_bound_values_and_the_primary_key_locator() {
        let original = map(&[
            ("id", json!(1)),
            ("name", json!("before")),
            ("optional", json!(null)),
        ]);
        let changes = TableChanges {
            object: Object {
                schema: "a.b".into(),
                name: "items".into(),
                ..Default::default()
            },
            rows: vec![
                RowChange {
                    kind: "update".into(),
                    original: original.clone(),
                    values: map(&[("name", json!("'; DELETE FROM items; --"))]),
                },
                RowChange {
                    kind: "delete".into(),
                    original: original.clone(),
                    values: Map::new(),
                },
            ],
            ..Default::default()
        };
        let statements = build_changes("postgres", &changes, &test_info()).unwrap();
        assert_eq!(statements.len(), 2);
        assert_eq!(
            statements[0].sql,
            "UPDATE \"a.b\".\"items\" SET \"name\" = $1 WHERE \"id\" = $2"
        );
        assert_eq!(
            statements[0].args,
            vec![
                ArgValue::Text("'; DELETE FROM items; --".into()),
                ArgValue::Integer(1)
            ]
        );
        assert!(statements[0].require_one);
        assert_eq!(
            statements[1].sql,
            "DELETE FROM \"a.b\".\"items\" WHERE \"id\" = $1"
        );
        assert_eq!(statements[1].args, vec![ArgValue::Integer(1)]);
        assert!(statements[1].require_one);
        assert!(!statements[0].sql.contains("DROP"));
    }

    #[test]
    fn uses_all_original_composite_primary_key_fields() {
        let info = TableInfo {
            columns: vec![
                ColumnDefinition {
                    name: "tenant_id".into(),
                    key: "PRI".into(),
                    data_type: "INT".into(),
                    ..Default::default()
                },
                ColumnDefinition {
                    name: "name".into(),
                    data_type: "VARCHAR(255)".into(),
                    ..Default::default()
                },
                ColumnDefinition {
                    name: "id".into(),
                    key: "PRI".into(),
                    data_type: "BIGINT".into(),
                    ..Default::default()
                },
                ColumnDefinition {
                    name: "updated_at".into(),
                    data_type: "DATETIME".into(),
                    ..Default::default()
                },
            ],
            indexes: Vec::new(),
        };
        let original = map(&[
            ("tenant_id", json!("7")),
            ("id", json!("26")),
            ("name", json!("before")),
            ("updated_at", json!("2026-06-11T10:00:18+08:00")),
        ]);
        let changes = TableChanges {
            object: Object {
                scope: "catalog".into(),
                name: "items".into(),
                ..Default::default()
            },
            rows: vec![
                RowChange {
                    kind: "update".into(),
                    original: original.clone(),
                    values: map(&[("id", json!(27)), ("name", json!("after"))]),
                },
                RowChange {
                    kind: "delete".into(),
                    original: original.clone(),
                    values: Map::new(),
                },
            ],
            ..Default::default()
        };
        let statements = build_changes("mysql", &changes, &info).unwrap();
        assert_eq!(
            statements[0].sql,
            "UPDATE \x60catalog\x60.\x60items\x60 SET \x60name\x60 = ?, \x60id\x60 = ? WHERE \x60tenant_id\x60 = ? AND \x60id\x60 = ?"
        );
        assert_eq!(
            statements[0].args,
            vec![
                ArgValue::Text("after".into()),
                ArgValue::Integer(27),
                ArgValue::Integer(7),
                ArgValue::Integer(26)
            ]
        );
        assert_eq!(
            statements[1].sql,
            "DELETE FROM \x60catalog\x60.\x60items\x60 WHERE \x60tenant_id\x60 = ? AND \x60id\x60 = ?"
        );
        assert_eq!(
            statements[1].args,
            vec![ArgValue::Integer(7), ArgValue::Integer(26)]
        );
        for missing in ["tenant_id", "id"] {
            let mut incomplete = original.clone();
            incomplete.remove(missing);
            for row_kind in ["update", "delete"] {
                let broken = TableChanges {
                    object: changes.object.clone(),
                    rows: vec![RowChange {
                        kind: row_kind.into(),
                        original: incomplete.clone(),
                        values: map(&[("name", json!("after"))]),
                    }],
                    ..Default::default()
                };
                assert!(
                    build_changes("mysql", &broken, &info).is_err(),
                    "{row_kind} without {missing}"
                );
            }
        }
    }

    #[test]
    fn rejects_unknown_fields_missing_locators_and_no_ops() {
        let info = test_info();
        let base = TableChanges {
            object: Object {
                name: "items".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        for row in [
            RowChange {
                kind: "update".into(),
                values: map(&[("name", json!("x"))]),
                ..Default::default()
            },
            RowChange {
                kind: "insert".into(),
                values: map(&[("unknown", json!("x"))]),
                ..Default::default()
            },
            RowChange {
                kind: "insert".into(),
                ..Default::default()
            },
            RowChange {
                kind: "drop".into(),
                ..Default::default()
            },
        ] {
            let mut changes = base.clone();
            changes.rows = vec![row.clone()];
            assert!(build_changes("sqlite", &changes, &info).is_err(), "{row:?}");
        }
        let mut keyless = test_info();
        keyless.columns[0].key = String::new();
        let mut changes = base.clone();
        changes.rows = vec![RowChange {
            kind: "delete".into(),
            original: map(&[
                ("id", json!(1)),
                ("name", json!("x")),
                ("optional", json!(null)),
            ]),
            ..Default::default()
        }];
        assert!(build_changes("sqlite", &changes, &keyless).is_err());
        let mut insert = base.clone();
        insert.rows = vec![RowChange {
            kind: "insert".into(),
            values: map(&[("name", json!("x"))]),
            ..Default::default()
        }];
        let statements = build_changes("sqlite", &insert, &keyless).unwrap();
        assert_eq!(
            statements[0].sql,
            "INSERT INTO \"items\" (\"name\") VALUES (?)"
        );
    }

    #[test]
    fn previews_statements_with_parameters() {
        let statement = Statement {
            sql: "UPDATE t SET a = ?".into(),
            args: vec![ArgValue::Text("x".into()), ArgValue::Bytes(b"fo".to_vec())],
            require_one: true,
        };
        assert_eq!(
            preview_changes(&[statement]),
            "-- 1\nUPDATE t SET a = ?;\n-- parameters: [\"x\",\"Zm8=\"]\n\n"
        );
        assert!(preview_changes(&[]).is_empty());
    }
}
