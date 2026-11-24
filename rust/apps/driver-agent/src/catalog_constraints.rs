use crate::{sql::Sql, sql_catalog::literal};
use serde_json::Value;
use superlink_drivers::{ForeignKeyEntry, IndexEntry};
type Constraints = (Vec<String>, Vec<IndexEntry>, Vec<ForeignKeyEntry>);
impl Sql {
    pub fn constraints(&mut self, schema: &str, table: &str) -> Result<Constraints, String> {
        if matches!(self, Self::ClickHouse(_) | Self::IotDb(_)) {
            return Ok(Default::default());
        }
        let name = literal(table);
        let schema_filter = if schema.is_empty() {
            String::new()
        } else {
            format!(" AND c.table_schema={}", literal(schema))
        };
        let (primary, indexes, foreign) = match self {
            Self::Navicat(_) => {
                let db = if schema.is_empty() {
                    "DATABASE()".into()
                } else {
                    literal(schema)
                };
                (
                    format!(
                        "SELECT column_name FROM information_schema.key_column_usage WHERE table_schema={db} AND table_name={name} AND constraint_name='PRIMARY' ORDER BY ordinal_position"
                    ),
                    format!(
                        "SELECT index_name,CASE WHEN non_unique=0 THEN 1 ELSE 0 END,column_name FROM information_schema.statistics WHERE table_schema={db} AND table_name={name} ORDER BY index_name,seq_in_index"
                    ),
                    format!(
                        "SELECT constraint_name,column_name,referenced_table_name,referenced_column_name FROM information_schema.key_column_usage WHERE table_schema={db} AND table_name={name} AND referenced_table_name IS NOT NULL ORDER BY constraint_name,ordinal_position"
                    ),
                )
            }
            Self::Oracle(_) | Self::Dameng(_) => {
                let owner = if schema.is_empty() {
                    "USER".into()
                } else {
                    literal(schema)
                };
                (
                    format!(
                        "SELECT k.column_name FROM all_constraints c JOIN all_cons_columns k ON k.owner=c.owner AND k.constraint_name=c.constraint_name WHERE c.owner={owner} AND c.table_name={name} AND c.constraint_type='P' ORDER BY k.position"
                    ),
                    format!(
                        "SELECT i.index_name,CASE WHEN i.uniqueness='UNIQUE' THEN 1 ELSE 0 END,k.column_name FROM all_indexes i JOIN all_ind_columns k ON k.index_owner=i.owner AND k.index_name=i.index_name WHERE i.table_owner={owner} AND i.table_name={name} ORDER BY i.index_name,k.column_position"
                    ),
                    format!(
                        "SELECT c.constraint_name,k.column_name,r.owner||'.'||r.table_name,rk.column_name FROM all_constraints c JOIN all_cons_columns k ON k.owner=c.owner AND k.constraint_name=c.constraint_name JOIN all_constraints r ON r.owner=c.r_owner AND r.constraint_name=c.r_constraint_name JOIN all_cons_columns rk ON rk.owner=r.owner AND rk.constraint_name=r.constraint_name AND rk.position=k.position WHERE c.owner={owner} AND c.table_name={name} AND c.constraint_type='R' ORDER BY c.constraint_name,k.position"
                    ),
                )
            }
            Self::SqlServer(_) => {
                let object = literal(&if schema.is_empty() {
                    table.into()
                } else {
                    format!("{schema}.{table}")
                });
                (
                    format!(
                        "SELECT col.name FROM sys.indexes i JOIN sys.index_columns k ON k.object_id=i.object_id AND k.index_id=i.index_id JOIN sys.columns col ON col.object_id=k.object_id AND col.column_id=k.column_id WHERE i.object_id=OBJECT_ID({object}) AND i.is_primary_key=1 ORDER BY k.key_ordinal"
                    ),
                    format!(
                        "SELECT i.name,CAST(i.is_unique AS INT),col.name FROM sys.indexes i JOIN sys.index_columns k ON k.object_id=i.object_id AND k.index_id=i.index_id JOIN sys.columns col ON col.object_id=k.object_id AND col.column_id=k.column_id WHERE i.object_id=OBJECT_ID({object}) AND k.key_ordinal>0 ORDER BY i.name,k.key_ordinal"
                    ),
                    format!(
                        "SELECT f.name,col.name,OBJECT_SCHEMA_NAME(k.referenced_object_id)+'.'+OBJECT_NAME(k.referenced_object_id),r.name FROM sys.foreign_keys f JOIN sys.foreign_key_columns k ON k.constraint_object_id=f.object_id JOIN sys.columns col ON col.object_id=k.parent_object_id AND col.column_id=k.parent_column_id JOIN sys.columns r ON r.object_id=k.referenced_object_id AND r.column_id=k.referenced_column_id WHERE f.parent_object_id=OBJECT_ID({object}) ORDER BY f.name,k.constraint_column_id"
                    ),
                )
            }
            _ => {
                let join = "FROM information_schema.table_constraints c JOIN information_schema.key_column_usage k ON k.constraint_catalog=c.constraint_catalog AND k.constraint_schema=c.constraint_schema AND k.constraint_name=c.constraint_name";
                (
                    format!(
                        "SELECT k.column_name {join} WHERE c.constraint_type='PRIMARY KEY' AND c.table_name={name}{schema_filter} ORDER BY k.ordinal_position"
                    ),
                    format!(
                        "SELECT c.constraint_name,1,k.column_name {join} WHERE c.constraint_type IN ('PRIMARY KEY','UNIQUE') AND c.table_name={name}{schema_filter} ORDER BY c.constraint_name,k.ordinal_position"
                    ),
                    format!(
                        "SELECT c.constraint_name,k.column_name,uk.table_schema||'.'||uk.table_name,uk.column_name {join} JOIN information_schema.referential_constraints r ON r.constraint_catalog=c.constraint_catalog AND r.constraint_schema=c.constraint_schema AND r.constraint_name=c.constraint_name JOIN information_schema.key_column_usage uk ON uk.constraint_catalog=r.unique_constraint_catalog AND uk.constraint_schema=r.unique_constraint_schema AND uk.constraint_name=r.unique_constraint_name AND uk.ordinal_position=k.position_in_unique_constraint WHERE c.constraint_type='FOREIGN KEY' AND c.table_name={name}{schema_filter} ORDER BY c.constraint_name,k.ordinal_position"
                    ),
                )
            }
        };
        let primary = crate::rows::list(self.query(&primary, &[], 4096)?);
        let mut grouped: Vec<IndexEntry> = vec![];
        for row in self.query(&indexes, &[], 65536)?.rows {
            let name = text(&row, 0);
            let column = text(&row, 2);
            if let Some(index) = grouped.iter_mut().find(|i| i.name == name) {
                index.columns.push(column);
            } else {
                grouped.push(IndexEntry {
                    name,
                    columns: vec![column],
                    unique: row.get(1).is_some_and(|v| v == 1 || v == true || v == "1"),
                });
            }
        }
        let mut fks: Vec<ForeignKeyEntry> = vec![];
        // DuckDB exposes key metadata but has no position_in_unique_constraint column.
        let foreign = if matches!(self, Self::DuckDb(_)) {
            format!(
                "SELECT constraint_name,unnest(constraint_column_names),referenced_table,unnest(referenced_column_names) FROM duckdb_constraints() WHERE constraint_type='FOREIGN KEY' AND table_name={name}"
            )
        } else {
            foreign
        };
        for row in self.query(&foreign, &[], 65536)?.rows {
            let name = text(&row, 0);
            let column = text(&row, 1);
            let referenced = text(&row, 3);
            if let Some(fk) = fks.iter_mut().find(|i| i.name == name) {
                fk.columns.push(column);
                fk.referenced_columns.push(referenced);
            } else {
                fks.push(ForeignKeyEntry {
                    name,
                    columns: vec![column],
                    referenced_table: text(&row, 2),
                    referenced_columns: vec![referenced],
                });
            }
        }
        Ok((primary, grouped, fks))
    }
}
fn text(row: &[Value], index: usize) -> String {
    row.get(index).and_then(Value::as_str).unwrap_or("").into()
}
