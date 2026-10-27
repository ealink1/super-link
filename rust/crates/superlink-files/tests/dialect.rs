//! Dialect quoting and SQL literal rendering.

use superlink_domain::Object;
use superlink_files::Cell;
use superlink_files::dialect::{normalize, object_name, quote_identifier, sql_literal};

#[test]
fn identifiers_are_quoted_per_dialect() {
    assert_eq!(quote_identifier("mysql", "a`b").unwrap(), "`a``b`");
    assert_eq!(quote_identifier("sqlserver", "a]b").unwrap(), "[a]]b]");
    assert_eq!(quote_identifier("postgres", "a\"b").unwrap(), "\"a\"\"b\"");
    assert!(quote_identifier("mysql", "").is_err());
}

#[test]
fn object_names_use_schema_or_mysql_scope() {
    let object = Object {
        name: "items".to_string(),
        schema: "public".to_string(),
        scope: "demo".to_string(),
        ..Object::default()
    };
    assert_eq!(
        object_name("postgres", &object).unwrap(),
        "\"public\".\"items\""
    );

    let scoped = Object {
        name: "items".to_string(),
        scope: "demo".to_string(),
        ..Object::default()
    };
    assert_eq!(object_name("mysql", &scoped).unwrap(), "`demo`.`items`");
    assert_eq!(object_name("postgres", &scoped).unwrap(), "\"items\"");
}

#[test]
fn sql_literals_match_the_go_export() {
    assert_eq!(sql_literal("postgres", &Cell::Bool(true)).unwrap(), "TRUE");
    assert_eq!(sql_literal("mysql", &Cell::Bool(true)).unwrap(), "1");
    assert_eq!(
        sql_literal("postgres", &Cell::Bool(false)).unwrap(),
        "FALSE"
    );
    assert_eq!(sql_literal("sqlserver", &Cell::Bool(false)).unwrap(), "0");
    assert_eq!(sql_literal("postgres", &Cell::Null).unwrap(), "NULL");
    assert_eq!(
        sql_literal("postgres", &Cell::Int(9223372036854775807)).unwrap(),
        "9223372036854775807"
    );
    assert_eq!(
        sql_literal("postgres", &Cell::Text("O'Reilly".to_string())).unwrap(),
        "E'O''Reilly'"
    );
    assert_eq!(
        sql_literal("sqlserver", &Cell::Text("O'Reilly".to_string())).unwrap(),
        "'O''Reilly'"
    );
    assert_eq!(
        sql_literal("mysql", &Cell::Bytes(vec![0xde, 0xad])).unwrap(),
        "X'dead'"
    );
    assert_eq!(
        sql_literal("postgres", &Cell::Bytes(vec![0xde, 0xad])).unwrap(),
        "decode('dead', 'hex')"
    );
    assert_eq!(
        sql_literal("oracle", &Cell::Bytes(vec![0xde, 0xad])).unwrap(),
        "HEXTORAW('dead')"
    );
    assert_eq!(
        sql_literal("sqlserver", &Cell::Bytes(vec![0xde, 0xad])).unwrap(),
        "0xdead"
    );
    assert!(sql_literal("postgres", &Cell::Float(f64::NAN)).is_err());
    assert_eq!(
        sql_literal("mysql", &Cell::Text("a'b".to_string())).unwrap(),
        "CONVERT(X'612762' USING utf8mb4)"
    );
}

#[test]
fn dialect_aliases_are_normalized() {
    assert_eq!(normalize("PostgreSQL"), "postgres");
    assert_eq!(normalize("pgx"), "postgres");
    assert_eq!(normalize("doris"), "diros");
    assert_eq!(normalize("greatdb"), "goldendb");
    assert_eq!(normalize("TiDB"), "tidb");
}
