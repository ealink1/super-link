//! 连接串 / 连接参数里的 query 文本解析，供原生驱动合并到各自选项。
//!
//! 旧版 Fyne 用 `url.ParseQuery`：`k=v&k2=v2`，值做百分号解码，重复键取最后一个。

use std::collections::BTreeMap;

/// 解析 `?k=v&k2=v2` 形式的参数文本；非法转义按原样保留。
pub fn parse_params(raw: &str) -> BTreeMap<String, String> {
    let mut query = raw.trim();
    if let Some((_, rest)) = query.split_once('?') {
        query = rest;
    }
    query = query.split('#').next().unwrap_or(query);
    query = query.trim_start_matches(['?', '&']);
    let mut result = BTreeMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        let key = percent_decode(key.replace('+', " ").as_str());
        if key.trim().is_empty() {
            continue;
        }
        result.insert(key, percent_decode(&value.replace('+', " ")));
    }
    result
}

/// 把 URI 与连接参数字段合并成一个参数表；同键以连接参数为准。
pub fn merged_params(uri: &str, connection_params: &str) -> BTreeMap<String, String> {
    let mut params = parse_params(uri);
    for (key, value) in parse_params(connection_params) {
        params.insert(key, value);
    }
    params
}

/// 只解码 %XX。
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_query_text_with_prefix_and_escapes() {
        let params = parse_params("?charset=utf8mb4&time_zone=%27%2B08%3A00%27");
        assert_eq!(params["charset"], "utf8mb4");
        assert_eq!(params["time_zone"], "'+08:00'");
    }

    #[test]
    fn parses_a_full_uri_query_and_decodes_plus_as_space() {
        let params = parse_params("mysql://u@h:3306/db?ssl-mode=required&app+name=a+b");
        assert_eq!(params["ssl-mode"], "required");
        assert_eq!(params["app name"], "a b");
    }

    #[test]
    fn keeps_the_last_duplicate_and_ignores_blank_keys() {
        let params = parse_params("a=1&a=2&=3&");
        assert_eq!(params["a"], "2");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn field_values_override_uri_values() {
        let params = merged_params("mysql://h/db?charset=latin1&tls=true", "charset=utf8mb4");
        assert_eq!(params["charset"], "utf8mb4");
        assert_eq!(params["tls"], "true");
    }

    #[test]
    fn tolerates_truncated_escapes() {
        assert_eq!(parse_params("k=%").get("k").map(String::as_str), Some("%"));
        assert_eq!(
            parse_params("k=%4").get("k").map(String::as_str),
            Some("%4")
        );
    }
}
