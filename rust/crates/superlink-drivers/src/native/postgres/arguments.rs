use super::*;
use postgres::types::{Format, IsNull, ToSql};

/// Text protocol parameters let PostgreSQL apply the actual prepared column type,
/// retaining decimal/unsigned precision and supporting NULL for every SQL type.
struct Parameter(Option<String>);
impl std::fmt::Debug for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Parameter([redacted])")
    }
}
impl ToSql for Parameter {
    fn to_sql(
        &self,
        _: &Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Send + Sync>> {
        match &self.0 {
            None => Ok(IsNull::Yes),
            Some(value) => {
                out.extend_from_slice(value.as_bytes());
                Ok(IsNull::No)
            }
        }
    }
    fn accepts(_: &Type) -> bool {
        true
    }
    fn encode_format(&self, _: &Type) -> Format {
        Format::Text
    }
    postgres::types::to_sql_checked!();
}
pub(super) fn to_params(args: &[Value]) -> Result<Vec<Box<dyn ToSql + Sync>>, DriverError> {
    args.iter()
        .map(|value| {
            let text = if let Some(bytes) = crate::arguments::binary(value)? {
                Some(format!("\\x{}", hex::encode(bytes)))
            } else {
                match value {
                    Value::Null => None,
                    Value::String(s) => Some(s.clone()),
                    Value::Bool(_) | Value::Number(_) => Some(value.to_string()),
                    _ => return Err(DriverError::Malformed("parameter type")),
                }
            };
            Ok(Box::new(Parameter(text)) as Box<dyn ToSql + Sync>)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sends_exact_values_in_the_prepared_column_format() {
        let args = to_params(&[
            json!("12345678901234567890.123456789"),
            Value::Null,
            crate::arguments::bytes(&[0, 255]),
        ])
        .unwrap();
        let mut out = bytes::BytesMut::new();
        args[0].to_sql_checked(&Type::NUMERIC, &mut out).unwrap();
        assert_eq!(&out[..], b"12345678901234567890.123456789");
        out.clear();
        assert!(matches!(
            args[1].to_sql_checked(&Type::INT4, &mut out).unwrap(),
            IsNull::Yes
        ));
        args[2].to_sql_checked(&Type::BYTEA, &mut out).unwrap();
        assert_eq!(&out[..], b"\\x00ff");
    }
}
