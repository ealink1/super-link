use iotdb::client::{
    Session, Value as Cell,
    remote::{Config, RpcSession},
};
use serde_json::json;
use superlink_domain::{Column, ConnectionConfig, ResultSet};
pub struct IotDb {
    session: RpcSession,
    timeout: i64,
}
impl IotDb {
    pub fn open(config: &ConnectionConfig) -> Result<Self, String> {
        if config.use_ssl {
            return Err("IoTDB Rust Thrift 传输尚未支持 TLS；已拒绝明文降级".into());
        }
        let timeout = i64::from(config.query_timeout.max(30)) * 1000;
        let options = Config {
            host: config.host.clone(),
            port: i32::from(if config.port == 0 { 6667 } else { config.port }),
            username: config.user.clone(),
            password: config.password.clone(),
            timeout_ms: Some(timeout),
            fetch_size: 256,
            ..Default::default()
        };
        let mut session = RpcSession::new(options).map_err(|_| "IoTDB 连接失败")?;
        session.open().map_err(|_| "IoTDB 会话认证失败")?;
        Ok(Self { session, timeout })
    }
    pub fn query(&mut self, text: &str, limit: u32) -> Result<ResultSet, String> {
        let mut data = self
            .session
            .execute_statement(text, Some(self.timeout))
            .map_err(|_| "IoTDB SQL 执行失败")?;
        let mut columns: Vec<_> = data
            .get_column_names()
            .into_iter()
            .map(|name| Column {
                id: name.clone(),
                name,
            })
            .collect();
        let timestamp = !data.is_ignore_timestamp();
        if timestamp {
            columns.insert(
                0,
                Column {
                    id: "Time".into(),
                    name: "Time".into(),
                },
            );
        }
        let mut result = ResultSet {
            columns,
            ..Default::default()
        };
        let mut budget = superlink_drivers::budget::ResultBudget::default();
        for row in data.by_ref() {
            if limit > 0 && result.rows.len() >= limit as usize {
                result.truncated = true;
                break;
            }
            let mut values = Vec::new();
            if timestamp {
                values.push(json!(row.timestamp));
            }
            values.extend(row.values.into_iter().map(|v| match v {
                Cell::Bool(v) => json!(v),
                Cell::Int32(v) => json!(v),
                Cell::Int64(v) => json!(v),
                Cell::Float(v) => json!(v),
                Cell::Double(v) => json!(v),
                Cell::Text(v) => json!(v),
                Cell::Null => serde_json::Value::Null,
            }));
            if !budget.accept(&values) {
                result.truncated = true;
                break;
            }
            result.rows.push(values);
        }
        Ok(result)
    }
}
impl Drop for IotDb {
    fn drop(&mut self) {
        let _ = self.session.close();
    }
}
