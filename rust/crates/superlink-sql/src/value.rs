//! Binding conversion for edited values.
//!
//! Integer and decimal precision must survive the round trip, so decimals stay
//! text and integers stay integers unless they exceed `i64`.

use serde_json::Value;

use crate::base64;
use crate::error::{SqlError, SqlResult};
use crate::model::ArgValue;

/// Converts one edited value for the driver binder of `kind` (a column type,
/// not a dialect).
pub fn convert_value(kind: &str, value: &Value) -> SqlResult<ArgValue> {
    if value.is_null() {
        return Ok(ArgValue::Null);
    }
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    };
    let upper = kind.to_uppercase();
    if upper.contains("BOOL") {
        return parse_bool(&text)
            .map(ArgValue::Bool)
            .ok_or_else(|| SqlError::new("invalid boolean value"));
    }
    if upper.contains("INT") {
        if let Ok(integer) = text.parse::<i64>() {
            return Ok(ArgValue::Integer(integer));
        }
        if text.parse::<u64>().is_ok() {
            return Ok(ArgValue::Text(text));
        }
        return Err(SqlError::new("invalid integer value"));
    }
    if upper.contains("REAL") || upper.contains("DOUBLE") || upper.contains("FLOAT") {
        let parsed: f64 = text
            .parse()
            .map_err(|_| SqlError::new("invalid numeric value"))?;
        if !parsed.is_finite() {
            return Err(SqlError::new("value must be a finite number"));
        }
        return Ok(ArgValue::Float(parsed));
    }
    if upper.contains("NUMERIC") || upper.contains("DECIMAL") || upper.contains("NUMBER") {
        let parsed: f64 = text
            .parse()
            .map_err(|_| SqlError::new("invalid decimal value"))?;
        if !parsed.is_finite() {
            return Err(SqlError::new("invalid decimal value"));
        }
        return Ok(ArgValue::Text(text));
    }
    if upper.contains("BLOB") || upper.contains("BYTEA") || upper.contains("BINARY") {
        let bytes = base64::decode(&text).ok_or_else(|| SqlError::new("invalid base64 value"))?;
        return Ok(ArgValue::Bytes(bytes));
    }
    Ok(ArgValue::Text(text))
}

/// `strconv.ParseBool` accepts exactly these spellings.
fn parse_bool(text: &str) -> Option<bool> {
    match text {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_integer_and_decimal_precision() {
        assert_eq!(
            convert_value("BIGINT", &json!("26")).unwrap(),
            ArgValue::Integer(26)
        );
        assert_eq!(
            convert_value("INT", &json!(7)).unwrap(),
            ArgValue::Integer(7)
        );
        assert_eq!(
            convert_value("NUMERIC(10,2)", &json!("12.50")).unwrap(),
            ArgValue::Text("12.50".into())
        );
        assert_eq!(
            convert_value("VARCHAR(20)", &json!("x")).unwrap(),
            ArgValue::Text("x".into())
        );
        assert_eq!(convert_value("TEXT", &json!(null)).unwrap(), ArgValue::Null);
    }

    #[test]
    fn handles_unsigned_bool_and_binary() {
        assert_eq!(
            convert_value("INT", &json!("18446744073709551615")).unwrap(),
            ArgValue::Text("18446744073709551615".into())
        );
        assert!(convert_value("INT", &json!("abc")).is_err());
        assert_eq!(
            convert_value("BOOLEAN", &json!("TRUE")).unwrap(),
            ArgValue::Bool(true)
        );
        assert!(convert_value("BOOLEAN", &json!("yes")).is_err());
        assert_eq!(
            convert_value("BYTEA", &json!("Zm8=")).unwrap(),
            ArgValue::Bytes(b"fo".to_vec())
        );
        assert!(convert_value("BLOB", &json!("!!")).is_err());
        assert!(convert_value("DOUBLE", &json!("NaN")).is_err());
    }
}
