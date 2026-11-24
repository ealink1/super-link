//! A whole-field designer draft, independent of UI widgets and database I/O.
use crate::identifier::{ddl_text_literal, object_name, quote_identifier, structure_dialect};
use crate::structure_column::column_definition;
use crate::{
    ColumnDefinition, Object, SqlError, SqlResult, StructureChange, StructureRequest, TableInfo,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignSnapshot {
    pub columns: Vec<ColumnDefinition>,
    pub comment: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignField {
    pub original: Option<String>,
    pub column: ColumnDefinition,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignDraft {
    pub fields: Vec<DesignField>,
    pub comment: String,
}
impl From<&DesignSnapshot> for DesignDraft {
    fn from(snapshot: &DesignSnapshot) -> Self {
        Self {
            fields: snapshot
                .columns
                .iter()
                .map(|column| DesignField {
                    original: Some(column.name.clone()),
                    column: column.clone(),
                })
                .collect(),
            comment: snapshot.comment.clone(),
        }
    }
}

pub fn design_sql(
    kind: &str,
    object: &Object,
    before: &DesignSnapshot,
    draft: &DesignDraft,
) -> SqlResult<Vec<String>> {
    validate(draft, before)?;
    let statements = if structure_dialect(kind) == "mysql" {
        mysql_sql(kind, object, before, draft)?
    } else {
        portable_sql(kind, object, before, draft)?
    };
    if statements.join("\n").len() > 1 << 20 {
        return Err(SqlError::new("结构变更 SQL 超过 1 MiB"));
    }
    Ok(statements)
}
fn validate(draft: &DesignDraft, before: &DesignSnapshot) -> SqlResult<()> {
    if draft.fields.is_empty() || draft.fields.len() > 4096 {
        return Err(SqlError::new("表必须包含 1–4096 个字段"));
    }
    let mut names = std::collections::HashSet::new();
    let mut originals = std::collections::HashSet::new();
    for field in &draft.fields {
        if field.column.name.trim().is_empty() || !names.insert(field.column.name.to_lowercase()) {
            return Err(SqlError::new("字段名为空或重复"));
        }
        if let Some(original) = &field.original
            && (!originals.insert(original.clone())
                || !before.columns.iter().any(|c| c.name == *original))
        {
            return Err(SqlError::new("字段原始身份无效，请重新读取结构"));
        }
    }
    Ok(())
}
fn mysql_sql(
    kind: &str,
    object: &Object,
    before: &DesignSnapshot,
    draft: &DesignDraft,
) -> SqlResult<Vec<String>> {
    let table = object_name(kind, object)?;
    let primary_before: Vec<_> = before
        .columns
        .iter()
        .filter(|c| c.key == "PRI")
        .map(|c| &c.name)
        .collect();
    let primary_after: Vec<_> = draft
        .fields
        .iter()
        .filter(|f| f.column.key == "PRI")
        .map(|f| &f.column.name)
        .collect();
    // Renaming/reordering a field must preserve the existing composite key order.
    // MySQL CHANGE COLUMN already updates references to renamed key fields.
    let primary_changed = primary_before.len() != primary_after.len()
        || draft
            .fields
            .iter()
            .filter(|f| f.column.key == "PRI")
            .any(|f| {
                !before
                    .columns
                    .iter()
                    .any(|c| Some(&c.name) == f.original.as_ref() && c.key == "PRI")
            });
    let auto: Vec<_> = draft
        .fields
        .iter()
        .filter(|f| f.column.extra.to_lowercase().contains("auto_increment"))
        .collect();
    if auto.len() > 1
        || auto.iter().any(|f| {
            f.column.key != "PRI" || f.column.has_default || !integer_type(&f.column.data_type)
        })
    {
        return Err(SqlError::new(
            "自增字段必须为整数主键、无默认值，且只能有一个",
        ));
    }
    let mut clauses = Vec::new();
    if primary_changed && !primary_before.is_empty() {
        clauses.push("DROP PRIMARY KEY".into());
    }
    for old in &before.columns {
        if !draft
            .fields
            .iter()
            .any(|f| f.original.as_ref() == Some(&old.name))
        {
            clauses.push(format!(
                "DROP COLUMN {}",
                quote_identifier(kind, &old.name)?
            ));
        }
    }
    for (position, field) in draft.fields.iter().enumerate() {
        let old = field
            .original
            .as_ref()
            .and_then(|name| before.columns.iter().find(|c| &c.name == name));
        let old_position = field
            .original
            .as_ref()
            .and_then(|name| before.columns.iter().position(|c| &c.name == name));
        if old == Some(&field.column) && old_position == Some(position) {
            continue;
        }
        let mut column = field.column.clone();
        if column.key == "PRI" {
            column.nullable = "NO".into();
        }
        let definition = column_definition(
            "mysql",
            &column,
            old.unwrap_or(&ColumnDefinition::default()),
        )?;
        let placement = if position == 0 {
            "FIRST".into()
        } else {
            format!(
                "AFTER {}",
                quote_identifier(kind, &draft.fields[position - 1].column.name)?
            )
        };
        clauses.push(match old {
            Some(old) => format!(
                "CHANGE COLUMN {} {definition} {placement}",
                quote_identifier(kind, &old.name)?
            ),
            None => format!("ADD COLUMN {definition} {placement}"),
        });
    }
    if primary_changed && !primary_after.is_empty() {
        clauses.push(format!(
            "ADD PRIMARY KEY ({})",
            primary_after
                .iter()
                .map(|n| quote_identifier(kind, n))
                .collect::<SqlResult<Vec<_>>>()?
                .join(", ")
        ));
    }
    if draft.comment != before.comment {
        clauses.push(format!(
            "COMMENT = {}",
            ddl_text_literal(kind, &draft.comment)
        ));
    }
    if clauses.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![format!(
        "ALTER TABLE {table}\n  {}",
        clauses.join(",\n  ")
    )])
}
fn integer_type(data_type: &str) -> bool {
    matches!(
        data_type
            .split(['(', ' '])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "bigint"
    )
}
fn portable_sql(
    kind: &str,
    object: &Object,
    before: &DesignSnapshot,
    draft: &DesignDraft,
) -> SqlResult<Vec<String>> {
    let remaining: Vec<_> = before
        .columns
        .iter()
        .filter(|old| {
            draft
                .fields
                .iter()
                .any(|f| f.original.as_ref() == Some(&old.name))
        })
        .map(|c| c.name.as_str())
        .collect();
    let desired: Vec<_> = draft
        .fields
        .iter()
        .filter_map(|f| f.original.as_deref())
        .collect();
    if remaining != desired
        || draft
            .fields
            .iter()
            .position(|f| f.original.is_none())
            .is_some_and(|first| draft.fields[first..].iter().any(|f| f.original.is_some()))
    {
        return Err(SqlError::new("该数据库不支持直接调整字段顺序；草稿已保留"));
    }
    let mut changes = Vec::new();
    for old in &before.columns {
        if !draft
            .fields
            .iter()
            .any(|f| f.original.as_ref() == Some(&old.name))
        {
            changes.push(StructureChange {
                kind: "dropColumn".into(),
                original_name: old.name.clone(),
                ..Default::default()
            });
        }
    }
    for field in &draft.fields {
        let old = field
            .original
            .as_ref()
            .and_then(|name| before.columns.iter().find(|c| &c.name == name));
        if old == Some(&field.column) {
            continue;
        }
        changes.push(StructureChange {
            kind: if old.is_some() {
                "alterColumn"
            } else {
                "addColumn"
            }
            .into(),
            original_name: field.original.clone().unwrap_or_default(),
            column: field.column.clone(),
            ..Default::default()
        });
    }
    let mut statements = if changes.is_empty() {
        Vec::new()
    } else {
        crate::build_structure_changes(
            kind,
            &StructureRequest {
                object: object.clone(),
                changes,
                ..Default::default()
            },
            &TableInfo {
                columns: before.columns.clone(),
                ..Default::default()
            },
        )?
    };
    if before.comment != draft.comment {
        if structure_dialect(kind) != "postgres" {
            return Err(SqlError::new("该数据库不支持表备注"));
        }
        statements.push(format!(
            "COMMENT ON TABLE {} IS {}",
            object_name(kind, object)?,
            ddl_text_literal(kind, &draft.comment)
        ));
    }
    Ok(statements)
}

#[cfg(test)]
#[path = "design_tests.rs"]
mod tests;
