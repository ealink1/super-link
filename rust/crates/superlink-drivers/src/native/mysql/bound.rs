use super::*;

impl NativeMysql {
    pub(super) fn query_bound(
        &mut self,
        sql: &str,
        args: &[Value],
        max_rows: u32,
    ) -> Result<ResultSet, DriverError> {
        let started = Instant::now();
        let args = args
            .iter()
            .map(|v| {
                if let Some(bytes) = crate::arguments::binary(v)? {
                    return Ok(MyValue::Bytes(bytes));
                }
                Ok(match v {
                    Value::Null => MyValue::NULL,
                    Value::Bool(b) => MyValue::Int(i64::from(*b)),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            MyValue::Int(i)
                        } else if let Some(u) = n.as_u64() {
                            MyValue::UInt(u)
                        } else {
                            MyValue::Double(n.as_f64().unwrap_or_default())
                        }
                    }
                    Value::String(s) => MyValue::Bytes(s.as_bytes().to_vec()),
                    other => MyValue::Bytes(other.to_string().into_bytes()),
                })
            })
            .collect::<Result<Vec<_>, DriverError>>()?;
        let mut result = self
            .conn
            .exec_iter(sql, mysql::Params::Positional(args))
            .map_err(|e| DriverError::Native(format!("参数查询失败：{e}")))?;
        let columns = result.columns();
        let metadata = columns.as_ref();
        let columns: Vec<Column> = metadata
            .iter()
            .map(|c| Column {
                id: c.name_str().to_string(),
                name: c.name_str().to_string(),
            })
            .collect();
        let kinds = metadata
            .iter()
            .map(|c| {
                (
                    c.column_type(),
                    c.character_set(),
                    c.flags().contains(ColumnFlags::UNSIGNED_FLAG),
                )
            })
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        let mut truncated = false;
        let mut budget = crate::budget::ResultBudget::default();
        for row in result.by_ref() {
            let row = row.map_err(|e| DriverError::Native(format!("读取行失败：{e}")))?;
            if rows.len() >= max_rows.max(1) as usize {
                truncated = true;
                break;
            }
            let values = kinds
                .iter()
                .enumerate()
                .map(|(i, (kind, charset, unsigned))| {
                    row.as_ref(i)
                        .map(|v| decode_value(v, *kind, *charset, *unsigned))
                        .unwrap_or(Value::Null)
                })
                .collect::<Vec<_>>();
            if !budget.accept(&values) {
                truncated = true;
                break;
            }
            rows.push(values);
        }
        drop(result);
        let rows_affected = if columns.is_empty() {
            self.conn.affected_rows() as i64
        } else {
            0
        };
        Ok(ResultSet {
            columns,
            rows,
            rows_affected,
            truncated,
            duration_ns: started.elapsed().as_nanos() as u64,
            ..Default::default()
        })
    }
}
