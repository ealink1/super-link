//! Navicat-compatible MySQL script tunnel, ported from the retained Apache-2.0 adapter.
use base64::Engine;
use serde_json::Value;
use superlink_domain::{ConnectionConfig, ResultSet};
use superlink_drivers::{QueryCancel, native::http::Http};
pub struct Navicat {
    http: Http,
    config: ConnectionConfig,
    endpoint: String,
}
impl Navicat {
    pub fn configured(config: &ConnectionConfig) -> bool {
        config.use_http_tunnel
            && config
                .http_tunnel
                .as_ref()
                .and_then(|v| v["host"].as_str())
                .is_some_and(|s| s.starts_with("https://") || s.starts_with("http://"))
    }
    pub fn open(config: &ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        let endpoint = config
            .http_tunnel
            .as_ref()
            .and_then(|v| v["host"].as_str())
            .ok_or("HTTP 隧道地址缺失")?
            .to_string();
        let url = url::Url::parse(&endpoint).map_err(|_| "HTTP 隧道地址无效")?;
        if !["http", "https"].contains(&url.scheme())
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("HTTP 隧道应为不含内嵌凭据的 http(s) URL".into());
        }
        let mut http = Http::new(config).map_err(|e| e.to_string())?;
        http.set_cancellation(cancel);
        let driver = Self {
            http,
            config: config.clone(),
            endpoint,
        };
        let bytes = driver.post("C", None)?;
        let mut parser = crate::navicat_codec::Parser::new(&bytes);
        parser.header()?;
        for _ in 0..3 {
            parser.block()?;
        }
        Ok(driver)
    }
    fn post(&self, action: &str, query: Option<&str>) -> Result<Vec<u8>, String> {
        let tunnel = self
            .config
            .http_tunnel
            .as_ref()
            .ok_or("HTTP 隧道配置缺失")?;
        let mut fields = vec![
            ("actn", action.to_string()),
            ("host", self.config.host.clone()),
            ("port", self.config.port.to_string()),
            ("login", self.config.user.clone()),
            ("password", self.config.password.clone()),
            ("db", self.config.database.clone()),
        ];
        if let Some(query) = query {
            let encode = tunnel["encodeBase64"].as_bool().unwrap_or(true);
            if encode {
                fields.push(("encodeBase64", "1".into()));
            }
            fields.push((
                "q[]",
                if encode {
                    base64::engine::general_purpose::STANDARD.encode(query)
                } else {
                    query.into()
                },
            ));
        }
        let mut request = self
            .http
            .request(reqwest::Method::POST, &self.endpoint)
            .header("Accept-Encoding", "identity")
            .form(&fields);
        let user = tunnel["user"].as_str().unwrap_or("");
        let password = tunnel["password"].as_str().unwrap_or("");
        if !user.is_empty() || !password.is_empty() {
            request = request.basic_auth(user, Some(password));
        }
        self.http.bytes(request).map_err(|e| e.to_string())
    }
    pub fn query(&self, text: &str, args: &[Value], limit: u32) -> Result<ResultSet, String> {
        let tokens = superlink_sql::tokenize(text);
        if tokens.len() > 10000 {
            return Err("隧道语句超过词法预算".into());
        }
        if tokens.iter().any(|t| {
            t.kind == superlink_sql::TokenKind::Comment
                && (t.text.starts_with("/*!") || t.text.starts_with("/*M!"))
        }) {
            return Err("脚本隧道不接受可能包含事务控制的可执行注释".into());
        }
        let leading = tokens
            .iter()
            .filter(|t| t.kind != superlink_sql::TokenKind::Comment && !t.text.trim().is_empty())
            .take(4)
            .map(|t| t.text.to_ascii_uppercase())
            .collect::<Vec<_>>();
        if leading.first().is_some_and(|s| {
            [
                "BEGIN",
                "COMMIT",
                "ROLLBACK",
                "SAVEPOINT",
                "RELEASE",
                "XA",
                "START",
            ]
            .contains(&s.as_str())
        }) || leading.first().is_some_and(|s| s == "SET")
            && leading.iter().any(|s| s == "AUTOCOMMIT")
        {
            return Err("HTTP 脚本每次请求创建连接，不支持跨请求事务".into());
        }
        let mut sql = String::new();
        let mut index = 0;
        for token in tokens {
            if token.kind == superlink_sql::TokenKind::Plain && token.text == "?" {
                let value = args.get(index).ok_or("绑定参数不足")?;
                index += 1;
                sql.push_str(&literal(value)?);
            } else {
                sql.push_str(&token.text);
            }
        }
        if index != args.len() {
            return Err("绑定参数数量不匹配".into());
        }
        let bytes = self.post("Q", Some(&sql))?;
        crate::navicat_codec::Parser::new(&bytes).result(limit)
    }
}
fn literal(value: &Value) -> Result<String, String> {
    if let Some(bytes) =
        superlink_drivers::arguments::binary(value).map_err(|_| "二进制参数无效")?
    {
        return Ok(format!("X'{}'", hex::encode(bytes)));
    }
    Ok(match value {
        Value::Null => "NULL".into(),
        Value::Bool(v) => if *v { "1" } else { "0" }.into(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => format!("CONVERT(X'{}' USING utf8mb4)", hex::encode(v.as_bytes())),
        _ => return Err("隧道参数类型无效".into()),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn bound_strings_do_not_depend_on_backslash_sql_mode() {
        assert_eq!(
            literal(&json!("a'\\b")).unwrap(),
            "CONVERT(X'61275c62' USING utf8mb4)"
        );
        assert_eq!(
            literal(&superlink_drivers::arguments::bytes(&[0, 255])).unwrap(),
            "X'00ff'"
        );
    }
    #[test]
    fn script_tunnel_authentication_framing_and_bound_query_roundtrip() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker =
            std::thread::spawn(move || {
                for action in ["C", "Q"] {
                    let (mut socket, _) = listener.accept().unwrap();
                    socket
                        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                        .unwrap();
                    let mut header = Vec::new();
                    let mut byte = [0];
                    while !header.ends_with(b"\r\n\r\n") {
                        socket.read_exact(&mut byte).unwrap();
                        header.push(byte[0]);
                        assert!(header.len() < 8192);
                    }
                    let header = String::from_utf8(header).unwrap();
                    let size = header
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|s| s.trim().parse::<usize>().ok())
                        })
                        .unwrap();
                    assert!(header.lines().any(|l| l.split_once(':').is_some_and(
                        |(key, value)| key.eq_ignore_ascii_case("authorization")
                            && value.trim()
                                == format!(
                                    "Basic {}",
                                    base64::engine::general_purpose::STANDARD
                                        .encode("web:password")
                                )
                    )));
                    let mut body = vec![0; size];
                    socket.read_exact(&mut body).unwrap();
                    let form: std::collections::HashMap<_, _> =
                        url::form_urlencoded::parse(&body).into_owned().collect();
                    assert_eq!(form["actn"], action);
                    assert_eq!(form["password"], "database-password");
                    let mut reply = 1111u32.to_be_bytes().to_vec();
                    reply.extend([0; 12]);
                    if action == "C" {
                        reply.extend([0; 3]);
                    } else {
                        assert_eq!(
                            base64::engine::general_purpose::STANDARD
                                .decode(&form["q[]"])
                                .unwrap(),
                            b"SELECT 9223372036854775807 AS n"
                        );
                        for n in [0u32, 0, 0, 1, 1] {
                            reply.extend(n.to_be_bytes());
                        }
                        reply.extend([0; 12]);
                        reply.extend([1, b'n', 0]);
                        for n in [8u32, 0, 20] {
                            reply.extend(n.to_be_bytes());
                        }
                        reply.push(19);
                        reply.extend(b"9223372036854775807");
                        reply.push(0);
                    }
                    write!(
                        socket,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        reply.len()
                    )
                    .unwrap();
                    socket.write_all(&reply).unwrap();
                }
            });
        let config = ConnectionConfig {
            kind: "mysql".into(),
            host: "database.internal".into(),
            port: 3306,
            password: "database-password".into(),
            use_http_tunnel: true,
            http_tunnel: Some(
                json!({"host":format!("http://{address}/tunnel.php"),"user":"web","password":"password"}),
            ),
            ..Default::default()
        };
        let mut sql = crate::sql::Sql::open(&config, &QueryCancel::new()).unwrap();
        assert!(!sql.transactional());
        let result = sql.query("SELECT ? AS n", &[json!(i64::MAX)], 10).unwrap();
        assert_eq!(result.rows, vec![vec![json!(i64::MAX)]]);
        assert!(sql.query("BEGIN", &[], 1).is_err());
        worker.join().unwrap();
    }
}
