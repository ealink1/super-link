//! Dialect-aware table paging and export queries.

use std::collections::HashSet;

use crate::error::{SqlError, SqlResult};
use crate::identifier::{object_name, quote_identifier, text_literal};
use crate::model::{TableInfo, TableRequest};

/// Returns the page statement and its matching count statement.
pub fn build_page_query(
    kind: &str,
    request: &TableRequest,
    info: &TableInfo,
) -> SqlResult<(String, String)> {
    if request.page < 1 || request.size < 1 || request.size > 1000 || request.page > 10_000_000 {
        return Err(SqlError::new("invalid page or page size"));
    }
    let name = object_name(kind, &request.object)?;
    let columns: HashSet<&str> = info
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    let where_clause = conditions(kind, request, &columns)?;
    let mut orders: Vec<String> = Vec::new();
    for sort in &request.sorts {
        if !columns.contains(sort.column.as_str()) {
            return Err(SqlError::new("sort column is not in table metadata"));
        }
        let quoted = quote_identifier(kind, &sort.column).unwrap_or_default();
        let direction = if sort.descending { " DESC" } else { " ASC" };
        orders.push(format!("{quoted}{direction}"));
    }
    if orders.is_empty() {
        for column in &info.columns {
            if column.key == "PRI" {
                let quoted = quote_identifier(kind, &column.name).unwrap_or_default();
                orders.push(format!("{quoted} ASC"));
            }
        }
    }
    let order = if orders.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {}", orders.join(", "))
    };
    let offset = (request.page - 1) * request.size;
    let base = format!("SELECT * FROM {name}{where_clause}");
    let count = format!("SELECT COUNT(*) AS navi_total FROM {name}{where_clause}");
    match kind {
        "sqlserver" => {
            let order = if order.is_empty() {
                " ORDER BY (SELECT NULL)".to_string()
            } else {
                order
            };
            Ok((
                format!(
                    "{base}{order} OFFSET {offset} ROWS FETCH NEXT {} ROWS ONLY",
                    request.size
                ),
                count,
            ))
        }
        "oracle" | "dameng" => Ok((
            format!(
                "{base}{order} OFFSET {offset} ROWS FETCH NEXT {} ROWS ONLY",
                request.size
            ),
            count,
        )),
        "iris" | "cache" => {
            if request.page != 1 {
                return Err(SqlError::new(
                    "this driver supports the first TOP preview page only",
                ));
            }
            Ok((
                format!(
                    "SELECT TOP {} * FROM {name}{where_clause}{order}",
                    request.size
                ),
                count,
            ))
        }
        _ => Ok((
            format!("{base}{order} LIMIT {} OFFSET {offset}", request.size),
            count,
        )),
    }
}

/// Keeps current filters and ordering while removing pagination.
pub fn build_export_query(
    kind: &str,
    request: &TableRequest,
    info: &TableInfo,
) -> SqlResult<String> {
    let mut first_page = request.clone();
    first_page.page = 1;
    let (query, _) = build_page_query(kind, &first_page, info)?;
    let query = match kind {
        "oracle" | "dameng" | "sqlserver" => query
            .strip_suffix(&format!(
                " OFFSET 0 ROWS FETCH NEXT {} ROWS ONLY",
                request.size
            ))
            .unwrap_or(&query)
            .to_string(),
        "iris" | "cache" => query.replacen(&format!("SELECT TOP {} ", request.size), "SELECT ", 1),
        _ => query
            .strip_suffix(&format!(" LIMIT {} OFFSET 0", request.size))
            .unwrap_or(&query)
            .to_string(),
    };
    Ok(query)
}

fn conditions(kind: &str, request: &TableRequest, columns: &HashSet<&str>) -> SqlResult<String> {
    let mut parts: Vec<String> = Vec::new();
    let condition = request.condition.trim();
    if !condition.is_empty() {
        parts.push(format!("({condition})"));
    }
    for filter in &request.filters {
        if !columns.contains(filter.column.as_str()) {
            return Err(SqlError::new("filter column is not in table metadata"));
        }
        let column = quote_identifier(kind, &filter.column).unwrap_or_default();
        let operator = filter.operator.trim().to_uppercase();
        let join = match filter.join.as_str() {
            "OR" => "OR",
            "" | "AND" => "AND",
            _ => return Err(SqlError::new("invalid condition join")),
        };
        let mut value = text_literal(kind, &filter.value);
        match operator.as_str() {
            "=" | "<>" | ">" | ">=" | "<" | "<=" | "LIKE" | "NOT LIKE" => {}
            "IS NULL" | "IS NOT NULL" => value.clear(),
            _ => return Err(SqlError::new("invalid filter operator")),
        }
        let mut predicate = format!("({column} {operator}");
        if !value.is_empty() {
            predicate.push(' ');
            predicate.push_str(&value);
        }
        predicate.push(')');
        if !parts.is_empty() {
            predicate = format!("{join} {predicate}");
        }
        parts.push(predicate);
    }
    if parts.is_empty() {
        return Ok(String::new());
    }
    Ok(format!(" WHERE {}", parts.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Filter, Object, Sort};

    fn test_info() -> TableInfo {
        TableInfo {
            columns: vec![
                crate::model::ColumnDefinition {
                    name: "id".into(),
                    key: "PRI".into(),
                    data_type: "INTEGER".into(),
                    ..Default::default()
                },
                crate::model::ColumnDefinition {
                    name: "name".into(),
                    data_type: "TEXT".into(),
                    ..Default::default()
                },
                crate::model::ColumnDefinition {
                    name: "optional".into(),
                    data_type: "TEXT".into(),
                    nullable: "YES".into(),
                    ..Default::default()
                },
            ],
            indexes: Vec::new(),
        }
    }

    #[test]
    fn qualifies_scopes_and_keeps_values_literal() {
        let request = TableRequest {
            object: Object {
                scope: "db.name".into(),
                name: "table\x60name".into(),
                ..Default::default()
            },
            page: 3,
            size: 50,
            filters: vec![Filter {
                column: "name".into(),
                operator: "=".into(),
                value: "\\'; DROP TABLE x; --".into(),
                join: String::new(),
            }],
            ..Default::default()
        };
        let (query, count) = build_page_query("mysql", &request, &test_info()).unwrap();
        assert!(
            query.contains("\x60db.name\x60.\x60table\x60\x60name\x60"),
            "{query}"
        );
        assert!(
            query.ends_with("ORDER BY \x60id\x60 ASC LIMIT 50 OFFSET 100"),
            "{query}"
        );
        assert!(!query.contains("DROP TABLE"), "{query}");
        assert!(query.contains("CONVERT(X'"), "{query}");
        assert!(!count.contains("LIMIT"), "{count}");

        let request = TableRequest {
            object: Object {
                schema: "a.b".into(),
                name: "c\"d".into(),
                ..Default::default()
            },
            page: 1,
            size: 100,
            ..Default::default()
        };
        let (query, _) = build_page_query("postgres", &request, &test_info()).unwrap();
        assert!(query.contains("\"a.b\".\"c\"\"d\""), "{query}");
    }

    #[test]
    fn combines_conditions_and_sorts() {
        let request = TableRequest {
            object: Object {
                name: "items".into(),
                ..Default::default()
            },
            page: 1,
            size: 10,
            filters: vec![
                Filter {
                    column: "name".into(),
                    operator: "like".into(),
                    value: "a%".into(),
                    join: String::new(),
                },
                Filter {
                    column: "optional".into(),
                    operator: "is null".into(),
                    value: String::new(),
                    join: "OR".into(),
                },
            ],
            sorts: vec![Sort {
                column: "name".into(),
                descending: true,
            }],
            condition: "id > 1".into(),
            ..Default::default()
        };
        let (query, count) = build_page_query("sqlite", &request, &test_info()).unwrap();
        assert_eq!(
            query,
            "SELECT * FROM \"items\" WHERE (id > 1) AND (\"name\" LIKE 'a%') OR (\"optional\" IS NULL) ORDER BY \"name\" DESC LIMIT 10 OFFSET 0"
        );
        assert_eq!(
            count,
            "SELECT COUNT(*) AS navi_total FROM \"items\" WHERE (id > 1) AND (\"name\" LIKE 'a%') OR (\"optional\" IS NULL)"
        );
    }

    #[test]
    fn rejects_invalid_metadata_and_pagination() {
        let base = TableRequest {
            object: Object {
                name: "items".into(),
                ..Default::default()
            },
            page: 1,
            size: 100,
            ..Default::default()
        };
        let mut cases = vec![
            base.clone(),
            base.clone(),
            base.clone(),
            base.clone(),
            base.clone(),
        ];
        cases[0].size = 0;
        cases[1].page = -1;
        cases[2].size = 1001;
        cases[3].filters = vec![Filter {
            column: "missing".into(),
            operator: "=".into(),
            ..Default::default()
        }];
        cases[4].sorts = vec![Sort {
            column: "id; DELETE".into(),
            descending: false,
        }];
        for (index, request) in cases.iter().enumerate() {
            assert!(
                build_page_query("sqlite", request, &test_info()).is_err(),
                "case {index}"
            );
        }
        let mut paged = base.clone();
        paged.page = 2;
        let (query, _) = build_page_query("sqlserver", &paged, &test_info()).unwrap();
        assert!(
            query.ends_with("OFFSET 100 ROWS FETCH NEXT 100 ROWS ONLY"),
            "{query}"
        );
    }

    #[test]
    fn export_queries_drop_pagination() {
        let request = TableRequest {
            object: Object {
                name: "items".into(),
                ..Default::default()
            },
            page: 4,
            size: 25,
            ..Default::default()
        };
        let query = build_export_query("mysql", &request, &test_info()).unwrap();
        assert!(query.ends_with("ORDER BY \x60id\x60 ASC"), "{query}");
        assert!(!query.contains("LIMIT"), "{query}");
        let query = build_export_query("sqlserver", &request, &test_info()).unwrap();
        assert!(!query.contains("OFFSET"), "{query}");
        assert!(!query.contains("FETCH"), "{query}");
        let mut first = request.clone();
        first.page = 1;
        let query = build_export_query("iris", &first, &test_info()).unwrap();
        assert!(query.starts_with("SELECT * FROM"), "{query}");
    }
}
