use crate::sql::Sql;
use serde_json::{Value, json};
pub fn request(driver: &mut Sql, request: &Value) -> Result<Value, String> {
    let method = request["method"].as_str().unwrap_or_default();
    let database = request["dbName"].as_str().unwrap_or_default();
    let table = request["tableName"].as_str().unwrap_or_default();
    match method {
        "getDatabases"=>Ok(json!(driver.databases()?)),
        "getTables"=>Ok(json!(driver.tables(database)?.into_iter().map(|t|t.name).collect::<Vec<_>>())),
        "tableExists"=>Ok(json!(driver.tables(database)?.iter().any(|t|t.name==table))),
        "getAllColumns"=>{
            let mut output=Vec::new();
            for table in driver.tables(database)? {
                for column in driver.structure(database,&table.name)?.columns {
                    if output.len()>=65536{return Err("列目录超过 65536 项，请缩小数据库范围".into());}
                    output.push(json!({"tableName":table.name,"name":column.name,"type":column.data_type}));
                }
            }
            Ok(json!(output))
        },
        "getColumns"=>Ok(json!(driver.structure(database,table)?.columns.into_iter().map(|c|json!({"name":c.name,"type":c.data_type,"nullable":if c.nullable {"YES"}else{"NO"},"key":if c.primary_key{"PRI"}else{""},"default":c.default_value,"extra":c.extra,"comment":c.comment})).collect::<Vec<_>>())),
        "getIndexes"=>Ok(json!(driver.structure(database,table)?.indexes.into_iter().flat_map(|i|i.columns.into_iter().enumerate().map(move|(n,c)|json!({"name":i.name,"nonUnique":if i.unique{0}else{1},"seqInIndex":n+1,"columnName":c}))).collect::<Vec<_>>())),
        "getForeignKeys"=>Ok(json!(driver.structure(database,table)?.foreign_keys.into_iter().flat_map(|f|f.columns.into_iter().zip(f.referenced_columns).map(move|(c,r)|json!({"constraintName":f.name,"columnName":c,"refTableName":f.referenced_table,"refColumnName":r}))).collect::<Vec<_>>())),
        "getTriggers"=>driver.triggers(database,table),
        "getCreateStatement"=>Ok(json!(driver.create_statement(database,table)?)),
        _=>Err("未知目录方法".into()),
    }
}
