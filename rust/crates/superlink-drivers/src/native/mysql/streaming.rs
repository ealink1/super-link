use super::*;
impl NativeMysql {
    pub fn stream_query(
        &mut self,
        sql: &str,
        args: &[Value],
        mut consume: impl FnMut(&[String], &[Vec<Value>]) -> Result<(), String>,
    ) -> Result<u64, DriverError> {
        let params = args
            .iter()
            .map(|v| {
                Ok(if let Some(b) = crate::arguments::binary(v)? {
                    MyValue::Bytes(b)
                } else {
                    match v {
                        Value::Null => MyValue::NULL,
                        Value::Bool(v) => MyValue::Int(i64::from(*v)),
                        Value::Number(n) if n.is_i64() => MyValue::Int(n.as_i64().unwrap()),
                        Value::Number(n) if n.is_u64() => MyValue::UInt(n.as_u64().unwrap()),
                        Value::Number(n) => MyValue::Double(n.as_f64().unwrap()),
                        Value::String(v) => MyValue::Bytes(v.as_bytes().to_vec()),
                        _ => return Err(DriverError::Malformed("parameter type")),
                    }
                })
            })
            .collect::<Result<Vec<_>, DriverError>>()?;
        let mut rows = self
            .conn
            .exec_iter(sql, mysql::Params::Positional(params))
            .map_err(|_| DriverError::Native("流式查询失败".into()))?;
        let columns = rows.columns();
        let columns = columns.as_ref();
        let names = columns.iter().map(|c| c.name_str().to_string()).collect();
        let kinds = columns
            .iter()
            .map(|c| {
                (
                    c.column_type(),
                    c.character_set(),
                    c.flags().contains(ColumnFlags::UNSIGNED_FLAG),
                )
            })
            .collect::<Vec<_>>();
        let mut sink = crate::stream::Rows::new(names, &mut consume)?;
        for row in rows.by_ref() {
            let row = row.map_err(|_| DriverError::Native("流式结果读取失败".into()))?;
            sink.push(
                kinds
                    .iter()
                    .enumerate()
                    .map(|(n, (kind, charset, unsigned))| {
                        row.as_ref(n)
                            .map(|v| decode_value(v, *kind, *charset, *unsigned))
                            .unwrap_or(Value::Null)
                    })
                    .collect(),
            )?;
        }
        sink.finish()
    }
}
