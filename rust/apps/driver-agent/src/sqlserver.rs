use futures_util::TryStreamExt;
use serde_json::{Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
pub struct SqlServer {
    client: tiberius::Client<Compat<tokio::net::TcpStream>>,
    runtime: tokio::runtime::Runtime,
}
impl SqlServer {
    pub fn open(config: &ConnectionConfig) -> Result<Self, String> {
        let mut options = if config.dsn.is_empty() {
            tiberius::Config::new()
        } else {
            tiberius::Config::from_ado_string(&config.dsn).map_err(|_| "SQL Server DSN 无效")?
        };
        if config.dsn.is_empty() {
            options.host(&config.host);
            options.port(if config.port == 0 { 1433 } else { config.port });
        }
        if config.dsn.is_empty() && !config.database.is_empty() {
            options.database(&config.database);
        }
        if config.dsn.is_empty() {
            options.authentication(tiberius::AuthMethod::sql_server(
                &config.user,
                &config.password,
            ));
        }
        options.encryption(if config.use_ssl {
            tiberius::EncryptionLevel::Required
        } else {
            tiberius::EncryptionLevel::Off
        });
        if !config.ssl_ca_path.is_empty() {
            options.trust_cert_ca(&config.ssl_ca_path);
        }
        let address = config
            .extra
            .get("rustConnectAddress")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| options.get_addr());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "SQL Server 运行时初始化失败")?;
        let client = runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(config.timeout.max(1).into()),
                async {
                    let tcp = tokio::net::TcpStream::connect(address)
                        .await
                        .map_err(|_| "SQL Server 连接失败")?;
                    tcp.set_nodelay(true)
                        .map_err(|_| "SQL Server 连接设置失败")?;
                    tiberius::Client::connect(options, tcp.compat_write())
                        .await
                        .map_err(|_| "SQL Server 握手或认证失败")
                },
            )
            .await
            .map_err(|_| "SQL Server 连接超时")?
        })?;
        Ok(Self { client, runtime })
    }
    pub fn query(&mut self, text: &str, args: &[Value], limit: u32) -> Result<ResultSet, String> {
        let control = text.trim().trim_end_matches(';').to_ascii_uppercase();
        let text = if matches!(control.as_str(), "BEGIN" | "START TRANSACTION") {
            "BEGIN TRANSACTION"
        } else {
            text
        };
        self.runtime.block_on(async {
            let mut query = tiberius::Query::new(text);
            let binaries: Vec<_> = args
                .iter()
                .map(superlink_drivers::arguments::binary)
                .collect::<Result<_, _>>()
                .map_err(|_| "二进制参数无效")?;
            for (value, binary) in args.iter().zip(&binaries) {
                if let Some(binary) = binary {
                    query.bind(binary.as_slice());
                    continue;
                }
                match value {
                    Value::Null => query.bind(None::<&str>),
                    Value::Bool(v) => query.bind(*v),
                    Value::Number(n) if n.is_i64() => query.bind(n.as_i64().unwrap()),
                    Value::Number(n) => query.bind(n.as_f64().ok_or("数值参数无效")?),
                    Value::String(v) => query.bind(v.as_str()),
                    _ => return Err("绑定参数类型无效".into()),
                }
            }
            if !crate::oracle::is_query(text) {
                let result = query
                    .execute(&mut self.client)
                    .await
                    .map_err(|_| "SQL Server 执行失败")?;
                return Ok(ResultSet {
                    rows_affected: result.total() as i64,
                    ..Default::default()
                });
            }
            let mut stream = query
                .query(&mut self.client)
                .await
                .map_err(|_| "SQL Server 查询失败")?;
            let mut result = ResultSet::default();
            if let Some(columns) = stream
                .columns()
                .await
                .map_err(|_| "SQL Server 元数据读取失败")?
            {
                result.columns = columns
                    .iter()
                    .map(|c| Column {
                        id: c.name().into(),
                        name: c.name().into(),
                    })
                    .collect();
            }
            let mut budget = superlink_drivers::budget::ResultBudget::default();
            while let Some(item) = stream
                .try_next()
                .await
                .map_err(|_| "SQL Server 读取结果失败")?
            {
                let tiberius::QueryItem::Row(row) = item else {
                    continue;
                };
                if result.columns.is_empty() {
                    result.columns = row
                        .columns()
                        .iter()
                        .map(|c| Column {
                            id: c.name().into(),
                            name: c.name().into(),
                        })
                        .collect();
                }
                if limit > 0 && result.rows.len() >= limit as usize {
                    result.truncated = true;
                    break;
                }
                let values = row
                    .cells()
                    .map(|(_, value)| convert(value))
                    .collect::<Result<Vec<_>, _>>()?;
                if !budget.accept(&values) {
                    result.truncated = true;
                    break;
                }
                result.rows.push(values);
            }
            Ok(result)
        })
    }
}
fn convert(value: &tiberius::ColumnData<'static>) -> Result<Value, String> {
    use tiberius::ColumnData::*;
    Ok(match value {
        U8(v) => json!(v),
        I16(v) => json!(v),
        I32(v) => json!(v),
        I64(v) => json!(v),
        F32(v) => json!(v),
        F64(v) => json!(v),
        Bit(v) => json!(v),
        String(v) => json!(v),
        Guid(v) => json!(v.map(|x| x.to_string())),
        Binary(v) => v
            .as_ref()
            .map(|v| superlink_drivers::arguments::bytes(v))
            .unwrap_or(Value::Null),
        Numeric(v) => json!(v.map(|n| n.to_string())),
        Xml(v) => json!(v.as_ref().map(|v| v.to_string())),
        DateTime(_) | SmallDateTime(_) | DateTime2(_) => temporal::<chrono::NaiveDateTime>(value)?,
        Time(_) => temporal::<chrono::NaiveTime>(value)?,
        Date(_) => temporal::<chrono::NaiveDate>(value)?,
        DateTimeOffset(_) => temporal::<chrono::DateTime<chrono::FixedOffset>>(value)?,
    })
}
fn temporal<'a, T: tiberius::FromSql<'a> + ToString>(
    value: &'a tiberius::ColumnData<'static>,
) -> Result<Value, String> {
    T::from_sql(value)
        .map(|v| json!(v.map(|v| v.to_string())))
        .map_err(|_| "SQL Server 日期时间解码失败".into())
}
