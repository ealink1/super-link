use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use superlink_domain::ResultSet;

pub fn arguments(request: &Value) -> Result<Vec<Value>, String> {
    let values = request
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if values.len() > 10000 {
        return Err("绑定参数数量超限".into());
    }
    let Some(types) = request.get("argTypes") else {
        if values.iter().any(|v| v.is_array() || v.is_object()) {
            return Err("绑定参数类型无效".into());
        }
        return Ok(values);
    };
    let types = types.as_array().ok_or("参数类型列表无效")?;
    if types.len() != values.len() {
        return Err("参数类型数量不匹配".into());
    }
    values
        .into_iter()
        .zip(types)
        .map(|(value, kind)| {
            Ok(match kind.as_str().ok_or("参数类型无效")? {
                "null" if value.is_null() => Value::Null,
                "boolean" if value.is_boolean() => value,
                "string" if value.is_string() => value,
                "integer" => json!(
                    value
                        .as_str()
                        .ok_or("整数参数无效")?
                        .parse::<i64>()
                        .map_err(|_| "整数参数无效")?
                ),
                "float" => {
                    let n = value
                        .as_str()
                        .ok_or("浮点参数无效")?
                        .parse::<f64>()
                        .map_err(|_| "浮点参数无效")?;
                    if !n.is_finite() {
                        return Err("浮点参数无效".into());
                    }
                    json!(n)
                }
                "binary" => {
                    let text = value.as_str().ok_or("二进制参数无效")?;
                    if text.len() > 2 << 20 {
                        return Err("二进制参数超限".into());
                    }
                    STANDARD.decode(text).map_err(|_| "二进制参数无效")?;
                    json!({"type":"binary","value":text})
                }
                _ => return Err("绑定参数类型无效".into()),
            })
        })
        .collect()
}
pub fn result(result: ResultSet) -> Value {
    json!({"data":result.rows,"fields":result.columns.into_iter().map(|c|c.name).collect::<Vec<_>>(),
        "rowsAffected":result.rows_affected,"messages":result.messages,"truncated":result.truncated})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_parameters_preserve_integer_precision_and_binary() {
        let args = arguments(
            &json!({"args":["9223372036854775807","AP8="],"argTypes":["integer","binary"]}),
        )
        .unwrap();
        assert_eq!(args[0], json!(i64::MAX));
        assert_eq!(
            superlink_drivers::arguments::binary(&args[1]).unwrap(),
            Some(vec![0, 255])
        );
        assert!(arguments(&json!({"args":["x"],"argTypes":["integer"]})).is_err());
        assert!(arguments(&json!({"args":[1],"argTypes":[]})).is_err());
    }
}
