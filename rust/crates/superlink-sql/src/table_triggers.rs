//! Read-only trigger catalog queries; values are escaped as literals.
use crate::{Object, structure_dialect, text_literal};

pub fn table_triggers_query(kind: &str, object: &Object) -> Option<String> {
    let name = text_literal(kind, &object.name);
    match structure_dialect(kind) {
        "sqlite" => Some(format!(
            "SELECT name, sql AS definition FROM sqlite_master WHERE type='trigger' AND tbl_name={name} ORDER BY name LIMIT 1000"
        )),
        "mysql" => {
            let scope = if object.scope.is_empty() {
                "DATABASE()".into()
            } else {
                text_literal(kind, &object.scope)
            };
            Some(format!(
                "SELECT TRIGGER_NAME AS name, ACTION_TIMING AS timing, EVENT_MANIPULATION AS event, ACTION_STATEMENT AS definition FROM information_schema.TRIGGERS WHERE EVENT_OBJECT_SCHEMA={scope} AND EVENT_OBJECT_TABLE={name} ORDER BY TRIGGER_NAME LIMIT 1000"
            ))
        }
        "postgres" => {
            let schema = if object.schema.is_empty() {
                "current_schema()".into()
            } else {
                text_literal(kind, &object.schema)
            };
            Some(format!(
                "SELECT t.tgname AS name, pg_get_triggerdef(t.oid) AS definition FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE NOT t.tgisinternal AND n.nspname={schema} AND c.relname={name} ORDER BY t.tgname LIMIT 1000"
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trigger_queries_keep_table_and_scope_as_literals_and_are_bounded() {
        let object = Object {
            name: "table'\\name".into(),
            scope: "db'\\name".into(),
            schema: "schema'name".into(),
            ..Default::default()
        };
        for kind in ["sqlite", "mysql", "mariadb", "postgres", "opengauss"] {
            let sql = table_triggers_query(kind, &object).unwrap();
            assert!(sql.starts_with("SELECT "));
            assert!(sql.contains(&text_literal(kind, &object.name)));
            assert!(sql.ends_with("LIMIT 1000"));
        }
        assert!(table_triggers_query("redis", &object).is_none());
    }
}
