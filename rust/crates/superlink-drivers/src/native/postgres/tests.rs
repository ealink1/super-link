use super::*;

fn config(ssl_mode: &str, use_ssl: bool) -> ConnectionConfig {
    ConnectionConfig {
        kind: "postgresql".into(),
        ssl_mode: ssl_mode.into(),
        use_ssl,
        ..Default::default()
    }
}

#[test]
fn qualifies_non_public_schemas() {
    assert_eq!(qualify("public", "users"), "users");
    assert_eq!(qualify("app", "users"), "app.users");
    assert_eq!(table_kind("BASE TABLE"), "table");
    assert_eq!(table_kind("VIEW"), "view");
}

#[test]
fn groups_composite_indexes_in_column_order() {
    let grouped = group_indexes(vec![
        ("idx_a".into(), false, "a".into()),
        ("idx_a".into(), false, "b".into()),
        ("idx_pk".into(), true, "id".into()),
    ]);
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].columns, vec!["a".to_string(), "b".to_string()]);
    assert!(!grouped[0].unique);
    assert!(grouped[1].unique);
}

#[test]
fn groups_composite_foreign_keys_by_position() {
    let grouped = group_foreign_keys(vec![
        ("fk_pair".into(), "a".into(), "other".into(), "x".into()),
        ("fk_pair".into(), "b".into(), "other".into(), "y".into()),
    ]);
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].columns, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(
        grouped[0].referenced_columns,
        vec!["x".to_string(), "y".to_string()]
    );
    assert_eq!(grouped[0].referenced_table, "other");
}

#[test]
fn splits_schema_qualified_names() {
    assert_eq!(split_table("users"), ("public".into(), "users".into()));
    assert_eq!(split_table("app.users"), ("app".into(), "users".into()));
}

#[test]
fn maps_sslmode_to_a_plan() {
    assert_eq!(ssl_mode(&config("disable", false)).unwrap(), SslMode::Plain);
    assert_eq!(ssl_mode(&config("prefer", false)).unwrap(), SslMode::Plain);
    assert_eq!(ssl_mode(&config("allow", false)).unwrap(), SslMode::Plain);
    assert_eq!(
        ssl_mode(&config("require", false)).unwrap(),
        SslMode::Require
    );
    assert_eq!(
        ssl_mode(&config("verify-full", false)).unwrap(),
        SslMode::Verify
    );
    assert_eq!(
        ssl_mode(&config("verify-ca", false)).unwrap(),
        SslMode::Verify
    );
    // 空值按 useSSL 推断
    assert_eq!(ssl_mode(&config("", true)).unwrap(), SslMode::Require);
    assert_eq!(ssl_mode(&config("", false)).unwrap(), SslMode::Plain);
    // 旧版 Go 配置的写法
    assert_eq!(
        ssl_mode(&config("required", true)).unwrap(),
        SslMode::Require
    );
    assert_eq!(
        ssl_mode(&config("disabled", false)).unwrap(),
        SslMode::Plain
    );
    assert!(ssl_mode(&config("bogus", false)).is_err());
}

#[test]
fn builds_tls_configs_for_both_verification_modes() {
    let require = tls_config(&config("require", false)).unwrap();
    assert!(require.is_some());
    let verify = tls_config(&config("verify-full", false)).unwrap();
    assert!(verify.is_some());
    assert!(tls_config(&config("disable", false)).unwrap().is_none());
}

#[test]
fn requires_cert_and_key_together() {
    let mut half = config("require", false);
    half.ssl_cert_path = "/tmp/client.pem".into();
    let error = tls_config(&half).unwrap_err().to_string();
    assert!(error.contains("同时配置"), "{error}");
}

#[test]
fn rejects_missing_ca_files() {
    let mut bad = config("verify-full", false);
    bad.ssl_ca_path = "/no/such/ca.pem".into();
    assert!(tls_config(&bad).is_err());
}

#[test]
fn decodes_numeric_values() {
    // 123.45 → ndigits=2, weight=0, sign=0, dscale=2, digits=[123, 4500]
    let raw = [0, 2, 0, 0, 0, 0, 0, 2, 0, 123, 0x11, 0x94];
    assert_eq!(decode_numeric(&raw).unwrap(), "123.45");
    // -0.5 → ndigits=1, weight=-1, sign=0x4000, dscale=1, digits=[5000]
    let raw = [0, 1, 0xFF, 0xFF, 0x40, 0x00, 0, 1, 0x13, 0x88];
    assert_eq!(decode_numeric(&raw).unwrap(), "-0.5");
    // 42 → ndigits=1, weight=0, dscale=0, digits=[42]
    let raw = [0, 1, 0, 0, 0, 0, 0, 0, 0, 42];
    assert_eq!(decode_numeric(&raw).unwrap(), "42");
    // NaN
    let raw = [0, 0, 0, 0, 0xC0, 0x00, 0, 0];
    assert_eq!(decode_numeric(&raw).unwrap(), "NaN");
}

/// 真实实例的连接参数；SUPERLINK_TEST_PG=1 开启，
/// SUPERLINK_TEST_PG_KIND=kingbase 切换方言（默认端口随之变为 54321）。
fn live_config() -> Option<ConnectionConfig> {
    // SUPERLINK_TEST_PG=1 开启；PG_KIND=kingbase/vastbase 可切换方言。
    std::env::var("SUPERLINK_TEST_PG").ok()?;
    let kind = std::env::var("SUPERLINK_TEST_PG_KIND").unwrap_or_else(|_| "postgres".into());
    let default_port = if kind.eq_ignore_ascii_case("kingbase") {
        54321
    } else {
        5432
    };
    Some(ConnectionConfig {
        kind,
        host: std::env::var("SUPERLINK_TEST_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: std::env::var("SUPERLINK_TEST_PG_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default_port),
        user: std::env::var("SUPERLINK_TEST_PG_USER").unwrap_or_else(|_| "postgres".into()),
        password: std::env::var("SUPERLINK_TEST_PG_PASSWORD").unwrap_or_default(),
        database: std::env::var("SUPERLINK_TEST_PG_DB").unwrap_or_else(|_| "postgres".into()),
        ssl_mode: std::env::var("SUPERLINK_TEST_PG_SSLMODE").unwrap_or_default(),
        ..Default::default()
    })
}

/// 真实实例冒烟：类型解码 + DDL/DML + 行预算 + 错误路径。
fn live_battery(driver: &mut NativePostgres) {
    let values = driver
        .query(
            "SELECT 1::int4 AS i, 2.5::numeric AS n, 'x'::text AS t, true AS b",
            &[],
            10,
        )
        .unwrap();
    assert_eq!(values.columns.len(), 4);
    assert_eq!(values.rows[0][0], json!(1));
    assert_eq!(values.rows[0][1], json!("2.5"));
    assert_eq!(values.rows[0][2], json!("x"));
    assert_eq!(values.rows[0][3], json!(true));

    driver
        .query(
            "CREATE TEMP TABLE sl_probe(a int, b numeric(10,2), c varchar(20))",
            &[],
            0,
        )
        .unwrap();
    let inserted = driver
        .query(
            "INSERT INTO sl_probe VALUES (1, 12.34, '甲'), (2, 0.50, '乙')",
            &[],
            0,
        )
        .unwrap();
    assert_eq!(inserted.rows_affected, 2, "DDL/DML 的 rows_affected");

    let rows = driver
        .query("SELECT a, b, c FROM sl_probe ORDER BY a", &[], 10)
        .unwrap();
    assert_eq!(rows.rows.len(), 2);
    assert_eq!(rows.rows[0], vec![json!(1), json!("12.34"), json!("甲")]);
    assert_eq!(rows.rows[1][1], json!("0.50"), "numeric 需保留标度");

    let limited = driver
            .query(
                "SELECT a FROM sl_probe UNION ALL SELECT a FROM sl_probe UNION ALL SELECT a FROM sl_probe",
                &[],
                2,
            )
            .unwrap();
    assert_eq!(limited.rows.len(), 2);
    assert!(limited.truncated, "行预算需触发截断");

    let error = driver
        .query("SELECT * FROM missing_table", &[], 0)
        .unwrap_err();
    assert!(!error.to_string().is_empty(), "错误需带服务端信息");
}

#[test]
fn passes_the_battery_on_a_live_pg_server() {
    let Some(config) = live_config() else {
        eprintln!("skipping: SUPERLINK_TEST_PG not set");
        return;
    };
    let mut driver = NativePostgres::open(&config).unwrap();
    live_battery(&mut driver);
}

#[test]
fn formats_uuid_bytes() {
    let raw = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ];
    let value = PgUuid::from_sql(&Type::UUID, &raw).unwrap();
    assert_eq!(value.0, "12345678-9abc-def0-1122-334455667788");
}
