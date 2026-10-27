//! Bounded read preview for one metadata object.

use crate::error::SqlResult;
use crate::identifier::object_name;
use crate::model::Object;

/// Uses literal metadata identifier components, including dots inside a name.
pub fn preview_read(dialect: &str, object: &Object) -> SqlResult<String> {
    let target = object_name(dialect, object)?;
    Ok(match dialect {
        "sqlserver" => format!("SELECT TOP 100 * FROM {target};"),
        "oracle" | "dameng" => format!("SELECT * FROM {target} WHERE ROWNUM <= 100;"),
        _ => format!("SELECT * FROM {target} LIMIT 100;"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_each_dialect_family() {
        let object = Object {
            name: "order.details".into(),
            scope: "demo".into(),
            ..Default::default()
        };
        assert_eq!(
            preview_read("sqlserver", &object).unwrap(),
            "SELECT TOP 100 * FROM [order.details];"
        );
        assert_eq!(
            preview_read("oracle", &object).unwrap(),
            "SELECT * FROM \"order.details\" WHERE ROWNUM <= 100;"
        );
        assert_eq!(
            preview_read("mysql", &object).unwrap(),
            "SELECT * FROM \x60demo\x60.\x60order.details\x60 LIMIT 100;"
        );
    }
}
