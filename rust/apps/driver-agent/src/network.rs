//! Connection-scoped Rust forwarding. No enabled transport may silently bypass its route.
use serde_json::{Value, json};
use superlink_domain::{ConnectionConfig, ShellHost};
use superlink_drivers::QueryCancel;
use superlink_terminal::{CancelToken, HostKeyPolicy, SshClient, TcpForward};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub struct Network {
    _ssh: Option<TcpForward>,
    proxy: Option<tokio::task::JoinHandle<()>>,
    _runtime: Option<tokio::runtime::Runtime>,
}
impl Network {
    pub fn prepare(config: &mut ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        if config.host.is_empty() {
            config.host = "127.0.0.1".into();
        }
        if config.port == 0 {
            config.port = superlink_domain::catalog::resolve(&config.kind)
                .ok()
                .and_then(|d| d.port)
                .unwrap_or(0);
        }
        let mut result = Self {
            _ssh: None,
            proxy: None,
            _runtime: None,
        };
        let routes = u8::from(config.use_ssh)
            + u8::from(config.use_proxy)
            + u8::from(config.use_http_tunnel);
        if routes == 0 {
            return Ok(result);
        }
        if !config.dsn.is_empty() {
            return Err("自定义 DSN 与网络隧道的组合尚未接入，连接已停止".into());
        }
        if routes > 1 {
            return Err("Rust 网络层暂不支持叠加 SSH、代理与 HTTP 隧道".into());
        }
        if crate::navicat::Navicat::configured(config) {
            return Ok(result);
        }
        if matches!(config.kind.as_str(), "sqlite" | "duckdb") {
            return Err("本地数据库不使用网络隧道".into());
        }
        if matches!(config.kind.as_str(), "kafka" | "rocketmq" | "pulsar")
            || config.hosts.len() > 1
            || config.topology == "cluster"
            || config.mongo_srv
            || !config.redis_sentinel_master.is_empty()
        {
            return Err("此拓扑需要动态节点转发，当前不能使用固定端口隧道".into());
        }
        let protocol = crate::sql::protocol_kind(config).to_string();
        let http = matches!(
            protocol.as_str(),
            "qdrant"
                | "chroma"
                | "milvus"
                | "nacos"
                | "rabbitmq"
                | "tdengine"
                | "trino"
                | "elasticsearch"
                | "clickhouse"
        );
        let tls = config.use_ssl
            || !matches!(
                config.ssl_mode.as_str(),
                "" | "disable" | "disabled" | "off"
            );
        if tls
            && !http
            && !matches!(
                protocol.as_str(),
                "postgres" | "gauss" | "dameng" | "sqlserver" | "iris" | "cache"
            )
        {
            return Err("此驱动的 TLS 隧道主机名校验尚未接入，连接已停止".into());
        }
        let mut target_host = config.host.clone();
        let mut target_port = config.port;
        if !config.uri.is_empty() {
            let uri = url::Url::parse(&config.uri).map_err(|_| "隧道连接 URI 无效")?;
            target_host = uri.host_str().ok_or("连接 URI 缺少主机")?.into();
            target_port = uri.port().unwrap_or(config.port);
        }
        let address = if config.use_ssh {
            let value = config.ssh.as_ref().ok_or("SSH 配置缺失")?;
            let host = ShellHost {
                name: "数据库 SSH".into(),
                host: text(value, "host").into(),
                port: port(value, 22)?,
                user: text(value, "user").into(),
                password: text(value, "password").into(),
                key_path: text(value, "keyPath").into(),
                passphrase: text(value, "passphrase").into(),
                ..Default::default()
            };
            let stop = CancelToken::new();
            let interrupt = stop.clone();
            cancel.on_cancel(move || interrupt.cancel());
            let client = SshClient::connect_transport(
                &host,
                HostKeyPolicy::Pinned(text(value, "hostKeyFingerprint").into()),
                &stop,
            )
            .map_err(|e| e.to_string())?;
            let tunnel = client
                .forward_tcp(target_host.clone(), target_port)
                .map_err(|e| e.to_string())?;
            let address = tunnel.address();
            result._ssh = Some(tunnel);
            address
        } else {
            let value = if config.use_proxy {
                config.proxy.as_ref()
            } else {
                config.http_tunnel.as_ref()
            }
            .ok_or("代理配置缺失")?;
            let mut value = value.clone();
            if config.use_http_tunnel {
                value["type"] = json!("http");
            }
            if text(&value, "host").contains("://") {
                return Err("脚本式 HTTP 隧道尚未迁移，连接已停止".into());
            }
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|_| "代理运行时初始化失败")?;
            let listener = runtime
                .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
                .map_err(|_| "代理监听失败")?;
            let address = listener.local_addr().map_err(|_| "代理监听地址无效")?;
            let target = (target_host.clone(), target_port);
            result.proxy=Some(runtime.spawn(async move {
                let mut tasks=tokio::task::JoinSet::new();loop{tokio::select!{
                    accepted=listener.accept(),if tasks.len()<16=>{let Ok((mut socket,_))=accepted else{break};let proxy=value.clone();let target=target.clone();tasks.spawn(async move{if let Ok(Ok(mut upstream))=tokio::time::timeout(std::time::Duration::from_secs(15),proxy_connect(&proxy,&target)).await{let _=tokio::io::copy_bidirectional(&mut socket,&mut upstream).await;}});},
                    _=tasks.join_next(),if !tasks.is_empty()=>{},
                }}
            }));
            result._runtime = Some(runtime);
            address
        };
        config
            .extra
            .insert("rustServerName".into(), json!(target_host));
        config
            .extra
            .insert("rustConnectAddress".into(), json!(address.to_string()));
        config.host = if http || matches!(protocol.as_str(), "postgres" | "gauss" | "sqlserver") {
            target_host
        } else {
            address.ip().to_string()
        };
        config.port = address.port();
        config.hosts.clear();
        if !config.uri.is_empty() {
            let mut uri = url::Url::parse(&config.uri).map_err(|_| "URI 无效")?;
            uri.set_host(Some(&config.host))
                .map_err(|_| "URI 主机无效")?;
            uri.set_port(Some(config.port))
                .map_err(|_| "URI 端口无效")?;
            config.uri = uri.into();
        }
        config.use_ssh = false;
        config.use_proxy = false;
        config.use_http_tunnel = false;
        Ok(result)
    }
}
impl Drop for Network {
    fn drop(&mut self) {
        if let Some(task) = self.proxy.take() {
            task.abort();
        }
    }
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn port(value: &Value, default: u16) -> Result<u16, String> {
    let p = value["port"].as_u64().unwrap_or(default.into());
    u16::try_from(p)
        .ok()
        .filter(|p| *p > 0)
        .ok_or("网络端口无效".into())
}
async fn proxy_connect(
    proxy: &Value,
    target: &(String, u16),
) -> Result<tokio::net::TcpStream, String> {
    let host = text(proxy, "host");
    if host.is_empty() || host.contains(['\r', '\n', '/']) {
        return Err("代理主机无效".into());
    }
    if target.0.contains(['\r', '\n', '/']) {
        return Err("目标主机无效".into());
    }
    let mut socket = tokio::net::TcpStream::connect((host, port(proxy, 1080)?))
        .await
        .map_err(|_| "代理连接失败")?;
    match text(proxy, "type") {
        "socks5" => {
            let user = text(proxy, "user").as_bytes();
            let password = text(proxy, "password").as_bytes();
            if user.len() > 255 || password.len() > 255 || target.0.len() > 255 {
                return Err("SOCKS5 参数过长".into());
            }
            socket
                .write_all(&[5, 1, if user.is_empty() { 0 } else { 2 }])
                .await
                .map_err(|_| "SOCKS5 握手失败")?;
            let mut reply = [0; 2];
            socket
                .read_exact(&mut reply)
                .await
                .map_err(|_| "SOCKS5 握手失败")?;
            if reply != [5, if user.is_empty() { 0 } else { 2 }] {
                return Err("SOCKS5 认证方法不匹配".into());
            }
            if !user.is_empty() {
                let mut auth = vec![1, user.len() as u8];
                auth.extend(user);
                auth.push(password.len() as u8);
                auth.extend(password);
                socket
                    .write_all(&auth)
                    .await
                    .map_err(|_| "SOCKS5 认证失败")?;
                socket
                    .read_exact(&mut reply)
                    .await
                    .map_err(|_| "SOCKS5 认证失败")?;
                if reply != [1, 0] {
                    return Err("SOCKS5 认证失败".into());
                }
            }
            let mut request = vec![5, 1, 0, 3, target.0.len() as u8];
            request.extend(target.0.as_bytes());
            request.extend(target.1.to_be_bytes());
            socket
                .write_all(&request)
                .await
                .map_err(|_| "SOCKS5 连接请求失败")?;
            let mut header = [0; 4];
            socket
                .read_exact(&mut header)
                .await
                .map_err(|_| "SOCKS5 响应失败")?;
            if header[0] != 5 || header[1] != 0 {
                return Err("SOCKS5 目标连接失败".into());
            }
            let length = match header[3] {
                1 => 4,
                4 => 16,
                3 => socket.read_u8().await.map_err(|_| "SOCKS5 响应截断")? as usize,
                _ => return Err("SOCKS5 地址类型无效".into()),
            };
            let mut address = vec![0; length + 2];
            socket
                .read_exact(&mut address)
                .await
                .map_err(|_| "SOCKS5 地址截断")?;
        }
        "http" => {
            use base64::Engine;
            let destination = if target.0.contains(':') {
                format!("[{}]:{}", target.0, target.1)
            } else {
                format!("{}:{}", target.0, target.1)
            };
            let auth = if text(proxy, "user").is_empty() {
                String::new()
            } else {
                format!(
                    "Proxy-Authorization: Basic {}\r\n",
                    base64::engine::general_purpose::STANDARD.encode(format!(
                        "{}:{}",
                        text(proxy, "user"),
                        text(proxy, "password")
                    ))
                )
            };
            socket
                .write_all(
                    format!("CONNECT {destination} HTTP/1.1\r\nHost: {destination}\r\n{auth}\r\n")
                        .as_bytes(),
                )
                .await
                .map_err(|_| "HTTP CONNECT 发送失败")?;
            let mut header = Vec::new();
            loop {
                if header.len() >= 8192 {
                    return Err("HTTP CONNECT 响应头过长".into());
                }
                header.push(
                    socket
                        .read_u8()
                        .await
                        .map_err(|_| "HTTP CONNECT 响应截断")?,
                );
                if header.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let header = std::str::from_utf8(&header).map_err(|_| "HTTP CONNECT 响应无效")?;
            if header
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                != Some("200")
            {
                return Err("HTTP CONNECT 目标连接失败".into());
            }
        }
        _ => return Err("代理类型必须为 socks5 或 http".into()),
    }
    Ok(socket)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn socks5_authenticates_and_sends_remote_dns_target() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut greeting = [0; 3];
                socket.read_exact(&mut greeting).await.unwrap();
                assert_eq!(greeting, [5, 1, 2]);
                socket.write_all(&[5, 2]).await.unwrap();
                let mut auth = [0; 5];
                socket.read_exact(&mut auth).await.unwrap();
                assert_eq!(&auth, b"\x01\x01u\x01p");
                socket.write_all(&[1, 0]).await.unwrap();
                let mut header = [0; 5];
                socket.read_exact(&mut header).await.unwrap();
                assert_eq!(&header[..4], &[5, 1, 0, 3]);
                let mut target = vec![0; header[4] as usize + 2];
                socket.read_exact(&mut target).await.unwrap();
                assert_eq!(&target[..target.len() - 2], b"private.invalid");
                assert_eq!(&target[target.len() - 2..], &5432u16.to_be_bytes());
                socket
                    .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
                    .await
                    .unwrap();
                socket.write_all(b"ok").await.unwrap();
            });
            let mut socket = proxy_connect(
                &json!({"type":"socks5","host":"127.0.0.1","port":port,"user":"u","password":"p"}),
                &("private.invalid".into(), 5432),
            )
            .await
            .unwrap();
            let mut reply = [0; 2];
            socket.read_exact(&mut reply).await.unwrap();
            assert_eq!(&reply, b"ok");
            server.await.unwrap();
        });
    }
    #[test]
    fn script_tunnels_preserve_the_endpoint_and_stacked_routes_are_rejected() {
        let mut config = ConnectionConfig {
            kind: "mysql".into(),
            use_http_tunnel: true,
            http_tunnel: Some(json!({"host":"https://example.invalid/tunnel.php"})),
            ..Default::default()
        };
        assert!(Network::prepare(&mut config, &QueryCancel::new()).is_ok());
        assert!(crate::navicat::Navicat::configured(&config));
        config.use_ssh = true;
        assert!(Network::prepare(&mut config, &QueryCancel::new()).is_err());
    }
}
