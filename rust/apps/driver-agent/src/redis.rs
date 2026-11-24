use ::redis::{ConnectionLike, Value as Reply};
use serde_json::{Value, json};
use superlink_domain::{ConnectionConfig, ResultSet};
pub struct Redis {
    connection: Box<dyn ConnectionLike>,
    database: u8,
}
impl Redis {
    pub fn open(config: &ConnectionConfig) -> Result<Self, String> {
        let timeout = std::time::Duration::from_secs(u64::from(config.timeout.max(1)));
        let mut connection = if config.topology == "cluster" {
            let hosts = if config.hosts.is_empty() {
                vec![format!("{}:{}", config.host, config.port)]
            } else {
                config.hosts.clone()
            };
            let urls = hosts
                .iter()
                .map(|h| endpoint(config, h))
                .collect::<Result<Vec<_>, _>>()?;
            let mut builder =
                ::redis::cluster::ClusterClient::builder(urls).connection_timeout(timeout);
            if config.use_ssl {
                builder = builder.certs(certificates(config)?);
            }
            let client = builder.build().map_err(|_| "Redis Cluster 配置无效")?;
            Box::new(
                client
                    .get_connection()
                    .map_err(|_| "Redis Cluster 连接失败")?,
            ) as Box<dyn ConnectionLike>
        } else {
            let host = format!(
                "{}:{}",
                config.host,
                if config.port == 0 { 6379 } else { config.port }
            );
            let mut sentinel = config.clone();
            if !config.redis_sentinel_master.is_empty() {
                sentinel.user = config.redis_sentinel_user.clone();
                sentinel.password = config.redis_sentinel_password.clone();
                sentinel.redis_db = 0;
                sentinel.uri.clear();
            }
            let url = endpoint(&sentinel, &host)?;
            let client = redis_client(config, &url)?;
            let mut conn = client
                .get_connection_with_timeout(timeout)
                .map_err(|_| "Redis 连接失败")?;
            conn.set_read_timeout(Some(std::time::Duration::from_secs(
                config.query_timeout.max(30).into(),
            )))
            .map_err(|_| "Redis 超时设置失败")?;
            conn.set_write_timeout(Some(timeout))
                .map_err(|_| "Redis 超时设置失败")?;
            if !config.redis_sentinel_master.is_empty() {
                let address: Vec<String> = ::redis::cmd("SENTINEL")
                    .arg("get-master-addr-by-name")
                    .arg(&config.redis_sentinel_master)
                    .query(&mut conn)
                    .map_err(|_| "Redis Sentinel 发现失败")?;
                if address.len() != 2 {
                    return Err("Redis Sentinel 返回无效地址".into());
                }
                let client = redis_client(
                    config,
                    &endpoint(config, &format!("{}:{}", address[0], address[1]))?,
                )?;
                conn = client
                    .get_connection_with_timeout(timeout)
                    .map_err(|_| "Redis 主节点连接失败")?;
            }
            conn.set_read_timeout(Some(std::time::Duration::from_secs(
                if config.query_timeout == 0 {
                    30
                } else {
                    config.query_timeout.into()
                },
            )))
            .map_err(|_| "Redis 超时设置失败")?;
            conn.set_write_timeout(Some(timeout))
                .map_err(|_| "Redis 超时设置失败")?;
            Box::new(conn) as Box<dyn ConnectionLike>
        };
        let _: String = ::redis::cmd("PING")
            .query(connection.as_mut())
            .map_err(|_| "Redis PING 失败")?;
        Ok(Self {
            connection,
            database: if config.topology == "cluster" {
                0
            } else {
                config.redis_db
            },
        })
    }
    pub fn query(&mut self, text: &str, limit: u32) -> Result<ResultSet, String> {
        let args = shlex::split(text).ok_or("Redis 命令引号无效")?;
        let (name, args) = args.split_first().ok_or("Redis 命令为空")?;
        let mut command = ::redis::cmd(name);
        for arg in args {
            command.arg(arg);
        }
        let value: Reply = command
            .query(self.connection.as_mut())
            .map_err(|_| "Redis 命令执行失败")?;
        Ok(crate::rows::json_rows(json!({"value":reply(value)}), limit))
    }
    pub fn databases(&mut self) -> Result<Vec<String>, String> {
        Ok(vec![self.database.to_string()])
    }
    pub fn tables(&mut self, pattern: &str) -> Result<Vec<String>, String> {
        let mut cursor = 0u64;
        let mut keys = Vec::new();
        loop {
            let (next, batch): (u64, Vec<String>) = ::redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(if pattern.is_empty() { "*" } else { pattern })
                .arg("COUNT")
                .arg(100)
                .query(self.connection.as_mut())
                .map_err(|_| "Redis SCAN 失败")?;
            keys.extend(batch.into_iter().take(10000 - keys.len()));
            cursor = next;
            if cursor == 0 || keys.len() >= 10000 {
                break;
            }
        }
        Ok(keys)
    }
}
fn certificates(config: &ConnectionConfig) -> Result<::redis::TlsCertificates, String> {
    let client_tls = if config.ssl_cert_path.is_empty() && config.ssl_key_path.is_empty() {
        None
    } else {
        Some(::redis::ClientTlsConfig {
            client_cert: crate::tls::pem(&config.ssl_cert_path)?,
            client_key: crate::tls::pem(&config.ssl_key_path)?,
        })
    };
    Ok(::redis::TlsCertificates {
        client_tls,
        root_cert: if config.ssl_ca_path.is_empty() {
            None
        } else {
            Some(crate::tls::pem(&config.ssl_ca_path)?)
        },
    })
}
fn redis_client(config: &ConnectionConfig, url: &str) -> Result<::redis::Client, String> {
    if config.use_ssl {
        ::redis::Client::build_with_tls(url, certificates(config)?)
    } else {
        ::redis::Client::open(url)
    }
    .map_err(|_| "Redis TLS 或连接配置无效".into())
}
fn endpoint(config: &ConnectionConfig, host: &str) -> Result<String, String> {
    if !config.uri.is_empty() {
        return Ok(config.uri.clone());
    }
    let mut url = url::Url::parse(&format!(
        "{}://{host}/{}",
        if config.use_ssl { "rediss" } else { "redis" },
        config.redis_db
    ))
    .map_err(|_| "Redis 地址无效")?;
    url.set_username(&config.user)
        .map_err(|_| "Redis 用户名无效")?;
    if !config.password.is_empty() {
        url.set_password(Some(&config.password))
            .map_err(|_| "Redis 凭据无效")?;
    }
    Ok(url.into())
}
fn reply(value: Reply) -> Value {
    match value {
        Reply::Nil => Value::Null,
        Reply::Int(n) => json!(n),
        Reply::BulkString(bytes) => String::from_utf8(bytes.clone())
            .map(Value::String)
            .unwrap_or_else(|_| superlink_drivers::arguments::bytes(&bytes)),
        Reply::Array(items) | Reply::Set(items) => {
            Value::Array(items.into_iter().map(reply).collect())
        }
        Reply::Map(items) => Value::Array(
            items
                .into_iter()
                .map(|(k, v)| json!({"key":reply(k),"value":reply(v)}))
                .collect(),
        ),
        Reply::SimpleString(s) => json!(s),
        Reply::Okay => json!("OK"),
        Reply::Double(n) => json!(n),
        Reply::Boolean(v) => json!(v),
        other => json!(format!("{other:?}")),
    }
}
