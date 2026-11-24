use super::*;
impl NativePostgres {
    pub fn stream_query(
        &mut self,
        sql: &str,
        args: &[Value],
        mut consume: impl FnMut(&[String], &[Vec<Value>]) -> Result<(), String>,
    ) -> Result<u64, DriverError> {
        let params = to_params(args)?;
        let statement = self
            .client
            .prepare(sql)
            .map_err(|_| DriverError::Native("SQL 准备失败".into()))?;
        let names = statement
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect::<Vec<_>>();
        let count = names.len();
        let mut sink = crate::stream::Rows::new(names, &mut consume)?;
        let mut rows = self
            .client
            .query_raw(&statement, params.iter().map(|v| v.as_ref()))
            .map_err(|_| DriverError::Native("流式查询失败".into()))?;
        while let Some(row) = rows
            .next()
            .map_err(|_| DriverError::Native("流式结果读取失败".into()))?
        {
            sink.push(
                (0..count)
                    .map(|n| to_json(&row, n))
                    .collect::<Result<Vec<_>, _>>()?,
            )?;
        }
        sink.finish()
    }
}
