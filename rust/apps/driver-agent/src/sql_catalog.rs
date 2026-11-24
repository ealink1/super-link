use crate::sql::Sql;
use serde_json::Value;
use superlink_drivers::{ColumnEntry, TableStructure};
impl Sql {
    pub fn generic_tables(&mut self, database: &str) -> Result<Vec<String>, String> {
        let schema = literal(database);
        let query: String = match self {
            Self::Navicat(_)=>format!("SELECT table_name FROM information_schema.tables WHERE table_schema={} ORDER BY table_name",if database.is_empty(){"DATABASE()".into()}else{literal(database)}),
            Self::Oracle(_) | Self::Dameng(_) => {
                if database.is_empty() { "SELECT USER,table_name FROM user_tables UNION ALL SELECT USER,view_name FROM user_views ORDER BY 1,2".into() }
                else { format!("SELECT owner,table_name FROM all_tables WHERE owner={schema} UNION ALL SELECT owner,view_name FROM all_views WHERE owner={schema} ORDER BY 1,2") }
            }
            Self::SqlServer(_) => {
                "SELECT TABLE_SCHEMA,TABLE_NAME FROM INFORMATION_SCHEMA.TABLES ORDER BY 1,2".into()
            }
            Self::ClickHouse(_) => if database.is_empty(){"SHOW TABLES".into()}else{format!("SHOW TABLES FROM {}",identifier(database))},
            Self::DuckDb(_)=>format!("SELECT table_catalog,table_schema,table_name FROM information_schema.tables WHERE table_schema NOT IN ('information_schema','pg_catalog'){} ORDER BY 1,2,3",if database.is_empty(){String::new()}else{format!(" AND table_catalog={}",literal(database))}),
            Self::Iris(_) | Self::Gauss(_) => {
                "SELECT table_schema,table_name FROM information_schema.tables WHERE table_schema NOT IN ('information_schema','pg_catalog') ORDER BY 1,2".into()
            }
            Self::IotDb(_) => if database.is_empty(){"SHOW DEVICES".into()}else{format!("SHOW DEVICES {}.**",iot_path(database)?)},
            _ => return Err("元数据类型无效".into()),
        };
        let result = self.query(&query, &[], 10000)?;
        Ok(result
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .filter_map(Value::as_str)
                    .map(display_part)
                    .collect::<Vec<_>>()
                    .join(".")
            })
            .collect())
    }
    pub fn generic_structure(
        &mut self,
        database: &str,
        table: &str,
    ) -> Result<TableStructure, String> {
        let default_schema = match self {
            Self::DuckDb(_) => "main",
            Self::Gauss(_) => "public",
            Self::SqlServer(_) => "dbo",
            _ => database,
        };
        let (catalog, schema, name) = if matches!(self, Self::IotDb(_)) {
            (String::new(), String::new(), table.into())
        } else {
            metadata_parts(default_schema, table)?
        };
        let filter = format!(
            "table_name={}{}",
            literal(&name),
            if schema.is_empty() {
                String::new()
            } else {
                format!(" AND table_schema={}", literal(&schema))
            }
        );
        let filter = if catalog.is_empty() {
            filter
        } else {
            format!("{filter} AND table_catalog={}", literal(&catalog))
        };
        let query = match self {
            Self::Navicat(_) => format!(
                "SELECT column_name,column_type,is_nullable,column_default FROM information_schema.columns WHERE {filter} ORDER BY ordinal_position"
            ),
            Self::Oracle(_) | Self::Dameng(_) => {
                if schema.is_empty() {
                    format!(
                        "SELECT COLUMN_NAME,DATA_TYPE,NULLABLE,DATA_DEFAULT FROM USER_TAB_COLUMNS WHERE TABLE_NAME={} ORDER BY COLUMN_ID",
                        literal(&name)
                    )
                } else {
                    format!(
                        "SELECT COLUMN_NAME,DATA_TYPE,NULLABLE,DATA_DEFAULT FROM ALL_TAB_COLUMNS WHERE OWNER={} AND TABLE_NAME={} ORDER BY COLUMN_ID",
                        literal(&schema),
                        literal(&name)
                    )
                }
            }
            Self::SqlServer(_) => format!(
                "SELECT COLUMN_NAME,DATA_TYPE,IS_NULLABLE,COLUMN_DEFAULT FROM INFORMATION_SCHEMA.COLUMNS WHERE {filter} ORDER BY ORDINAL_POSITION"
            ),
            Self::ClickHouse(_) => format!("DESCRIBE TABLE {}", qualified(table)),
            Self::Iris(_) | Self::DuckDb(_) | Self::Gauss(_) => format!(
                "SELECT column_name,data_type,is_nullable,column_default FROM information_schema.columns WHERE {filter} ORDER BY ordinal_position"
            ),
            Self::IotDb(_) => format!("SHOW TIMESERIES {}.*", iot_path(table)?),
            _ => return Err("元数据类型无效".into()),
        };
        let result = self.query(&query, &[], 4096)?;
        let text = |r: &Vec<Value>, n| r.get(n).and_then(Value::as_str).unwrap_or("").to_string();
        if matches!(self, Self::IotDb(_)) {
            return Ok(TableStructure {
                columns: result
                    .rows
                    .iter()
                    .map(|r| ColumnEntry {
                        name: text(r, 0),
                        data_type: text(r, 3),
                        nullable: true,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            });
        }
        let mut columns: Vec<_> = result
            .rows
            .iter()
            .map(|r| ColumnEntry {
                name: text(r, 0),
                data_type: text(r, 1),
                nullable: matches!(text(r, 2).as_str(), "Y" | "YES"),
                primary_key: false,
                default_value: r.get(3).and_then(Value::as_str).map(str::to_owned),
                extra: String::new(),
                comment: None,
            })
            .collect();
        let (primary, indexes, foreign_keys) = self.constraints(&schema, &name)?;
        for column in &mut columns {
            column.primary_key = primary.contains(&column.name);
        }
        Ok(TableStructure {
            columns,
            indexes,
            foreign_keys,
        })
    }
}
pub fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
pub fn identifier(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
pub fn qualified(s: &str) -> String {
    superlink_sql::identifier::parse_catalog_path(s)
        .unwrap_or_else(|_| vec![s.into()])
        .iter()
        .map(|part| identifier(part))
        .collect::<Vec<_>>()
        .join(".")
}
fn iot_path(s: &str) -> Result<&str, String> {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '*'))
    {
        Err("IoTDB 路径无效".into())
    } else {
        Ok(s)
    }
}

fn display_part(part: &str) -> String {
    if part.is_empty()
        || part.starts_with(|c: char| c.is_ascii_digit())
        || !part
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
    {
        identifier(part)
    } else {
        part.into()
    }
}
pub fn metadata_parts(
    default_schema: &str,
    table: &str,
) -> Result<(String, String, String), String> {
    let parts = superlink_sql::identifier::parse_catalog_path(table).map_err(|e| e.to_string())?;
    match parts.as_slice() {
        [name] => Ok((String::new(), default_schema.into(), name.clone())),
        [schema, name] => Ok((String::new(), schema.clone(), name.clone())),
        [catalog, schema, name] => Ok((catalog.clone(), schema.clone(), name.clone())),
        _ => Err("对象路径最多三部分".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_dots_are_identifier_content() {
        assert_eq!(
            metadata_parts("public", r#""own.er"."a.b""#).unwrap(),
            (String::new(), "own.er".into(), "a.b".into())
        );
        assert_eq!(
            metadata_parts("main", "memory.main.t").unwrap(),
            ("memory".into(), "main".into(), "t".into())
        );
        assert_eq!(qualified(r#""a.b".t"#), r#""a.b"."t""#);
    }
}
