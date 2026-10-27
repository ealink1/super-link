//! Cached table statistics queries.

use crate::identifier::text_literal;

/// Reads cached table statistics without scanning table rows.
///
/// An empty string means the dialect does not expose this metadata.
pub fn table_catalog_query(kind: &str, scope: &str) -> String {
    match kind {
        "mysql" | "mariadb" | "goldendb" => format!(
            "SELECT TABLE_NAME, TABLE_ROWS, DATA_LENGTH, ENGINE, CREATE_TIME, UPDATE_TIME, TABLE_COLLATION, TABLE_COMMENT FROM information_schema.TABLES WHERE TABLE_SCHEMA = {} AND TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_NAME LIMIT 10000",
            text_literal(kind, scope)
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_and_quotes_scope() {
        for kind in ["mysql", "mariadb", "goldendb"] {
            let query = table_catalog_query(kind, "db'\\; DROP TABLE x");
            assert!(!query.contains("DROP TABLE"), "{query}");
            assert!(query.contains("LIMIT 10000"), "{query}");
            assert!(query.contains("TABLE_ROWS"), "{query}");
        }
        assert_eq!(table_catalog_query("sqlite", "main"), "");
    }
}
