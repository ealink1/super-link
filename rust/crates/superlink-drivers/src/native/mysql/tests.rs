use super::*;

fn config(ssl_mode: &str, use_ssl: bool) -> ConnectionConfig {
    ConnectionConfig {
        kind: "mysql".into(),
        ssl_mode: ssl_mode.into(),
        use_ssl,
        ..Default::default()
    }
}

#[test]
fn normalizes_table_kinds() {
    assert_eq!(table_kind("BASE TABLE"), "table");
    assert_eq!(table_kind("VIEW"), "view");
    assert_eq!(table_kind("SYSTEM VIEW"), "view");
    assert_eq!(table_kind("view"), "view");
}

#[test]
fn maps_sslmode_to_a_plan() {
    assert_eq!(ssl_mode(&config("disable", false)).unwrap(), SslMode::Plain);
    assert_eq!(ssl_mode(&config("prefer", false)).unwrap(), SslMode::Plain);
    assert_eq!(
        ssl_mode(&config("require", false)).unwrap(),
        SslMode::Require
    );
    assert_eq!(
        ssl_mode(&config("verify-full", false)).unwrap(),
        SslMode::Verify
    );
    assert_eq!(ssl_mode(&config("", true)).unwrap(), SslMode::Require);
    // 旧版 Go 配置的写法
    assert_eq!(
        ssl_mode(&config("required", true)).unwrap(),
        SslMode::Require
    );
    assert_eq!(
        ssl_mode(&config("disabled", false)).unwrap(),
        SslMode::Plain
    );
    assert_eq!(
        ssl_mode(&config("preferred", false)).unwrap(),
        SslMode::Plain
    );
    assert!(ssl_mode(&config("bogus", false)).is_err());
}

#[test]
fn decodes_integers_and_unsigned_values() {
    assert_eq!(
        decode_bytes(b"42", ColumnType::MYSQL_TYPE_LONG, 63, false),
        json!(42)
    );
    assert_eq!(
        decode_bytes(
            b"18446744073709551615",
            ColumnType::MYSQL_TYPE_LONGLONG,
            63,
            true
        ),
        json!(18446744073709551615u64)
    );
    assert_eq!(
        decode_bytes(b"abc", ColumnType::MYSQL_TYPE_LONG, 63, false),
        Value::Null
    );
}

#[test]
fn decodes_floats_and_text() {
    assert_eq!(
        decode_bytes(b"1.5", ColumnType::MYSQL_TYPE_DOUBLE, 63, false),
        json!(1.5)
    );
    assert_eq!(
        decode_bytes(
            "你好".as_bytes(),
            ColumnType::MYSQL_TYPE_VAR_STRING,
            45,
            false
        ),
        json!("你好")
    );
    assert_eq!(
        decode_bytes(
            "12.30".as_bytes(),
            ColumnType::MYSQL_TYPE_NEWDECIMAL,
            45,
            false
        ),
        json!("12.30")
    );
}

#[test]
fn encodes_binary_columns_as_base64() {
    // BLOB 与 VARBINARY 都以二进制字符集上报
    assert_eq!(
        decode_bytes(&[1, 2], ColumnType::MYSQL_TYPE_BLOB, BINARY_CHARSET, false),
        json!("AQI=")
    );
    assert_eq!(
        decode_bytes(
            &[1, 2],
            ColumnType::MYSQL_TYPE_VAR_STRING,
            BINARY_CHARSET,
            false
        ),
        json!("AQI=")
    );
    // 文本列（非二进制字符集）保持字符串
    assert_eq!(
        decode_bytes(
            "甲".as_bytes(),
            ColumnType::MYSQL_TYPE_VAR_STRING,
            45,
            false
        ),
        json!("甲")
    );
    // DECIMAL 是文本，不应 base64
    assert_eq!(
        decode_bytes(
            b"12.34",
            ColumnType::MYSQL_TYPE_NEWDECIMAL,
            BINARY_CHARSET,
            false
        ),
        json!("12.34")
    );
}

fn live_config() -> Option<ConnectionConfig> {
    std::env::var("SUPERLINK_TEST_MYSQL").ok()?;
    Some(ConnectionConfig {
        kind: "mysql".into(),
        host: std::env::var("SUPERLINK_TEST_MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: std::env::var("SUPERLINK_TEST_MYSQL_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3306),
        user: std::env::var("SUPERLINK_TEST_MYSQL_USER").unwrap_or_else(|_| "root".into()),
        password: std::env::var("SUPERLINK_TEST_MYSQL_PASSWORD").unwrap_or_default(),
        database: std::env::var("SUPERLINK_TEST_MYSQL_DB").unwrap_or_else(|_| "mysql".into()),
        ..Default::default()
    })
}

/// 真实实例冒烟：类型解码 + DDL/DML + 行预算 + 错误路径。
fn live_battery(driver: &mut NativeMysql) {
    let values = driver
        .query(
            "SELECT 1 AS i, CAST(12.34 AS DECIMAL(10,2)) AS d, 'x' AS s, NULL AS n",
            &[],
            10,
        )
        .unwrap();
    assert_eq!(values.columns.len(), 4);
    assert_eq!(values.rows[0][0], json!(1));
    assert_eq!(
        values.rows[0][1],
        json!("12.34"),
        "DECIMAL 需保留字符串与标度"
    );
    assert_eq!(values.rows[0][2], json!("x"));
    assert_eq!(values.rows[0][3], Value::Null);

    driver
        .query(
            "CREATE TEMPORARY TABLE sl_probe(a INT, b DECIMAL(10,2), c VARBINARY(8), d DATETIME)",
            &[],
            0,
        )
        .unwrap();
    let inserted = driver
            .query(
                "INSERT INTO sl_probe VALUES (1, 12.34, X'0102', '2026-10-27 13:05:09'), (2, 0.50, NULL, NULL)",
                &[],
                0,
            )
            .unwrap();
    assert_eq!(inserted.rows_affected, 2, "INSERT 的影响行数");

    let rows = driver
        .query("SELECT a, b, c, d FROM sl_probe ORDER BY a", &[], 10)
        .unwrap();
    assert_eq!(rows.rows.len(), 2);
    assert_eq!(rows.rows[0][0], json!(1));
    assert_eq!(rows.rows[0][1], json!("12.34"));
    assert_eq!(rows.rows[0][2], json!("AQI="), "二进制列转 base64");
    assert_eq!(rows.rows[0][3], json!("2026-10-27T13:05:09"));
    assert_eq!(rows.rows[1][1], json!("0.50"), "标度必须保留");

    // MySQL 不允许同一条语句重复引用临时表，截断用不依赖表的 UNION。
    let limited = driver
        .query(
            "SELECT 1 AS x UNION ALL SELECT 2 UNION ALL SELECT 3",
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
fn passes_the_battery_on_a_live_mysql_server() {
    let Some(config) = live_config() else {
        eprintln!("skipping: SUPERLINK_TEST_MYSQL not set");
        return;
    };
    let mut driver = NativeMysql::open(&config).unwrap();
    live_battery(&mut driver);
}

#[test]
fn normalizes_text_protocol_timestamps() {
    assert_eq!(
        decode_bytes(
            b"2026-10-27 13:05:09",
            ColumnType::MYSQL_TYPE_DATETIME,
            63,
            false
        ),
        json!("2026-10-27T13:05:09")
    );
    assert_eq!(
        decode_bytes(b"2026-10-27", ColumnType::MYSQL_TYPE_DATE, 63, false),
        json!("2026-10-27")
    );
    assert_eq!(
        normalize_temporal("2026-10-27 13:05:09.123"),
        "2026-10-27T13:05:09.123"
    );
}

#[test]
fn formats_dates_and_times() {
    let value = decode_value(
        &MyValue::Date(2026, 10, 27, 13, 5, 9, 0),
        ColumnType::MYSQL_TYPE_DATETIME,
        45,
        false,
    );
    assert_eq!(value, json!("2026-10-27T13:05:09"));
    let value = decode_value(
        &MyValue::Date(2026, 10, 27, 0, 0, 0, 0),
        ColumnType::MYSQL_TYPE_DATE,
        45,
        false,
    );
    assert_eq!(value, json!("2026-10-27"));
    let value = decode_value(
        &MyValue::Time(false, 1, 2, 3, 4, 0),
        ColumnType::MYSQL_TYPE_TIME,
        45,
        false,
    );
    assert_eq!(value, json!("26:03:04"));
}
