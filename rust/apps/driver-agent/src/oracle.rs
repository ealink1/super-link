use serde_json::Value;
use superlink_domain::{Column, ConnectionConfig, ResultSet};
pub struct Oracle {
    connection: oracledb::Connection,
    transaction: bool,
}
impl Oracle {
    pub fn open(config: &ConnectionConfig) -> Result<Self, String> {
        let endpoint = if !config.dsn.is_empty() {
            config.dsn.clone()
        } else {
            format!(
                "{}://{}:{}/{}",
                if config.use_ssl { "tcps" } else { "tcp" },
                config.host,
                if config.port == 0 { 1521 } else { config.port },
                config.database
            )
        };
        let mut options = oracledb::Config::default()
            .set_user(&config.user)
            .set_password(&config.password)
            .set_transport_connect_timeout(Some(std::time::Duration::from_secs(
                config.timeout.max(1).into(),
            )))
            .set_connect_string(&endpoint)
            .map_err(|_| "Oracle 连接配置无效")?;
        let params: std::collections::HashMap<_, _> =
            url::form_urlencoded::parse(config.connection_params.as_bytes())
                .into_owned()
                .collect();
        if let Some(wallet) = params.get("walletLocation") {
            options = options.set_wallet_location(wallet);
        }
        if let Some(password) = params.get("walletPassword") {
            options = options.set_wallet_password(password);
        }
        if !config.ssl_ca_path.is_empty()
            || !config.ssl_cert_path.is_empty()
            || !config.ssl_key_path.is_empty()
        {
            return Err("Oracle Thin TLS 使用 connectionParams 中的 walletLocation；不接受未应用的 PEM 配置".into());
        }
        let connection = oracledb::connect(options).map_err(|_| "Oracle 连接或认证失败")?;
        if let Some(schema) = config.extra.get("rustSchema").and_then(Value::as_str) {
            connection
                .execute(
                    &format!(
                        "ALTER SESSION SET CURRENT_SCHEMA={}",
                        crate::sql_catalog::identifier(schema)
                    ),
                    &[],
                )
                .map_err(|_| "Oracle 切换当前 schema 失败")?;
        }
        connection
            .set_call_timeout(Some(std::time::Duration::from_secs(
                config.query_timeout.max(30).into(),
            )))
            .map_err(|_| "Oracle 超时设置失败")?;
        Ok(Self {
            connection,
            transaction: false,
        })
    }
    pub fn query(&mut self, text: &str, args: &[Value], limit: u32) -> Result<ResultSet, String> {
        match text
            .trim()
            .trim_end_matches(';')
            .to_ascii_uppercase()
            .as_str()
        {
            "BEGIN" | "BEGIN TRANSACTION" | "START TRANSACTION" => {
                self.transaction = true;
                return Ok(ResultSet::default());
            }
            "COMMIT" => {
                self.connection.commit().map_err(|_| "Oracle 提交失败")?;
                self.transaction = false;
                return Ok(ResultSet::default());
            }
            "ROLLBACK" => {
                self.connection.rollback().map_err(|_| "Oracle 回滚失败")?;
                self.transaction = false;
                return Ok(ResultSet::default());
            }
            _ => {}
        }
        let values = args.iter().map(bind).collect::<Result<Vec<_>, _>>()?;
        let refs: Vec<_> = values.iter().map(|v| v.as_ref()).collect();
        if !is_query(text) {
            let result = self
                .connection
                .execute(text, &refs)
                .map_err(|_| "Oracle 执行失败")?;
            let rows_affected = result.rows_affected() as i64;
            if !self.transaction {
                self.connection
                    .commit()
                    .map_err(|_| "Oracle 自动提交失败")?;
            }
            return Ok(ResultSet {
                rows_affected,
                ..Default::default()
            });
        }
        let cursor = self
            .connection
            .query(text, &refs)
            .map_err(|_| "Oracle 查询失败")?;
        let mut result = ResultSet {
            columns: cursor
                .columns()
                .iter()
                .map(|c| Column {
                    id: c.name().into(),
                    name: c.name().into(),
                })
                .collect(),
            ..Default::default()
        };
        let mut budget = superlink_drivers::budget::ResultBudget::default();
        for row in cursor {
            if limit > 0 && result.rows.len() >= limit as usize {
                result.truncated = true;
                break;
            }
            let row = row.map_err(|_| "Oracle 读取结果失败")?;
            let values = (0..result.columns.len())
                .map(|n| {
                    if let Ok(value) = row.get::<Option<String>>(n) {
                        return Ok(value.map(Value::String).unwrap_or(Value::Null));
                    }
                    row.get::<Option<Vec<u8>>>(n)
                        .map(|v| {
                            v.map(|b| superlink_drivers::arguments::bytes(&b))
                                .unwrap_or(Value::Null)
                        })
                        .map_err(|_| "Oracle 列类型不受支持")
                })
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
pub fn is_query(text: &str) -> bool {
    let first = superlink_sql::tokenize(text)
        .into_iter()
        .find(|t| t.kind != superlink_sql::TokenKind::Comment && !t.text.trim().is_empty())
        .map(|t| t.text.to_ascii_uppercase())
        .unwrap_or_default();
    matches!(
        first.as_str(),
        "SELECT" | "WITH" | "EXPLAIN" | "SHOW" | "DESCRIBE" | "DESC"
    )
}
fn bind(value: &Value) -> Result<Box<dyn oracledb::ToDbValue>, String> {
    Ok(
        if let Some(bytes) =
            superlink_drivers::arguments::binary(value).map_err(|_| "二进制参数无效")?
        {
            Box::new(bytes)
        } else {
            match value {
                Value::Null => Box::new(None::<String>),
                Value::Bool(v) => Box::new(*v),
                Value::Number(n) if n.is_i64() => Box::new(n.as_i64().unwrap()),
                Value::Number(n) => Box::new(n.as_f64().ok_or("数值参数无效")?),
                Value::String(v) => Box::new(v.clone()),
                _ => return Err("绑定参数类型无效".into()),
            }
        },
    )
}
