//! 连接串（URI）与连接参数的纯文本处理，便于脱离界面测试。
//!
//! 与 Fyne 版行为一致：连接串解析结果填充各字段，字段可反向生成连接串；
//! 连接参数允许带前导 `?`，键值对保持输入顺序。

/// 粘贴连接串后拆出的字段。全部为字符串，交由界面直接写入输入框。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ParsedUri {
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: String,
    pub database: String,
    /// 未解码的 query 文本，可直接填进「连接参数」。
    pub params: String,
}

/// 解析 `scheme://user:pass@host:port/database?k=v` 形式的连接串。
pub(super) fn parse_uri(raw: &str) -> Result<ParsedUri, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("请先填写连接串".into());
    }
    let text = text.strip_prefix("jdbc:").unwrap_or(text);
    let Some((scheme, rest)) = text.split_once("://") else {
        return Err("连接串应包含协议与主机，例如 mysql://root@127.0.0.1:3306/app".into());
    };
    if scheme.trim().is_empty() {
        return Err("连接串缺少协议前缀".into());
    }
    // 用 authority / path?query / fragment 手工切分：数据库名常含点号与中文，
    // 交给通用 URL 解析器会把百分号编码等细节带进来。
    let rest = rest.split('#').next().unwrap_or(rest);
    let (authority, tail) = match rest.split_once('/') {
        Some((authority, tail)) => (authority, tail),
        None => (rest, ""),
    };
    let (path, query) = split_path_query(tail);
    let path = path.strip_prefix('/').unwrap_or(path);
    let (hostport, userinfo) = match authority.rsplit_once('@') {
        Some((userinfo, hostport)) => (hostport, Some(userinfo)),
        None => (authority, None),
    };
    let (host, port) = split_host_port(hostport)?;
    if host.is_empty() {
        return Err("连接串缺少主机名".into());
    }
    let (user, password) = match userinfo {
        Some(info) => match info.split_once(':') {
            Some((user, password)) => (percent_decode(user), percent_decode(password)),
            None => (percent_decode(info), String::new()),
        },
        None => (String::new(), String::new()),
    };
    let host = percent_decode(&host);
    Ok(ParsedUri {
        host,
        port,
        user,
        password,
        database: percent_decode(path),
        params: query.unwrap_or_default().to_string(),
    })
}

/// 把 `path?query` 拆成两段；没有 query 时返回 None。
fn split_path_query(tail: &str) -> (&str, Option<&str>) {
    match tail.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (tail, None),
    }
}

fn split_host_port(authority: &str) -> Result<(String, String), String> {
    if let Some(inner) = authority.strip_prefix('[') {
        let Some((address, tail)) = inner.split_once(']') else {
            return Err("IPv6 地址缺少右方括号".into());
        };
        let port = tail.strip_prefix(':').unwrap_or("").to_string();
        return Ok((format!("[{address}]"), port));
    }
    match authority.split_once(':') {
        Some((host, port)) => Ok((host.to_string(), port.to_string())),
        None => Ok((authority.to_string(), String::new())),
    }
}

/// 只解码 %XX；'+' 在 userinfo 中就是普通字符，不能当空格。
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

/// 需要转义才安全的字符集：保留 RFC 3986 的 unreserved 与子分隔符。
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b'!'
            | b'$'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'='
            | b':'
            | b'/'
            | b'?' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 字段反向生成连接串；`scheme` 为空时用 `default_scheme`。
pub(super) fn build_uri(
    scheme: &str,
    host: &str,
    port: &str,
    user: &str,
    password: &str,
    database: &str,
    params: &str,
) -> String {
    let scheme = if scheme.trim().is_empty() {
        "mysql"
    } else {
        scheme.trim()
    };
    let host = host.trim();
    let authority = if let Some(inner) = host.strip_prefix('[') {
        // 已是 IPv6 字面量
        format!("[{inner}")
    } else if host.contains(':') && !host.contains("://") {
        // 裸 IPv6
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let mut out = format!("{scheme}://");
    if !user.trim().is_empty() || !password.is_empty() {
        out.push_str(&encode(user.trim()));
        if !password.is_empty() {
            out.push(':');
            out.push_str(&encode(password));
        }
        out.push('@');
    }
    out.push_str(&authority);
    if !port.trim().is_empty() {
        out.push(':');
        out.push_str(port.trim());
    }
    let database = database.trim();
    if !database.is_empty() {
        out.push('/');
        out.push_str(&encode(database));
    }
    let params = normalize_params(params);
    if !params.is_empty() {
        out.push('?');
        out.push_str(&params);
    }
    out
}

/// 规范化连接参数：去掉前导 `?` 或 `&`，丢弃空白，保留其余原样。
pub(super) fn normalize_params(raw: &str) -> String {
    let text = raw.trim();
    let text = match text.split_once('?') {
        Some((_, query)) => query,
        None => text,
    };
    let text = text.split('#').next().unwrap_or(text);
    text.trim_start_matches(['?', '&']).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_uri() {
        let parsed =
            parse_uri("mysql://root:s%40cret@10.0.0.5:3307/app_db?charset=utf8mb4").unwrap();
        assert_eq!(parsed.host, "10.0.0.5");
        assert_eq!(parsed.port, "3307");
        assert_eq!(parsed.user, "root");
        assert_eq!(parsed.password, "s@cret");
        assert_eq!(parsed.database, "app_db");
        assert_eq!(parsed.params, "charset=utf8mb4");
    }

    #[test]
    fn parses_jdbc_prefix_and_ipv6_and_missing_parts() {
        let parsed = parse_uri("jdbc:postgresql://[::1]:5433").unwrap();
        assert_eq!(parsed.host, "[::1]");
        assert_eq!(parsed.port, "5433");
        assert!(parsed.user.is_empty());
        assert!(parsed.database.is_empty());
    }

    #[test]
    fn rejects_text_without_scheme_host() {
        assert!(parse_uri("").is_err());
        assert!(parse_uri("root@localhost").is_err());
        assert!(parse_uri("mysql://").is_err());
    }

    #[test]
    fn decodes_chinese_database_and_path() {
        let parsed = parse_uri("mysql://u@h:3306/%E4%B8%9A%E5%8A%A1").unwrap();
        assert_eq!(parsed.database, "业务");
    }

    #[test]
    fn round_trips_fields_into_a_uri() {
        let uri = build_uri(
            "postgres",
            "127.0.0.1",
            "5432",
            "app",
            "p@ss word",
            "mydb",
            "?sslmode=require",
        );
        assert_eq!(
            uri,
            "postgres://app:p%40ss%20word@127.0.0.1:5432/mydb?sslmode=require"
        );
        let back = parse_uri(&uri).unwrap();
        assert_eq!(back.password, "p@ss word");
        assert_eq!(back.params, "sslmode=require");
    }

    #[test]
    fn wraps_bare_ipv6_and_keeps_prefix() {
        assert_eq!(
            build_uri("mysql", "::1", "", "", "", "", ""),
            "mysql://[::1]"
        );
        assert_eq!(
            build_uri("mysql", "[fe80::1]", "", "", "", "", ""),
            "mysql://[fe80::1]"
        );
    }

    #[test]
    fn normalizes_params_text() {
        assert_eq!(normalize_params("  ?a=1&b=2 "), "a=1&b=2");
        assert_eq!(normalize_params("&a=1"), "a=1");
        assert_eq!(normalize_params("mysql://h/db?x=1#frag"), "x=1");
        assert_eq!(normalize_params(""), "");
    }
}
