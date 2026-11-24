use crate::{Output, catalog, protocol, sql::Sql};
use serde_json::{Value, json};
use superlink_domain::ConnectionConfig;
use superlink_infra::QueryCancel;

pub struct Runtime {
    kind: String,
    driver: Option<Sql>,
    cancel: QueryCancel,
    session: bool,
    network: Option<crate::network::Network>,
}
impl Runtime {
    pub fn new(kind: String, cancel: QueryCancel) -> Self {
        Self {
            kind,
            driver: None,
            cancel,
            session: false,
            network: None,
        }
    }
    pub fn handle(&mut self, request: &Value, output: &Output) -> Result<Value, String> {
        let method = request["method"].as_str().ok_or("请求缺少方法")?;
        if method == "metadata" {
            return Ok(
                json!({"data":{"driverType":self.kind,"agentRevision":concat!("rust-",env!("CARGO_PKG_VERSION")),"protocolSchema":"json-lines-v3-rust","implementation":"rust"}}),
            );
        }
        self.cancel.begin();
        let _deadline = Deadline::new(
            self.cancel.clone(),
            request["timeoutMs"].as_u64().unwrap_or(0),
        );
        if method == "connect" {
            let mut config: ConnectionConfig =
                serde_json::from_value(request["config"].clone()).map_err(|_| "连接配置无效")?;
            if config.kind != self.kind {
                return Err("驱动与连接类型不匹配".into());
            }
            crate::connection::normalize(&mut config)?;
            if config.timeout == 0 {
                config.timeout = 10;
            }
            self.driver = None;
            self.network = None;
            let network = crate::network::Network::prepare(&mut config, &self.cancel)?;
            self.driver = Some(Sql::open(&config, &self.cancel)?);
            self.network = Some(network);
            return Ok(
                json!({"data":{"protocolSchema":"json-lines-v3-rust","typedArguments":true,"inFlightCancel":true}}),
            );
        }
        if method == "close" {
            self.driver = None;
            self.network = None;
            self.session = false;
            return Ok(json!({"data":null}));
        }
        let driver = self.driver.as_mut().ok_or("尚未连接")?;
        if let Some(id) = request["sessionId"].as_str()
            && (!self.session || id != "rust-session")
        {
            return Err("SQL 会话已失效".into());
        }
        if method == "streamQuery" {
            return crate::streaming::query(driver, request, output);
        }
        match method {
            "openSession" => {
                if !driver.transactional() {
                    return Err("不支持 SQL 编辑器托管事务".into());
                }
                self.session = true;
                Ok(json!({"data":"rust-session"}))
            }
            "closeSession" => {
                if self.session {
                    let _ = driver.query("ROLLBACK", &[], 1);
                }
                self.session = false;
                Ok(json!({"data":null}))
            }
            "executeElasticsearchConsoleRequest" => {
                let Sql::Rest(rest) = driver else {
                    return Err("此连接不是 Elasticsearch".into());
                };
                Ok(json!({"data":rest.elasticsearch_console(request)?}))
            }
            "query" | "exec" => {
                let args = protocol::arguments(request)?;
                let limit = request["rowBudget"]["maxRowsPerResult"]
                    .as_u64()
                    .unwrap_or(1000)
                    .min(u32::MAX as u64) as u32;
                let text = request["query"].as_str().ok_or("查询无效")?;
                let result = driver.query(text, &args, limit)?;
                Ok(protocol::result(result))
            }
            _ => Ok(json!({"data":catalog::request(driver,request)?})),
        }
    }
}
struct Deadline {
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    cancel: QueryCancel,
}
impl Deadline {
    fn new(cancel: QueryCancel, millis: u64) -> Self {
        let mut result = Self {
            stop: None,
            thread: None,
            cancel: cancel.clone(),
        };
        if millis > 0 {
            let (sender, receiver) = std::sync::mpsc::channel();
            result.stop = Some(sender);
            result.thread = Some(std::thread::spawn(move || {
                if receiver
                    .recv_timeout(std::time::Duration::from_millis(millis))
                    .is_err()
                {
                    cancel.cancel();
                }
            }));
        }
        result
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.cancel.finish();
    }
}
