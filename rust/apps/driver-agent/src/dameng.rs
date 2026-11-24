use dameng_types::DmValue;
use serde_json::{Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};
pub struct Dameng {
    connection: dameng::Client,
}
impl Dameng {
    pub fn open(config: &ConnectionConfig) -> Result<Self, String> {
        let options = if config.dsn.is_empty() {
            dameng::ConnectOptions::new(
                &config.host,
                if config.port == 0 { 5236 } else { config.port },
                &config.user,
                &config.password,
            )
        } else {
            dameng::ConnectOptions::from_dsn(&config.dsn).map_err(|_| "达梦 DSN 无效")?
        }
        .connect_timeout(std::time::Duration::from_secs(config.timeout.max(1).into()))
        .ssl(config.use_ssl);
        let mut options = if config.database.is_empty() {
            options
        } else {
            options.schema(&config.database)
        };
        if options.ssl {
            options.tls_connector = Some(crate::tls::native(config)?);
            options.tls_server_name = config
                .extra
                .get("rustServerName")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        let mut connection =
            dameng::Client::connect_with(&options).map_err(|_| "达梦连接或认证失败")?;
        if let Some(schema) = options.schema.as_ref().filter(|s| !s.is_empty()) {
            connection
                .execute(&format!(
                    "SET SCHEMA {}",
                    crate::sql_catalog::identifier(schema)
                ))
                .map_err(|_| "达梦切换 schema 失败")?;
        }
        Ok(Self { connection })
    }
    pub fn query(&mut self, text: &str, args: &[Value], limit: u32) -> Result<ResultSet, String> {
        let control = text.trim().trim_end_matches(';').to_ascii_uppercase();
        match control.as_str() {
            "BEGIN" | "BEGIN TRANSACTION" | "START TRANSACTION" => {
                self.connection.begin().map_err(|_| "达梦开启事务失败")?;
                return Ok(ResultSet::default());
            }
            "COMMIT" => {
                self.connection.commit().map_err(|_| "达梦提交失败")?;
                return Ok(ResultSet::default());
            }
            "ROLLBACK" => {
                self.connection.rollback().map_err(|_| "达梦回滚失败")?;
                return Ok(ResultSet::default());
            }
            _ => {}
        }
        self.connection.result_row_limit = if limit == 0 {
            10000
        } else {
            limit as usize + 1
        };
        let values = args.iter().map(bind).collect::<Result<Vec<_>, _>>()?;
        let refs: Vec<_> = values.iter().map(|v| v.as_ref()).collect();
        if !crate::oracle::is_query(text) {
            return Ok(ResultSet {
                rows_affected: self
                    .connection
                    .execute_with_params(text, &refs)
                    .map_err(|_| "达梦执行失败")? as i64,
                ..Default::default()
            });
        }
        let auto_commit = self.connection.auto_commit;
        self.connection.auto_commit = false;
        let raw = self
            .connection
            .query_with_params(text, &refs)
            .map_err(|_| "达梦查询失败");
        self.connection.auto_commit = auto_commit;
        let raw = raw?;
        let mut result = ResultSet {
            columns: raw
                .columns
                .iter()
                .map(|c| Column {
                    id: c.name.clone(),
                    name: c.name.clone(),
                })
                .collect(),
            ..Default::default()
        };
        let mut budget = superlink_drivers::budget::ResultBudget::default();
        for row in &raw.rows {
            if limit > 0 && result.rows.len() >= limit as usize {
                result.truncated = true;
                break;
            }
            let values = (0..raw.columns.len())
                .map(|n| {
                    let value = row.get(n, &raw.columns).ok_or("达梦列解码失败")?;
                    if let DmValue::LobLocator(locator) = value {
                        let bytes = self
                            .connection
                            .read_lob(&locator)
                            .map_err(|_| "达梦 LOB 读取失败或超过 1 MiB")?;
                        if locator.is_clob {
                            Ok(json!(
                                String::from_utf8(bytes).map_err(|_| "达梦 CLOB 编码无效")?
                            ))
                        } else {
                            Ok(superlink_drivers::arguments::bytes(&bytes))
                        }
                    } else {
                        convert(value)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !budget.accept(&values) {
                result.truncated = true;
                break;
            }
            result.rows.push(values);
        }
        if raw.total_row_count > result.rows.len() as u64 {
            result.truncated = true;
        }
        Ok(result)
    }
}
fn bind(value: &Value) -> Result<Box<dyn dameng::ToDmValue>, String> {
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
fn convert(value: DmValue) -> Result<Value, String> {
    Ok(match value {
        DmValue::Null => Value::Null,
        DmValue::Boolean(v) => json!(v),
        DmValue::TinyInt(v) => json!(v),
        DmValue::SmallInt(v) => json!(v),
        DmValue::Int(v) => json!(v),
        DmValue::BigInt(v) => json!(v),
        DmValue::Float(v) => json!(v),
        DmValue::Double(v) => json!(v),
        DmValue::Text(v) => json!(v),
        DmValue::Bytea(v) => superlink_drivers::arguments::bytes(&v),
        DmValue::Decimal(v) => json!(v.to_string()),
        DmValue::LobLocator(_) => return Err("达梦 LOB 需要分块读取，不能当作空值返回".into()),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn dameng_fallback_binding_preserves_chinese_and_skips_literals_and_comments() {
        let sql = "SELECT 中文, '?' AS q, ? FROM 表 /* ? */ -- ?\n";
        let result = dameng::sql_bind::substitute(sql, &["'已绑定'".into()]).unwrap();
        assert_eq!(
            result,
            "SELECT 中文, '?' AS q, '已绑定' FROM 表 /* ? */ -- ?\n"
        );
        assert!(dameng::sql_bind::substitute("SELECT ?", &[]).is_err());
        assert!(dameng::sql_bind::substitute("SELECT '?'", &["1".into()]).is_err());
    }
}
