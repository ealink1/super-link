use super::*;
impl NativeSqlite {
    pub fn stream_query(
        &self,
        sql: &str,
        args: &[Value],
        mut consume: impl FnMut(&[String], &[Vec<Value>]) -> Result<(), String>,
    ) -> Result<u64, DriverError> {
        let params = to_sql_params(args)?;
        let mut statement = self
            .conn
            .prepare(sql)
            .map_err(|_| DriverError::Native("SQL 准备失败".into()))?;
        let names = statement
            .column_names()
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>();
        let count = names.len();
        let mut sink = crate::stream::Rows::new(names, &mut consume)?;
        let mut rows = statement
            .query(params_from_iter(params))
            .map_err(|_| DriverError::Native("流式查询失败".into()))?;
        while let Some(row) = rows
            .next()
            .map_err(|_| DriverError::Native("流式结果读取失败".into()))?
        {
            let values = (0..count)
                .map(|n| {
                    row.get_ref(n)
                        .map(to_json)
                        .map_err(|_| DriverError::Native("流式列读取失败".into()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            sink.push(values)?;
        }
        sink.finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sqlite_streams_large_results_without_truncation() {
        let d = NativeSqlite::open(&ConnectionConfig {
            database: ":memory:".into(),
            ..Default::default()
        })
        .unwrap();
        let mut total = 0;
        let count=d.stream_query("WITH RECURSIVE n(x) AS(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10000) SELECT x,printf('%04096d',x) FROM n",&[],|_,rows|{assert!(rows.len()<=256);total+=rows.len();Ok(())}).unwrap();
        assert_eq!(count, 10000);
        assert_eq!(total, 10000);
    }
}
