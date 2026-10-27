//! Structured table changes: validated column and index DDL.

use std::collections::HashMap;

use crate::error::{SqlError, SqlResult};
use crate::identifier::{object_name, quote_identifier, structure_dialect, text_literal};
use crate::model::{ColumnDefinition, Object, StructureChange, StructureRequest, TableInfo};
use crate::structure_column::{column_definition, default_expression, validate_column_type};
use crate::structure_index::{build_index_change, update_indexes};

/// Validates structured changes; raw SQL stays in the query workflow.
pub fn build_structure_changes(
    dialect: &str,
    request: &StructureRequest,
    info: &TableInfo,
) -> SqlResult<Vec<String>> {
    let kind = structure_dialect(dialect);
    if kind.is_empty() {
        return Err(SqlError::new(
            "this driver does not support the structured designer",
        ));
    }
    if request.object.kind == "view" {
        return Err(SqlError::new("view definitions must be edited through DDL"));
    }
    if request.changes.is_empty() || request.changes.len() > 100 {
        return Err(SqlError::new("structure changes require 1–100 operations"));
    }
    let table = object_name(dialect, &request.object)?;
    let mut columns: HashMap<String, ColumnDefinition> = info
        .columns
        .iter()
        .map(|column| (column.name.clone(), column.clone()))
        .collect();
    let mut statements: Vec<String> = Vec::new();
    let mut indexes = info.indexes.clone();
    for change in &request.changes {
        let next = match change.kind.as_str() {
            "addColumn" | "alterColumn" | "dropColumn" => {
                build_column_change(kind, &table, change, &mut columns)?
            }
            "addIndex" | "dropIndex" => {
                let next =
                    build_index_change(kind, &table, &request.object, change, &columns, &indexes)?;
                indexes = update_indexes(&indexes, change);
                next
            }
            _ => return Err(SqlError::new("unsupported structure operation")),
        };
        statements.extend(next);
    }
    if statements.is_empty() {
        return Err(SqlError::new("no structure changes"));
    }
    if statements.join("\n").len() > 1 << 20 {
        return Err(SqlError::new("structure SQL exceeds 1 MiB"));
    }
    Ok(statements)
}

fn build_column_change(
    kind: &str,
    table: &str,
    change: &StructureChange,
    columns: &mut HashMap<String, ColumnDefinition>,
) -> SqlResult<Vec<String>> {
    let existing = columns.get(&change.original_name).cloned();
    if change.kind != "addColumn" && existing.is_none() {
        return Err(SqlError::new("original column is no longer present"));
    }
    let old = existing.unwrap_or_default();
    let quoted_original = quote_identifier(kind, &change.original_name);
    if change.kind == "dropColumn" {
        if old.key == "PRI" {
            return Err(SqlError::new(
                "remove the primary-key constraint before dropping its column",
            ));
        }
        let old_name = quoted_original?;
        columns.remove(&old.name);
        return Ok(vec![format!("ALTER TABLE {table} DROP COLUMN {old_name}")]);
    }
    let old_name = quoted_original.unwrap_or_default();
    let column = &change.column;
    if column.nullable != "YES" && column.nullable != "NO" {
        return Err(SqlError::new("invalid column nullability"));
    }
    if kind == "sqlite" && !column.comment.is_empty() {
        return Err(SqlError::new("SQLite column comments require explicit DDL"));
    }
    let name = quote_identifier(kind, &column.name)?;
    if columns.contains_key(&column.name) && (change.kind == "addColumn" || column.name != old.name)
    {
        return Err(SqlError::new("duplicate column name"));
    }
    if change.kind == "addColumn" {
        if !column.key.is_empty() || !column.extra.is_empty() {
            return Err(SqlError::new(
                "new primary or identity columns require explicit DDL",
            ));
        }
        let definition = column_definition(kind, column, &ColumnDefinition::default())?;
        columns.insert(column.name.clone(), column.clone());
        let mut statements = vec![format!("ALTER TABLE {table} ADD COLUMN {definition}")];
        if kind == "postgres" && !column.comment.is_empty() {
            statements.push(format!(
                "COMMENT ON COLUMN {table}.{name} IS {}",
                text_literal(kind, &column.comment)
            ));
        }
        return Ok(statements);
    }
    if column.key != old.key || column.extra != old.extra {
        return Err(SqlError::new(
            "primary-key and identity changes require explicit DDL",
        ));
    }
    if kind != "mysql" && (column.collation != old.collation || column.charset != old.charset) {
        return Err(SqlError::new(
            "charset and collation changes require explicit DDL",
        ));
    }
    let mut statements: Vec<String> = Vec::new();
    match kind {
        "mysql" => {
            let definition = column_definition(kind, column, &old)?;
            statements.push(format!(
                "ALTER TABLE {table} CHANGE COLUMN {old_name} {definition}"
            ));
        }
        "postgres" => {
            if column.data_type != old.data_type {
                validate_column_type(&column.data_type)?;
                statements.push(format!(
                    "ALTER TABLE {table} ALTER COLUMN {old_name} TYPE {}",
                    column.data_type
                ));
            }
            if column.nullable != old.nullable {
                let verb = if column.nullable == "YES" {
                    "DROP NOT NULL"
                } else {
                    "SET NOT NULL"
                };
                statements.push(format!(
                    "ALTER TABLE {table} ALTER COLUMN {old_name} {verb}"
                ));
            }
            if column.default != old.default || column.has_default != old.has_default {
                let mut verb = "DROP DEFAULT".to_string();
                if column.has_default {
                    if let Some(value) = &column.default {
                        verb = format!("SET DEFAULT {}", default_expression(kind, value)?);
                    }
                }
                statements.push(format!(
                    "ALTER TABLE {table} ALTER COLUMN {old_name} {verb}"
                ));
            }
            if column.comment != old.comment {
                statements.push(format!(
                    "COMMENT ON COLUMN {table}.{old_name} IS {}",
                    text_literal(kind, &column.comment)
                ));
            }
        }
        "sqlite" => {
            let mut copy = column.clone();
            copy.name = old.name.clone();
            if copy != old {
                return Err(SqlError::new(
                    "SQLite type, default and constraint edits require rebuilding the table; use explicit DDL",
                ));
            }
        }
        _ => {}
    }
    if kind != "mysql" && column.name != old.name {
        statements.push(format!(
            "ALTER TABLE {table} RENAME COLUMN {old_name} TO {name}"
        ));
    }
    columns.remove(&old.name);
    columns.insert(column.name.clone(), column.clone());
    Ok(statements)
}

/// Joins statements the way the preview panel shows them.
pub fn preview_structure(statements: &[String]) -> String {
    format!("{};", statements.join(";\n\n"))
}

/// Index names are schema-qualified on PostgreSQL and bare elsewhere.
pub fn qualified_index(kind: &str, object: &Object, name: &str) -> SqlResult<String> {
    let quoted = quote_identifier(kind, name)?;
    if kind == "postgres" && !object.schema.is_empty() {
        let schema = quote_identifier(kind, &object.schema)?;
        return Ok(format!("{schema}.{quoted}"));
    }
    Ok(quoted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{IndexColumn, IndexDefinition, NewIndex};

    fn fixture() -> TableInfo {
        TableInfo {
            columns: vec![
                ColumnDefinition {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    key: "PRI".into(),
                    nullable: "NO".into(),
                    ..Default::default()
                },
                ColumnDefinition {
                    name: "name".into(),
                    data_type: "TEXT".into(),
                    nullable: "YES".into(),
                    ..Default::default()
                },
            ],
            indexes: Vec::new(),
        }
    }

    fn rename_and_index() -> StructureRequest {
        StructureRequest {
            object: Object {
                name: "order.items".into(),
                schema: "tenant".into(),
                ..Default::default()
            },
            changes: vec![
                StructureChange {
                    kind: "alterColumn".into(),
                    original_name: "name".into(),
                    column: ColumnDefinition {
                        name: "renamed.name".into(),
                        data_type: "TEXT".into(),
                        nullable: "YES".into(),
                        ..Default::default()
                    },
                    index: NewIndex::default(),
                },
                StructureChange {
                    kind: "addIndex".into(),
                    original_name: String::new(),
                    column: ColumnDefinition::default(),
                    index: NewIndex {
                        name: "ix.name".into(),
                        method: "BTREE".into(),
                        columns: vec![IndexColumn {
                            name: "renamed.name".into(),
                            descending: true,
                        }],
                        unique: false,
                    },
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn quotes_names_and_uses_dialect_index_grammar() {
        for kind in ["mysql", "postgres", "sqlite"] {
            let statements =
                build_structure_changes(kind, &rename_and_index(), &fixture()).unwrap();
            let joined = statements.join("\n");
            assert!(joined.contains("renamed.name"), "{joined}");
            assert!(
                !joined.contains("ON \x60tenant\x60.\x60order.items\x60 USING BTREE"),
                "{joined}"
            );
            if kind == "mysql" {
                assert!(statements[1].ends_with("USING BTREE"), "{joined}");
            }
            if kind == "postgres" {
                assert!(statements[1].contains("USING BTREE ("), "{joined}");
            }
        }
    }

    #[test]
    fn rejects_stale_columns_injected_types_and_unsupported_edits() {
        let invalid = vec![
            StructureChange {
                kind: "dropColumn".into(),
                original_name: "missing".into(),
                ..Default::default()
            },
            StructureChange {
                kind: "dropColumn".into(),
                original_name: "id".into(),
                ..Default::default()
            },
            StructureChange {
                kind: "addColumn".into(),
                column: ColumnDefinition {
                    name: "a".into(),
                    data_type: "TEXT); DROP TABLE items; --".into(),
                    nullable: "YES".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            StructureChange {
                kind: "addColumn".into(),
                column: ColumnDefinition {
                    name: "name".into(),
                    data_type: "TEXT".into(),
                    nullable: "YES".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            StructureChange {
                kind: "addIndex".into(),
                index: NewIndex {
                    name: "ix".into(),
                    columns: vec![IndexColumn {
                        name: "missing".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            },
        ];
        for change in invalid {
            let request = StructureRequest {
                object: Object {
                    name: "items".into(),
                    ..Default::default()
                },
                changes: vec![change.clone()],
                ..Default::default()
            };
            assert!(
                build_structure_changes("postgres", &request, &fixture()).is_err(),
                "{change:?}"
            );
        }
        let request = StructureRequest {
            object: Object {
                name: "items".into(),
                ..Default::default()
            },
            changes: vec![StructureChange {
                kind: "alterColumn".into(),
                original_name: "name".into(),
                column: ColumnDefinition {
                    name: "name".into(),
                    data_type: "INTEGER".into(),
                    nullable: "YES".into(),
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(build_structure_changes("sqlite", &request, &fixture()).is_err());
    }

    #[test]
    fn adds_and_drops_indexes_against_metadata() {
        let mut info = fixture();
        info.indexes = vec![IndexDefinition {
            name: "old".into(),
            ..Default::default()
        }];
        let request = StructureRequest {
            object: Object {
                name: "items".into(),
                schema: "public".into(),
                ..Default::default()
            },
            changes: vec![StructureChange {
                kind: "dropIndex".into(),
                original_name: "old".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(
            build_structure_changes("postgres", &request, &info).unwrap(),
            vec!["DROP INDEX \"public\".\"old\"".to_string()]
        );
        let mut stale = request.clone();
        stale.changes[0].original_name = "PRIMARY".into();
        info.indexes[0].name = "PRIMARY".into();
        assert!(build_structure_changes("postgres", &stale, &info).is_err());
        assert_eq!(preview_structure(&["a".into(), "b".into()]), "a;\n\nb;");
    }
}
