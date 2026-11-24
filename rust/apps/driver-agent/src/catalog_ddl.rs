use crate::{
    sql::Sql,
    sql_catalog::{identifier, literal, qualified},
};
use serde_json::{Value, json};
impl Sql {
    pub fn create_statement(&mut self, database: &str, table: &str) -> Result<String, String> {
        let exact = match self {
            Self::Sqlite(_) => Some((
                "SELECT sql FROM sqlite_master WHERE name=?1".into(),
                vec![json!(table)],
                0,
            )),
            Self::Mysql(_) | Self::Navicat(_) => Some((
                format!(
                    "SHOW CREATE TABLE {}",
                    if database.is_empty() {
                        mysql_name(table)
                    } else {
                        format!("{} . {}", mysql_name(database), mysql_name(table))
                    }
                ),
                vec![],
                1,
            )),
            Self::Oracle(_) => {
                let (schema, name) = table.split_once('.').unwrap_or((database, table));
                Some((
                    format!(
                        "SELECT DBMS_METADATA.GET_DDL('TABLE',{}, {}) FROM DUAL",
                        literal(name),
                        if schema.is_empty() {
                            "USER".into()
                        } else {
                            literal(schema)
                        }
                    ),
                    vec![],
                    0,
                ))
            }
            Self::ClickHouse(_) => {
                Some((format!("SHOW CREATE TABLE {}", qualified(table)), vec![], 0))
            }
            Self::DuckDb(_) => Some((
                format!(
                    "SELECT sql FROM duckdb_tables() WHERE table_name={}",
                    literal(&crate::sql_catalog::metadata_parts("", table)?.2)
                ),
                vec![],
                0,
            )),
            Self::Rest(rest) if rest.config.kind == "elasticsearch" => {
                return rest
                    .call(
                        reqwest::Method::GET,
                        &format!("/{}/_mapping", crate::http::segment(table)),
                        None,
                    )
                    .and_then(|v| {
                        serde_json::to_string_pretty(&v).map_err(|_| "映射编码失败".into())
                    });
            }
            Self::Rest(rest)
                if ["qdrant", "chroma", "milvus"].contains(&rest.config.kind.as_str()) =>
            {
                return rest.object_definition(table);
            }
            _ => None,
        };
        if let Some((sql, args, column)) = exact {
            let rows = self.query(&sql, &args, 1)?;
            return rows
                .rows
                .first()
                .and_then(|r| r.get(column))
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or("未找到对象定义".into());
        }
        if !self.transactional() {
            return Ok(format!(
                "-- {}\n-- 此对象不使用关系表 DDL",
                table.replace(['\n', '\r'], " ")
            ));
        }
        let structure = self.structure(database, table)?;
        if structure.columns.is_empty() {
            return Err("未找到列定义".into());
        }
        let mut lines = structure
            .columns
            .iter()
            .map(|c| {
                format!(
                    "  {} {}{}{}",
                    identifier(&c.name),
                    c.data_type,
                    if c.nullable { "" } else { " NOT NULL" },
                    c.default_value
                        .as_ref()
                        .map(|v| format!(" DEFAULT {v}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>();
        let primary = structure
            .columns
            .iter()
            .filter(|c| c.primary_key)
            .map(|c| identifier(&c.name))
            .collect::<Vec<_>>();
        if !primary.is_empty() {
            lines.push(format!("  PRIMARY KEY ({})", primary.join(", ")));
        }
        for fk in structure.foreign_keys {
            lines.push(format!(
                "  CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
                identifier(&fk.name),
                fk.columns
                    .iter()
                    .map(|c| identifier(c))
                    .collect::<Vec<_>>()
                    .join(", "),
                qualified(&fk.referenced_table),
                fk.referenced_columns
                    .iter()
                    .map(|c| identifier(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        Ok(format!(
            "CREATE TABLE {} (\n{}\n);",
            qualified(table),
            lines.join(",\n")
        ))
    }
    pub fn triggers(&mut self, database: &str, table: &str) -> Result<Value, String> {
        let (schema, name) = table.split_once('.').unwrap_or((database, table));
        let query = match self {
            Self::Sqlite(_) => format!(
                "SELECT name,'','',sql,'ROW' FROM sqlite_master WHERE type='trigger' AND tbl_name={}",
                literal(table)
            ),
            Self::Oracle(_) | Self::Dameng(_) => format!(
                "SELECT trigger_name,trigger_type,triggering_event,trigger_body,'ROW' FROM all_triggers WHERE table_owner={} AND table_name={}",
                if schema.is_empty() {
                    "USER".into()
                } else {
                    literal(schema)
                },
                literal(name)
            ),
            Self::SqlServer(_) => format!(
                "SELECT name,'','',OBJECT_DEFINITION(object_id),'STATEMENT' FROM sys.triggers WHERE parent_id=OBJECT_ID({})",
                literal(table)
            ),
            Self::ClickHouse(_) | Self::DuckDb(_) | Self::IotDb(_) => return Ok(json!([])),
            _ if !self.transactional() && !matches!(self, Self::Navicat(_)) => {
                return Ok(json!([]));
            }
            _ => format!(
                "SELECT trigger_name,action_timing,event_manipulation,action_statement,action_orientation FROM information_schema.triggers WHERE event_object_table={}{}",
                literal(name),
                if schema.is_empty() {
                    String::new()
                } else {
                    format!(" AND event_object_schema={}", literal(schema))
                }
            ),
        };
        let rows = self.query(&query, &[], 4096)?;
        Ok(json!(rows.rows.into_iter().map(|r|json!({"name":r.first(),"timing":r.get(1),"event":r.get(2),"statement":r.get(3),"orientation":r.get(4)})).collect::<Vec<_>>()))
    }
}
fn mysql_name(s: &str) -> String {
    format!("`{}`", s.replace('`', "``"))
}
