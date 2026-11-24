//! Same opaque credential split as the original Fyne configuration vault.
use crate::{ConnectionConfig, DomainError};
use serde_json::{Map, Value};

pub fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace(['_', '-'], "");
    key != "savepassword"
        && (key.contains("password")
            || key.contains("token")
            || key.contains("secret")
            || key.contains("apikey")
            || matches!(
                key.as_str(),
                "dsn" | "uri" | "connectionparams" | "authorization" | "passphrase" | "privatekey"
            ))
}
fn secret_url(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(secret_url),
        Value::Object(items) => items
            .iter()
            .any(|(key, value)| sensitive_key(key) || secret_url(value)),
        Value::String(text) => {
            let Some((_, rest)) = text.split_once("://") else {
                return false;
            };
            if rest.split('/').next().unwrap_or_default().contains('@') {
                return true;
            }
            rest.split_once('?').is_some_and(|(_, query)| {
                query
                    .split('&')
                    .any(|pair| sensitive_key(pair.split('=').next().unwrap_or_default()))
            })
        }
        _ => false,
    }
}
pub fn split_map(public: &mut Map<String, Value>) -> Map<String, Value> {
    let mut secrets = Map::new();
    let keys = public.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        if sensitive_key(&key)
            || public
                .get(&key)
                .is_some_and(|v| !v.is_object() && secret_url(v))
        {
            if let Some(value) = public.remove(&key)
                && value != Value::Null
                && value != Value::String(String::new())
            {
                secrets.insert(key, value);
            }
            continue;
        }
        if let Some(Value::Object(nested)) = public.get_mut(&key) {
            let part = split_map(nested);
            if !part.is_empty() {
                secrets.insert(key, Value::Object(part));
            }
        }
    }
    secrets
}
pub fn merge(public: &mut Map<String, Value>, secret: &Map<String, Value>) {
    for (key, value) in secret {
        if let Value::Object(nested) = value {
            let target = public
                .entry(key.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if !target.is_object() {
                *target = Value::Object(Map::new());
            }
            merge(target.as_object_mut().unwrap(), nested);
        } else {
            public.insert(key.clone(), value.clone());
        }
    }
}
pub fn split_config(
    config: &ConnectionConfig,
) -> Result<(ConnectionConfig, Map<String, Value>), DomainError> {
    let mut value =
        serde_json::to_value(config).map_err(|_| DomainError::validation("连接配置无法编码"))?;
    let secrets = split_map(
        value
            .as_object_mut()
            .ok_or_else(|| DomainError::validation("连接配置无效"))?,
    );
    let public =
        serde_json::from_value(value).map_err(|_| DomainError::validation("连接配置无效"))?;
    Ok((public, secrets))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn nested_and_opaque_secrets_never_reach_metadata() {
        let config = ConnectionConfig {
            kind: "postgres".into(),
            dsn: "user=alice password=secret".into(),
            connection_params: "token=hidden".into(),
            ssh: Some(
                json!({"host":"localhost","password":"ssh-secret","passphrase":"key-secret"}),
            ),
            extra: json!({"api_token":"secret","endpoint":"https://user:secret@example.test/api"})
                .as_object()
                .unwrap()
                .clone(),
            ..Default::default()
        };
        let (public, secrets) = split_config(&config).unwrap();
        let raw = serde_json::to_string(&public).unwrap();
        assert!(!raw.contains("secret"));
        let mut merged = serde_json::to_value(public).unwrap();
        merge(merged.as_object_mut().unwrap(), &secrets);
        let joined: ConnectionConfig = serde_json::from_value(merged).unwrap();
        assert_eq!(joined, config);
    }
}
