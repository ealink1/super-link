//! Index changes for the structured designer.

use std::collections::HashMap;

use crate::error::{SqlError, SqlResult};
use crate::identifier::quote_identifier;
use crate::model::{ColumnDefinition, IndexDefinition, Object, StructureChange};
use crate::structure::qualified_index;

/// Builds one index change and validates it against current metadata.
pub(crate) fn build_index_change(
    kind: &str,
    table: &str,
    object: &Object,
    change: &StructureChange,
    columns: &HashMap<String, ColumnDefinition>,
    existing: &[IndexDefinition],
) -> SqlResult<Vec<String>> {
    if change.kind == "dropIndex" {
        if !existing
            .iter()
            .any(|index| index.name == change.original_name)
        {
            return Err(SqlError::new("original index is no longer present"));
        }
        if change.original_name.eq_ignore_ascii_case("PRIMARY")
            || change.original_name.starts_with("sqlite_autoindex_")
        {
            return Err(SqlError::new(
                "constraint indexes must be changed through explicit DDL",
            ));
        }
        let name = qualified_index(kind, object, &change.original_name)?;
        let mut statement = format!("DROP INDEX {name}");
        if kind == "mysql" {
            statement.push_str(" ON ");
            statement.push_str(table);
        }
        return Ok(vec![statement]);
    }
    let index = &change.index;
    let name = quote_identifier(kind, &index.name)?;
    if existing.iter().any(|old| old.name == index.name) {
        return Err(SqlError::new("duplicate index name"));
    }
    if index.columns.is_empty() || index.columns.len() > 32 {
        return Err(SqlError::new("index requires 1–32 columns"));
    }
    let mut parts: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for column in &index.columns {
        if !columns.contains_key(&column.name) || seen.contains(&column.name) {
            return Err(SqlError::new("unknown or duplicate index column"));
        }
        seen.push(column.name.clone());
        let mut quoted = quote_identifier(kind, &column.name)?;
        if column.descending {
            quoted.push_str(" DESC");
        }
        parts.push(quoted);
    }
    let mut method = index.method.to_uppercase();
    if method.is_empty() {
        method = "BTREE".into();
    }
    let allowed = method == "BTREE"
        || (kind != "sqlite" && method == "HASH")
        || (kind == "postgres" && matches!(method.as_str(), "GIN" | "GIST" | "BRIN" | "SPGIST"));
    if !allowed {
        return Err(SqlError::new("unsupported index method"));
    }
    let unique = if index.unique { "UNIQUE " } else { "" };
    let mut statement = format!("CREATE {unique}INDEX {name} ON {table}");
    if kind == "postgres" {
        statement.push_str(" USING ");
        statement.push_str(&method);
    }
    statement.push_str(" (");
    statement.push_str(&parts.join(", "));
    statement.push(')');
    if kind == "mysql" {
        statement.push_str(" USING ");
        statement.push_str(&method);
    }
    Ok(vec![statement])
}

/// Mirrors the metadata list after a successful change.
pub(crate) fn update_indexes(
    indexes: &[IndexDefinition],
    change: &StructureChange,
) -> Vec<IndexDefinition> {
    if change.kind == "addIndex" {
        let mut next = indexes.to_vec();
        next.push(IndexDefinition {
            name: change.index.name.clone(),
            ..Default::default()
        });
        return next;
    }
    indexes
        .iter()
        .filter(|index| index.name != change.original_name)
        .cloned()
        .collect()
}
