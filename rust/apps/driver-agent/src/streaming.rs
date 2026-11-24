use crate::{Output, sql::Sql, write_frame};
use serde_json::{Value, json};
pub fn query(driver: &mut Sql, request: &Value, output: &Output) -> Result<Value, String> {
    let args = crate::protocol::arguments(request)?;
    let sql = request["query"].as_str().ok_or("查询无效")?;
    let mut columns_sent = false;
    let mut consume = |columns: &[String], rows: &[Vec<Value>]| {
        if !columns_sent {
            write_frame(
                output,
                &json!({"id":request["id"],"success":true,"chunkType":"columns","fields":columns}),
            )
            .map_err(|_| "流式输出失败")?;
            columns_sent = true;
        }
        if !rows.is_empty() {
            write_frame(
                output,
                &json!({"id":request["id"],"success":true,"chunkType":"rows","data":rows}),
            )
            .map_err(|_| "流式输出失败")?;
        }
        Ok(())
    };
    match driver {
        Sql::Sqlite(d) => {
            d.stream_query(sql, &args, &mut consume)
                .map_err(|e| e.to_string())?;
        }
        Sql::Mysql(d) => {
            d.stream_query(sql, &args, &mut consume)
                .map_err(|e| e.to_string())?;
        }
        Sql::Postgres(d) => {
            d.stream_query(sql, &args, &mut consume)
                .map_err(|e| e.to_string())?;
        }
        _ => {
            let result = driver.query(sql, &args, 0)?;
            if result.truncated {
                return Err("该驱动流式结果超过预算，未完成导出".into());
            }
            let mut stream = superlink_drivers::stream::Rows::new(
                result.columns.into_iter().map(|c| c.name).collect(),
                &mut consume,
            )
            .map_err(|e| e.to_string())?;
            for row in result.rows {
                stream.push(row).map_err(|e| e.to_string())?;
            }
            stream.finish().map_err(|e| e.to_string())?;
        }
    }
    Ok(json!({"chunkType":"done","data":null}))
}
