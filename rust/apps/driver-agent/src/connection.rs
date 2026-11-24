//! Preserve persisted URI/DSN options without falling back to a different endpoint.
use superlink_domain::ConnectionConfig;
pub fn normalize(config: &mut ConnectionConfig) -> Result<(), String> {
    let protocol = crate::sql::protocol_kind(config).to_string();
    if config.port == 0 {
        config.port = superlink_domain::resolve(&protocol)
            .ok()
            .and_then(|d| d.port)
            .unwrap_or(0);
    }
    let mut parameters = url::form_urlencoded::parse(config.connection_params.as_bytes())
        .into_owned()
        .collect::<Vec<_>>();
    if protocol == "mysql" {
        if !config.dsn.is_empty() && !config.dsn.contains("://") {
            config.dsn = mysql_uri(&config.dsn)?;
        }
        let source = if !config.dsn.is_empty() {
            &config.dsn
        } else {
            &config.uri
        };
        if !source.is_empty() {
            let mut uri = url::Url::parse(source).map_err(|_| "MySQL URI 无效")?;
            parameters.extend(
                uri.query_pairs()
                    .map(|(k, v)| (k.into_owned(), v.into_owned())),
            );
            uri.set_query(None);
            if !config.dsn.is_empty() {
                config.dsn = uri.into();
            } else {
                config.uri = uri.into();
            }
        }
        // The Rust connector natively decodes dates and integers. Go's scan flags
        // do not become server session variables.
        for (key, value) in &parameters {
            match key.as_str() {
                "parseTime" | "loc" | "allowNativePasswords" => {}
                "tls" => match value.as_str() {
                    "false" | "skip-verify" | "true" => {
                        config.use_ssl = value != "false";
                        config.ssl_mode = if value == "skip-verify" {
                            "skip-verify"
                        } else if value == "true" {
                            "verify-full"
                        } else {
                            "disable"
                        }
                        .into();
                    }
                    _ => {
                        return Err(
                            "MySQL 自定义 TLS 注册名不适用于 Rust 驱动，请使用证书路径".into()
                        );
                    }
                },
                "charset" => {
                    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        return Err("MySQL 字符集无效".into());
                    }
                }
                "timeout" => {
                    if config.timeout == 0 {
                        config.timeout = seconds(value)?;
                    }
                }
                "readTimeout" | "writeTimeout" => {
                    if config.query_timeout == 0 {
                        config.query_timeout = seconds(value)?;
                    }
                }
                _ => return Err(format!("MySQL 连接参数 {key} 尚未映射到 Rust 连接器")),
            }
        }
        // Retain charset for the connector's initialization command.
        if let Some((_, value)) = parameters.iter().find(|(k, _)| k == "charset") {
            config
                .extra
                .insert("rustMysqlCharset".into(), serde_json::json!(value));
        }
    }
    if protocol == "postgres" || protocol == "gauss" {
        let source = if !config.dsn.is_empty() {
            &config.dsn
        } else {
            &config.uri
        };
        if source.contains("://") {
            parameters.extend(
                url::Url::parse(source)
                    .map_err(|_| "PostgreSQL URI 无效")?
                    .query_pairs()
                    .map(|(k, v)| (k.into_owned(), v.into_owned())),
            );
        } else if !source.is_empty() {
            for field in shlex::split(source).ok_or("PostgreSQL DSN 引号无效")? {
                if let Some((k, v)) = field.split_once('=') {
                    parameters.push((k.into(), v.into()));
                }
            }
        }
        for (key, value) in &parameters {
            match key.as_str() {
                "sslmode" => config.ssl_mode = value.clone(),
                "sslrootcert" => config.ssl_ca_path = value.clone(),
                "sslcert" => config.ssl_cert_path = value.clone(),
                "sslkey" => config.ssl_key_path = value.clone(),
                _ => {}
            }
        }
    }
    match config.ssl_mode.to_ascii_lowercase().as_str() {
        "require" | "required" | "verify-ca" | "verify-full" | "skip-verify" | "insecure"
        | "true" | "on" => config.use_ssl = true,
        "disable" | "disabled" | "false" | "off" => config.use_ssl = false,
        _ => {}
    }
    if config.use_ssl && (!config.ssl_cert_path.is_empty() || !config.ssl_key_path.is_empty()) {
        match protocol.as_str() {
            "sqlserver" | "pulsar" => {
                return Err("此 Rust 协议实现尚不接受客户端证书，连接已停止".into());
            }
            "mongodb" => {
                return Err(
                    "MongoDB 客户端证书需通过 URI 的 tlsCertificateKeyFile 指定合并 PEM 文件"
                        .into(),
                );
            }
            _ => {}
        }
    }
    Ok(())
}
fn seconds(value: &str) -> Result<u32, String> {
    let value = value.strip_suffix('s').unwrap_or(value);
    value
        .parse::<u32>()
        .map_err(|_| "超时参数请使用整数秒，如 10s".into())
}
fn mysql_uri(source: &str) -> Result<String, String> {
    let (credentials, tail) = source
        .rsplit_once("@tcp(")
        .ok_or("MySQL DSN 请使用 mysql:// URI 或 user:password@tcp(host:port)/database")?;
    let (address, database) = tail.split_once(")/").ok_or("MySQL DSN 地址无效")?;
    let (database, query) = database.split_once('?').unwrap_or((database, ""));
    let mut uri =
        url::Url::parse(&format!("mysql://{address}/")).map_err(|_| "MySQL DSN 地址无效")?;
    let (user, password) = credentials.split_once(':').unwrap_or((credentials, ""));
    uri.set_username(user).map_err(|_| "MySQL DSN 用户名无效")?;
    uri.set_password(Some(password))
        .map_err(|_| "MySQL DSN 凭据无效")?;
    uri.set_path(database);
    if !query.is_empty() {
        uri.set_query(Some(query));
    }
    Ok(uri.into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_legacy_mysql_credentials_and_tls() {
        let mut c = ConnectionConfig {
            kind: "custom".into(),
            driver: "mysql".into(),
            dsn: "u:p@ss?#@tcp(db.test:3307)/中文?parseTime=true&tls=true&charset=utf8mb4".into(),
            ..Default::default()
        };
        normalize(&mut c).unwrap();
        let uri = url::Url::parse(&c.dsn).unwrap();
        assert_eq!(uri.host_str(), Some("db.test"));
        assert_eq!(uri.port(), Some(3307));
        assert!(c.use_ssl);
        assert_eq!(c.ssl_mode, "verify-full");
        assert!(c.dsn.contains("p%40ss%3F%23"));
    }
    #[test]
    fn dsn_ssl_requirement_cannot_be_ignored() {
        let mut c = ConnectionConfig {
            kind: "postgres".into(),
            dsn: "host=db.test password='a b' sslmode=verify-full sslrootcert=/tmp/ca.pem".into(),
            ..Default::default()
        };
        normalize(&mut c).unwrap();
        assert!(c.use_ssl);
        assert_eq!(c.ssl_ca_path, "/tmp/ca.pem");
    }
}
