//! openGauss/GaussDB SHA256 and HighGo SM3 authentication via the reviewed Rust fork.
use fallible_iterator::FallibleIterator;
use opengauss::{
    Client, NoTls,
    types::{FromSql, IsNull, ToSql, Type},
};
use serde_json::{Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};
use superlink_drivers::QueryCancel;
pub struct Gauss {
    client: Client,
}
impl Gauss {
    pub fn open(config: &ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        let source = if config.dsn.is_empty() {
            &config.uri
        } else {
            &config.dsn
        };
        let mut options = if source.is_empty() {
            opengauss::Config::new()
        } else {
            source.parse().map_err(|_| "数据库 DSN 无效")?
        };
        if source.is_empty() {
            options
                .host(
                    config
                        .extra
                        .get("rustConnectAddress")
                        .and_then(Value::as_str)
                        .and_then(|s| s.parse::<std::net::SocketAddr>().ok())
                        .map(|a| a.ip().to_string())
                        .as_deref()
                        .unwrap_or(&config.host),
                )
                .port(if config.port == 0 { 5432 } else { config.port })
                .user(&config.user)
                .password(&config.password)
                .dbname(&config.database);
        }
        options.connect_timeout(std::time::Duration::from_secs(config.timeout.max(1).into()));
        let client = if config.use_ssl {
            options.connect(crate::tls::GaussTls {
                connector: opengauss_native_tls::MakeTlsConnector::new(crate::tls::native(config)?),
                server_name: config
                    .extra
                    .get("rustServerName")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
            })
        } else {
            options.connect(NoTls)
        }
        .map_err(|_| "数据库连接或认证失败")?;
        let token = client.cancel_token();
        cancel.on_cancel(move || {
            let _ = token.cancel_query(NoTls);
        });
        Ok(Self { client })
    }
    pub fn query(&mut self, text: &str, args: &[Value], limit: u32) -> Result<ResultSet, String> {
        let statement = self.client.prepare(text).map_err(|_| "SQL 准备失败")?;
        let params: Vec<_> = args.iter().cloned().map(Parameter).collect();
        let mut result = ResultSet {
            columns: statement
                .columns()
                .iter()
                .map(|c| Column {
                    id: c.name().into(),
                    name: c.name().into(),
                })
                .collect(),
            ..Default::default()
        };
        if result.columns.is_empty() {
            let refs: Vec<_> = params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
            result.rows_affected = self
                .client
                .execute(&statement, &refs)
                .map_err(|_| "SQL 执行失败")? as i64;
            return Ok(result);
        }
        let mut rows = self
            .client
            .query_raw(&statement, params.iter().map(|p| p as &(dyn ToSql + Sync)))
            .map_err(|_| "SQL 执行失败")?;
        let mut budget = superlink_drivers::budget::ResultBudget::default();
        while let Some(row) = rows.next().map_err(|_| "SQL 结果读取失败")? {
            if limit > 0 && result.rows.len() >= limit as usize {
                result.truncated = true;
                break;
            }
            let values = row
                .columns()
                .iter()
                .enumerate()
                .map(|(n, c)| cell(&row, n, c.type_()))
                .collect::<Result<Vec<_>, _>>()?;
            if !budget.accept(&values) {
                result.truncated = true;
                break;
            }
            result.rows.push(values);
        }
        Ok(result)
    }
}
struct Parameter(Value);
impl std::fmt::Debug for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Parameter([redacted])")
    }
}
impl ToSql for Parameter {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Send + Sync>> {
        if self.0.is_null() {
            return Ok(IsNull::Yes);
        }
        if let Some(binary) = superlink_drivers::arguments::binary(&self.0)? {
            out.extend(binary);
            return Ok(IsNull::No);
        }
        let text = self
            .0
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| self.0.to_string());
        match *ty {
            Type::BOOL => {
                out.extend_from_slice(&[u8::from(text.parse::<bool>()?)]);
            }
            Type::INT2 => out.extend_from_slice(&text.parse::<i16>()?.to_be_bytes()),
            Type::INT4 => out.extend_from_slice(&text.parse::<i32>()?.to_be_bytes()),
            Type::INT8 => out.extend_from_slice(&text.parse::<i64>()?.to_be_bytes()),
            Type::OID => out.extend_from_slice(&text.parse::<u32>()?.to_be_bytes()),
            Type::FLOAT4 => out.extend_from_slice(&text.parse::<f32>()?.to_be_bytes()),
            Type::FLOAT8 => out.extend_from_slice(&text.parse::<f64>()?.to_be_bytes()),
            Type::NUMERIC => out.extend(crate::numeric::encode(&text)?),
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::UNKNOWN | Type::JSON => {
                out.extend_from_slice(text.as_bytes())
            }
            Type::JSONB => {
                out.extend_from_slice(&[1]);
                out.extend_from_slice(text.as_bytes());
            }
            _ => return Err("绑定类型尚不支持".into()),
        }
        Ok(IsNull::No)
    }
    fn accepts(_: &Type) -> bool {
        true
    }
    opengauss::types::to_sql_checked!();
}
struct Numeric(String);
impl<'a> FromSql<'a> for Numeric {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self(superlink_drivers::native::postgres::decode_numeric(
            raw,
        )?))
    }
    fn accepts(ty: &Type) -> bool {
        *ty == Type::NUMERIC
    }
}
fn cell(row: &opengauss::Row, index: usize, ty: &Type) -> Result<Value, String> {
    macro_rules! get {
        ($t:ty) => {
            row.try_get::<_, Option<$t>>(index)
                .map(|v| json!(v))
                .map_err(|_| "数据库列解码失败".into())
        };
    }
    match *ty {
        Type::BOOL => get!(bool),
        Type::INT2 => get!(i16),
        Type::INT4 => get!(i32),
        Type::INT8 => get!(i64),
        Type::OID => get!(u32),
        Type::FLOAT4 => get!(f32),
        Type::FLOAT8 => get!(f64),
        Type::NUMERIC => row
            .try_get::<_, Option<Numeric>>(index)
            .map(|v| json!(v.map(|v| v.0)))
            .map_err(|_| "数值解码失败".into()),
        Type::BYTEA => row
            .try_get::<_, Option<Vec<u8>>>(index)
            .map(|v| {
                v.map(|b| superlink_drivers::arguments::bytes(&b))
                    .unwrap_or(Value::Null)
            })
            .map_err(|_| "二进制列解码失败".into()),
        Type::JSON | Type::JSONB => get!(Value),
        Type::DATE => get!(chrono::NaiveDate),
        Type::TIME => get!(chrono::NaiveTime),
        Type::TIMESTAMP => get!(chrono::NaiveDateTime),
        Type::TIMESTAMPTZ => get!(chrono::DateTime<chrono::Utc>),
        _ => get!(String),
    }
}
