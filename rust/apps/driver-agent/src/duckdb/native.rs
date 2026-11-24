use duckdb::{Connection, params_from_iter, types::Value as Cell};
use serde_json::{Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};
pub struct DuckDb {
    connection: Connection,
}
impl DuckDb {
    pub fn set_cancellation(&self, cancel: &superlink_drivers::QueryCancel) {
        let interrupt = self.connection.interrupt_handle();
        cancel.on_cancel(move || interrupt.interrupt());
    }

    pub fn open(config: &ConnectionConfig) -> Result<Self, String> {
        let connection = if config.database.is_empty() || config.database == ":memory:" {
            Connection::open_in_memory()
        } else {
            Connection::open(&config.database)
        }
        .map_err(|_| "DuckDB 打开失败")?;
        Ok(Self { connection })
    }
    pub fn query(&mut self, text: &str, args: &[Value], limit: u32) -> Result<ResultSet, String> {
        let values = args
            .iter()
            .map(|value| {
                Ok(
                    if let Some(bytes) =
                        superlink_drivers::arguments::binary(value).map_err(|_| "二进制参数无效")?
                    {
                        Cell::Blob(bytes)
                    } else {
                        match value {
                            Value::Null => Cell::Null,
                            Value::Bool(v) => Cell::Boolean(*v),
                            Value::Number(n) if n.is_i64() => Cell::BigInt(n.as_i64().unwrap()),
                            Value::Number(n) => Cell::Double(n.as_f64().ok_or("数值参数无效")?),
                            Value::String(v) => Cell::Text(v.clone()),
                            _ => return Err("绑定参数类型无效"),
                        }
                    },
                )
            })
            .collect::<Result<Vec<_>, &str>>()?;
        let mut statement = self
            .connection
            .prepare(text)
            .map_err(|_| "DuckDB SQL 准备失败")?;
        if !crate::oracle::is_query(text)
            && !text.trim_start().to_ascii_uppercase().starts_with("PRAGMA")
        {
            return Ok(ResultSet {
                rows_affected: statement
                    .execute(params_from_iter(values))
                    .map_err(|_| "DuckDB 执行失败")? as i64,
                ..Default::default()
            });
        }
        let mut rows = statement
            .query(params_from_iter(values))
            .map_err(|_| "DuckDB 查询失败")?;
        let mut result = ResultSet {
            columns: rows
                .as_ref()
                .ok_or("DuckDB 查询状态无效")?
                .column_names()
                .into_iter()
                .map(|name| Column {
                    id: name.clone(),
                    name,
                })
                .collect(),
            ..Default::default()
        };
        let mut budget = superlink_drivers::budget::ResultBudget::default();
        while let Some(row) = rows.next().map_err(|_| "DuckDB 读取结果失败")? {
            if limit > 0 && result.rows.len() >= limit as usize {
                result.truncated = true;
                break;
            }
            let values = (0..result.columns.len())
                .map(|n| {
                    row.get::<_, Cell>(n)
                        .map(convert)
                        .map_err(|_| "DuckDB 列读取失败")
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
fn convert(value: Cell) -> Value {
    match value {
        Cell::Null => Value::Null,
        Cell::Boolean(v) => json!(v),
        Cell::TinyInt(v) => json!(v),
        Cell::SmallInt(v) => json!(v),
        Cell::Int(v) => json!(v),
        Cell::BigInt(v) => json!(v),
        Cell::HugeInt(v) => json!(v.to_string()),
        Cell::UTinyInt(v) => json!(v),
        Cell::USmallInt(v) => json!(v),
        Cell::UInt(v) => json!(v),
        Cell::UBigInt(v) => json!(v),
        Cell::Float(v) => json!(v),
        Cell::Double(v) => json!(v),
        Cell::Decimal(v) => json!(v.to_string()),
        Cell::Text(v) => json!(v),
        Cell::Blob(v) => superlink_drivers::arguments::bytes(&v),
        other => json!(format!("{other:?}")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_values_and_transaction_rollback() {
        let mut d = DuckDb::open(&ConnectionConfig::default()).unwrap();
        d.query("CREATE TABLE t(id BIGINT,b BLOB)", &[], 1).unwrap();
        d.query("BEGIN", &[], 1).unwrap();
        d.query(
            "INSERT INTO t VALUES(?,?)",
            &[
                json!(i64::MAX),
                superlink_drivers::arguments::bytes(&[0, 255]),
            ],
            1,
        )
        .unwrap();
        assert_eq!(
            d.query("SELECT id FROM t", &[], 1).unwrap().rows[0][0],
            json!(i64::MAX)
        );
        d.query("ROLLBACK", &[], 1).unwrap();
        assert_eq!(
            d.query("SELECT count(*) FROM t", &[], 1).unwrap().rows[0][0],
            json!(0)
        );
    }
}
