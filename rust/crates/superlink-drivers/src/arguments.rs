//! Explicit binary arguments and scalar wire types; errors never include values.
use crate::DriverError;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

pub fn bytes(value: &[u8]) -> Value {
    json!({"type":"binary", "value": STANDARD.encode(value)})
}
pub fn binary(value: &Value) -> Result<Option<Vec<u8>>, DriverError> {
    if value.get("type").and_then(Value::as_str) != Some("binary") {
        return Ok(None);
    }
    let text = value
        .get("value")
        .and_then(Value::as_str)
        .ok_or(DriverError::Malformed("binary parameter"))?;
    if text.len() > 2 << 20 {
        return Err(DriverError::Malformed("oversize binary parameter"));
    }
    STANDARD
        .decode(text)
        .map(Some)
        .map_err(|_| DriverError::Malformed("binary parameter"))
}
pub(crate) fn wire(args: &[Value]) -> Result<(Vec<Value>, Vec<String>), DriverError> {
    if args.len() > 10_000 {
        return Err(DriverError::Malformed("too many parameters"));
    }
    let mut values = Vec::new();
    let mut kinds = Vec::new();
    for value in args {
        let (value, kind) = if let Some(bytes) = binary(value)? {
            (json!(STANDARD.encode(bytes)), "binary")
        } else {
            match value {
                Value::Null => (Value::Null, "null"),
                Value::Bool(_) => (value.clone(), "boolean"),
                Value::String(_) => (value.clone(), "string"),
                Value::Number(n) if n.is_i64() => (json!(n.to_string()), "integer"),
                Value::Number(n) if n.is_u64() => (json!(n.to_string()), "string"),
                Value::Number(n) => (json!(n.to_string()), "float"),
                _ => return Err(DriverError::Malformed("parameter type")),
            }
        };
        values.push(value);
        kinds.push(kind.into());
    }
    Ok((values, kinds))
}
