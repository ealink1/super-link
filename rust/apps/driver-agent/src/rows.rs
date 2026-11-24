use serde_json::{Value, json};
use superlink_domain::{Column, ResultSet};
pub fn json_rows(value: Value, limit: u32) -> ResultSet {
    let items = match value {
        Value::Array(items) => items,
        Value::Null => vec![],
        value => vec![value],
    };
    let mut names = std::collections::BTreeSet::new();
    for row in &items {
        if let Some(object) = row.as_object() {
            names.extend(object.keys().cloned());
        }
    }
    if names.is_empty() {
        names.insert("value".into());
    }
    let columns: Vec<_> = names
        .into_iter()
        .take(4096)
        .map(|name| Column {
            id: name.clone(),
            name,
        })
        .collect();
    let mut result = ResultSet {
        columns,
        ..Default::default()
    };
    let mut budget = superlink_drivers::budget::ResultBudget::default();
    for item in items {
        if limit > 0 && result.rows.len() >= limit as usize {
            result.truncated = true;
            break;
        }
        let row = if let Some(object) = item.as_object() {
            result
                .columns
                .iter()
                .map(|c| object.get(&c.name).cloned().unwrap_or(Value::Null))
                .collect()
        } else {
            vec![item]
        };
        if !budget.accept(&row) {
            result.truncated = true;
            break;
        }
        result.rows.push(row);
    }
    result
}
pub fn list(result: ResultSet) -> Vec<String> {
    result
        .rows
        .into_iter()
        .filter_map(|r| r.into_iter().next())
        .filter_map(|v| v.as_str().map(str::to_string))
        .take(10000)
        .collect()
}
pub fn accepted() -> ResultSet {
    let mut result = json_rows(json!({"accepted":true}), 1);
    result.rows_affected = 1;
    result
}
