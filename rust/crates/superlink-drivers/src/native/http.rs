//! Shared bounded HTTP transport; cancelling drops the in-flight request/response.
use crate::{DriverError, QueryCancel};
use std::{path::Path, sync::OnceLock, time::Duration};
use superlink_domain::ConnectionConfig;

pub struct Http {
    client: reqwest::Client,
    cancel: QueryCancel,
}
impl Http {
    pub fn new(config: &ConnectionConfig) -> Result<Self, DriverError> {
        super::install_crypto_provider();
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(if config.timeout == 0 {
                10
            } else {
                config.timeout.into()
            }));
        if config.query_timeout > 0 {
            builder = builder.timeout(Duration::from_secs(config.query_timeout.into()));
        }
        if let Some(address) = config
            .extra
            .get("rustConnectAddress")
            .and_then(serde_json::Value::as_str)
        {
            let address = address
                .parse()
                .map_err(|_| DriverError::Native("HTTP 转发地址无效".into()))?;
            builder = builder.resolve(&config.host, address);
        }
        if !config.ssl_ca_path.is_empty() {
            let bytes = read_pem(&config.ssl_ca_path)?;
            for certificate in reqwest::Certificate::from_pem_bundle(&bytes)
                .map_err(|_| DriverError::Native("CA 证书无效".into()))?
            {
                builder = builder.add_root_certificate(certificate);
            }
        }
        if config.ssl_cert_path.is_empty() != config.ssl_key_path.is_empty() {
            return Err(DriverError::Native("客户端证书和私钥必须同时填写".into()));
        }
        if !config.ssl_cert_path.is_empty() {
            let mut pem = read_pem(&config.ssl_cert_path)?;
            pem.push(b'\n');
            pem.extend(read_pem(&config.ssl_key_path)?);
            builder = builder.identity(
                reqwest::Identity::from_pem(&pem)
                    .map_err(|_| DriverError::Native("客户端证书或私钥无效".into()))?,
            );
        }
        Ok(Self {
            client: builder
                .build()
                .map_err(|_| DriverError::Native("HTTP 客户端初始化失败".into()))?,
            cancel: QueryCancel::new(),
        })
    }
    pub fn set_cancellation(&mut self, cancel: &QueryCancel) {
        self.cancel = cancel.clone();
    }
    pub fn post(&self, url: &str) -> reqwest::RequestBuilder {
        self.client.post(url)
    }
    pub fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.client.request(method, url)
    }
    pub fn json(&self, request: reqwest::RequestBuilder) -> Result<serde_json::Value, DriverError> {
        let bytes = self.bytes(request)?;
        if bytes.is_empty() || bytes == b"Ok.\n" {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| DriverError::Native("HTTP 响应不是有效 JSON".into()))
    }
    pub fn bytes(&self, request: reqwest::RequestBuilder) -> Result<Vec<u8>, DriverError> {
        let (status, bytes) = self.response(request)?;
        if !(200..300).contains(&status) {
            return Err(DriverError::Native(format!("服务端返回 HTTP {status}")));
        }
        Ok(bytes)
    }
    pub fn response(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<(u16, Vec<u8>), DriverError> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.cancel.on_cancel(move || {
            let _ = sender.send(());
        });
        static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
        let runtime = RUNTIME
            .get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .map_err(|_| "无法启动 HTTP 请求".into())
            })
            .as_ref()
            .map_err(|e| DriverError::Native(e.clone()))?;
        let outcome=runtime.block_on(async {
            tokio::select! {biased;_=receiver=>Err(DriverError::Native("HTTP 请求已停止".into())),result=async {
                let mut response=request.send().await.map_err(|_|DriverError::Native("HTTP 连接或请求失败".into()))?;
                let status = response.status().as_u16();
                let mut bytes=Vec::new();
                while let Some(chunk)=response.chunk().await.map_err(|_|DriverError::Native("HTTP 响应中断".into()))?{
                    if bytes.len()+chunk.len()>32<<20{return Err(DriverError::Native("HTTP 响应超过 32 MiB".into()));}bytes.extend_from_slice(&chunk);
                }
                Ok((status, bytes))
            }=>result}
        });
        self.cancel.clear();
        outcome
    }
}
fn read_pem(path: &str) -> Result<Vec<u8>, DriverError> {
    let path = Path::new(path);
    let metadata =
        std::fs::metadata(path).map_err(|_| DriverError::Native("无法读取 TLS 文件".into()))?;
    if !metadata.is_file() || metadata.len() > 1 << 20 {
        return Err(DriverError::Native(
            "TLS 文件超过 1 MiB 或不是普通文件".into(),
        ));
    }
    std::fs::read(path).map_err(|_| DriverError::Native("无法读取 TLS 文件".into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_aborts_a_waiting_http_response() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = [0; 4096];
            while socket.read(&mut bytes).is_ok_and(|n| n > 0) {}
        });
        let cancel = QueryCancel::new();
        let mut http = Http::new(&ConnectionConfig::default()).unwrap();
        http.set_cancellation(&cancel);
        let request = http.post(&format!("http://{address}/")).body("SELECT 1");
        let trigger = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            cancel.cancel();
        });
        let started = std::time::Instant::now();
        assert!(http.json(request).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        trigger.join().unwrap();
        worker.join().unwrap();
    }
}
