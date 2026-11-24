//! Bounded IRIS/Caché SuperServer transport, ported from MIT go-irisnative.
use crate::iris_codec::{Reader, encode};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};
use superlink_domain::{Column, ConnectionConfig, ResultSet};
use superlink_drivers::QueryCancel;
trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}
pub struct Iris {
    stream: Box<dyn Stream>,
    sequence: u32,
    statement: u32,
    timeout: u32,
    namespace: String,
}
struct Message {
    status: i16,
    body: Vec<u8>,
}
impl Iris {
    pub fn open(config: &ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        let address = (
            config.host.as_str(),
            if config.port == 0 { 1972 } else { config.port },
        )
            .to_socket_addrs()
            .map_err(|_| "IRIS 地址解析失败")?
            .next()
            .ok_or("IRIS 地址为空")?;
        let socket =
            TcpStream::connect_timeout(&address, Duration::from_secs(config.timeout.max(1).into()))
                .map_err(|_| "IRIS 连接失败")?;
        let timeout = config.query_timeout.max(30);
        socket
            .set_read_timeout(Some(Duration::from_secs(timeout.into())))
            .map_err(|_| "IRIS 读超时设置失败")?;
        socket
            .set_write_timeout(Some(Duration::from_secs(timeout.into())))
            .map_err(|_| "IRIS 写超时设置失败")?;
        let interrupt = socket.try_clone().map_err(|_| "IRIS 连接句柄创建失败")?;
        cancel.on_cancel(move || {
            let _ = interrupt.shutdown(std::net::Shutdown::Both);
        });
        let stream: Box<dyn Stream> = if config.use_ssl {
            let name = rustls::pki_types::ServerName::try_from(
                config
                    .extra
                    .get("rustServerName")
                    .and_then(Value::as_str)
                    .unwrap_or(&config.host)
                    .to_string(),
            )
            .map_err(|_| "IRIS TLS 主机名无效")?;
            let connection = rustls::ClientConnection::new(crate::tls::client(config)?, name)
                .map_err(|_| "IRIS TLS 初始化失败")?;
            Box::new(rustls::StreamOwned::new(connection, socket))
        } else {
            Box::new(socket)
        };
        let mut driver = Self {
            stream,
            sequence: 0,
            statement: 0,
            timeout,
            namespace: config.database.clone(),
        };
        driver.send(*b"HS", &69u16.to_le_bytes())?;
        let reply = driver.receive()?;
        if reply.body.len() < 4 {
            return Err("IRIS 握手响应无效".into());
        }
        let mut body = Vec::new();
        for value in [
            json!(config.database),
            superlink_drivers::arguments::bytes(&obfuscate(&config.user)),
            superlink_drivers::arguments::bytes(&obfuscate(&config.password)),
            json!("SuperLink"),
            json!("SuperLink"),
            json!("SuperLink"),
            json!(""),
            json!("rust"),
            json!(""),
            json!(1),
            json!(0),
            json!(37),
        ] {
            body.extend(encode(&value)?);
        }
        driver.send(*b"CN", &body)?;
        let reply = driver.receive()?;
        if reply.status != 0 {
            return Err("IRIS 会话认证失败".into());
        }
        Ok(driver)
    }
    fn send(&mut self, kind: [u8; 2], body: &[u8]) -> Result<(), String> {
        if body.len() > 32 << 20 {
            return Err("IRIS 请求超限".into());
        }
        let mut header = [0u8; 14];
        header[..4].copy_from_slice(&(body.len() as u32).to_le_bytes());
        header[4..8].copy_from_slice(&self.sequence.to_le_bytes());
        header[8..12].copy_from_slice(&self.statement.to_le_bytes());
        header[12..].copy_from_slice(&kind);
        self.sequence = self.sequence.wrapping_add(1);
        self.stream
            .write_all(&header)
            .and_then(|_| self.stream.write_all(body))
            .and_then(|_| self.stream.flush())
            .map_err(|_| "IRIS 请求发送失败".into())
    }
    fn receive(&mut self) -> Result<Message, String> {
        let mut header = [0u8; 14];
        self.stream
            .read_exact(&mut header)
            .map_err(|_| "IRIS 响应头读取失败")?;
        let length = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
        if length > 32 << 20 {
            return Err("IRIS 响应超过 32 MiB".into());
        }
        let mut body = vec![0; length];
        self.stream
            .read_exact(&mut body)
            .map_err(|_| "IRIS 响应读取失败")?;
        Ok(Message {
            status: i16::from_le_bytes([header[12], header[13]]),
            body,
        })
    }
    pub fn query(&mut self, text: &str, args: &[Value], limit: u32) -> Result<ResultSet, String> {
        let first = text.trim().trim_end_matches(';').to_ascii_uppercase();
        if matches!(first.as_str(), "COMMIT" | "ROLLBACK") {
            self.send(if first == "COMMIT" { *b"TC" } else { *b"TR" }, &[])?;
            let reply = self.receive()?;
            if reply.status != 0 {
                return Err("IRIS 事务结束失败".into());
            }
            return Ok(ResultSet::default());
        }
        let text = if matches!(first.as_str(), "BEGIN" | "BEGIN TRANSACTION") {
            "START TRANSACTION"
        } else {
            text
        };
        let mut text = text.to_string();
        if !args.is_empty() {
            text = bind_placeholders(&text, args.len())?;
        }
        let read = crate::oracle::is_query(&text);
        let mut body = Vec::new();
        let chunks = sql_chunks(&text);
        body.extend(encode(&json!(chunks.len()))?);
        for chunk in chunks {
            body.extend(encode(&json!(chunk))?);
        }
        body.extend(encode(&json!(args.len()))?);
        for _ in args {
            body.extend(encode(&json!(99))?);
            body.extend(encode(&json!(if read { 4 } else { 1 }))?);
        }
        if !read {
            body.extend(encode(&json!(""))?);
            body.extend(encode(&json!(0))?);
        }
        body.extend(encode(&json!(1))?);
        body.extend(encode(&json!(args.len()))?);
        for arg in args {
            body.extend(encode(&if arg == "" { json!("\0") } else { arg.clone() })?);
        }
        if read {
            body.extend(encode(&json!(self.timeout))?);
            body.extend(encode(&json!(limit.clamp(1, 1000)))?);
        }
        self.statement = self.statement.wrapping_add(1);
        self.send(if read { *b"DQ" } else { *b"DU" }, &body)?;
        let reply = self.receive()?;
        if reply.status != 0 && reply.status != 100 {
            return Err(format!("IRIS SQL 错误 {}", reply.status));
        }
        let mut metadata = Reader::new(&reply.body);
        if !read {
            let count = metadata.number()?.clamp(0, 10000);
            for _ in 0..count {
                for _ in 0..4 {
                    metadata.next()?;
                }
            }
            metadata.number()?;
            return Ok(ResultSet {
                rows_affected: metadata.number()?,
                ..Default::default()
            });
        }
        let feature = metadata.number()?;
        if feature == 2 {
            metadata.number()?;
        }
        let slots = if feature == 1 || feature == 2 {
            metadata.number()?.clamp(0, 4096) as usize
        } else {
            0
        };
        let count = metadata.number()?.clamp(0, 4096) as usize;
        let mut columns = Vec::new();
        let mut types = Vec::new();
        let mut positions = Vec::new();
        for index in 0..count {
            let name = metadata.text()?.replace('﹒', ".");
            let kind = metadata.number()?;
            for _ in 0..8 {
                metadata.next()?;
            }
            let position = if feature & 1 == 1 {
                metadata.number()?.checked_sub(1).ok_or("IRIS 列位置无效")? as usize
            } else {
                index
            };
            columns.push(Column {
                id: name.clone(),
                name,
            });
            types.push(kind);
            positions.push(position);
        }
        metadata.number()?;
        metadata.number()?;
        let mut result = ResultSet {
            columns,
            ..Default::default()
        };
        let mut budget = superlink_drivers::budget::ResultBudget::default();
        let mut packet = self.receive()?;
        loop {
            let mut data = Reader::new(&packet.body);
            while data.offset < data.bytes.len() {
                if limit > 0 && result.rows.len() >= limit as usize {
                    result.truncated = true;
                    return Ok(result);
                }
                let nested;
                let mut row_reader;
                if feature == 1 {
                    nested = data.next()?.data;
                    row_reader = Reader::new(nested);
                } else {
                    row_reader = Reader {
                        bytes: data.bytes,
                        offset: data.offset,
                    };
                }
                let mut items = Vec::new();
                for _ in 0..if feature == 1 { slots } else { count } {
                    items.push(row_reader.next()?);
                }
                if feature != 1 {
                    data.offset = row_reader.offset;
                }
                let values = positions
                    .iter()
                    .zip(&types)
                    .map(|(p, t)| {
                        items
                            .get(*p)
                            .ok_or("IRIS 列位置越界".into())
                            .and_then(|item| item.value(*t))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                if !budget.accept(&values) {
                    result.truncated = true;
                    return Ok(result);
                }
                result.rows.push(values);
            }
            if packet.status == 100 || packet.body.is_empty() {
                break;
            }
            self.send(*b"FD", &[])?;
            packet = self.receive()?;
            if packet.status != 0 && packet.status != 100 {
                return Err("IRIS 分块读取失败".into());
            }
        }
        Ok(result)
    }
    pub fn databases(&mut self) -> Result<Vec<String>, String> {
        if !self.namespace.is_empty() {
            return Ok(vec![self.namespace.clone()]);
        }
        Ok(crate::rows::list(self.query(
            "SELECT DISTINCT TABLE_CATALOG FROM INFORMATION_SCHEMA.TABLES",
            &[],
            10000,
        )?))
    }
}
fn bind_placeholders(text: &str, count: usize) -> Result<String, String> {
    let tokens = superlink_sql::tokenize(text);
    if tokens.len() > 10000 {
        return Err("绑定语句超过词法预算".into());
    }
    let mut index = 0;
    let mut result = String::with_capacity(text.len());
    for token in tokens {
        if token.kind == superlink_sql::TokenKind::Plain && token.text == "?" {
            index += 1;
            result.push_str(&format!(" :%qpar({index}) "));
        } else {
            result.push_str(&token.text);
        }
    }
    if index != count {
        return Err("绑定参数个数与占位符不匹配".into());
    }
    Ok(result)
}
fn sql_chunks(mut text: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    while !text.is_empty() {
        let mut end = text.len().min(31904);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        chunks.push(&text[..end]);
        text = &text[end..];
    }
    chunks
}
fn obfuscate(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = vec![0; bytes.len()];
    for (index, byte) in bytes.iter().enumerate() {
        let n = bytes.len() - index - 1;
        out[n] = ((*byte ^ 0xa7).wrapping_add(n as u8)).rotate_left(5);
    }
    out
}
impl Drop for Iris {
    fn drop(&mut self) {
        let _ = self.send(*b"DC", &[]);
    }
}

#[cfg(test)]
mod binding_tests {
    use super::*;
    #[test]
    fn never_binds_question_marks_inside_literals_or_comments() {
        let sql = bind_placeholders("SELECT '?' AS x,? /* ? */ -- ?\n", 1).unwrap();
        assert!(sql.contains("'?'"));
        assert!(sql.contains("/* ? */"));
        assert_eq!(sql.matches(":%qpar").count(), 1);
        assert!(bind_placeholders("SELECT ?", 2).is_err());
    }
    #[test]
    fn chinese_sql_chunks_preserve_utf8_boundaries_and_utf16_wire_items() {
        let sql = "SELECT 中文😀".repeat(4000);
        let chunks = sql_chunks(&sql);
        assert_eq!(chunks.concat(), sql);
        for chunk in chunks {
            let encoded = encode(&json!(chunk)).unwrap();
            let item = Reader::new(&encoded).next().unwrap();
            assert_eq!(item.kind, 2);
            assert_eq!(item.text().unwrap(), chunk);
        }
    }
}
