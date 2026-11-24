//! RocketMQ remoting protocol; preview requests never commit consumer offsets.
use base64::Engine;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha1::Sha1;
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};
use superlink_domain::{ConnectionConfig, ResultSet};
use superlink_drivers::QueryCancel;
trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}
pub struct RocketMq {
    config: ConnectionConfig,
    nameserver: String,
    cancel: QueryCancel,
    sequence: u32,
}
impl RocketMq {
    pub fn open(config: &ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        let nameserver = if let Some(host) = config.hosts.first() {
            host.clone()
        } else {
            format!(
                "{}:{}",
                config.host,
                if config.port == 0 { 9876 } else { config.port }
            )
        };
        Ok(Self {
            config: config.clone(),
            nameserver,
            cancel: cancel.clone(),
            sequence: 0,
        })
    }
    fn call(
        &mut self,
        address: &str,
        code: i32,
        mut fields: std::collections::BTreeMap<String, String>,
        body: Vec<u8>,
    ) -> Result<(Value, Vec<u8>), String> {
        let server_name = url::Url::parse(&format!("tcp://{address}"))
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .ok_or("RocketMQ 主机名无效")?;
        let address = address
            .to_socket_addrs()
            .map_err(|_| "RocketMQ 地址解析失败")?
            .next()
            .ok_or("RocketMQ 地址为空")?;
        let socket = TcpStream::connect_timeout(
            &address,
            Duration::from_secs(self.config.timeout.max(1).into()),
        )
        .map_err(|_| "RocketMQ 连接失败")?;
        socket
            .set_read_timeout(Some(Duration::from_secs(
                self.config.query_timeout.max(30).into(),
            )))
            .map_err(|_| "RocketMQ 超时设置失败")?;
        socket
            .set_write_timeout(Some(Duration::from_secs(10)))
            .map_err(|_| "RocketMQ 超时设置失败")?;
        let interrupt = socket.try_clone().map_err(|_| "RocketMQ 连接句柄失败")?;
        self.cancel.on_cancel(move || {
            let _ = interrupt.shutdown(std::net::Shutdown::Both);
        });
        let mut stream: Box<dyn Stream> = if self.config.use_ssl {
            let name = rustls::pki_types::ServerName::try_from(server_name)
                .map_err(|_| "RocketMQ TLS 主机名无效")?;
            let connection = rustls::ClientConnection::new(crate::tls::client(&self.config)?, name)
                .map_err(|_| "RocketMQ TLS 初始化失败")?;
            Box::new(rustls::StreamOwned::new(connection, socket))
        } else {
            Box::new(socket)
        };
        if !self.config.user.is_empty() {
            fields.insert("AccessKey".into(), self.config.user.clone());
            let mut content = fields
                .values()
                .flat_map(|v| v.as_bytes().iter().copied())
                .collect::<Vec<_>>();
            content.extend(&body);
            let mut mac = Hmac::<Sha1>::new_from_slice(self.config.password.as_bytes())
                .map_err(|_| "RocketMQ 签名初始化失败")?;
            mac.update(&content);
            fields.insert(
                "Signature".into(),
                base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()),
            );
        }
        self.sequence = self.sequence.wrapping_add(1);
        let header=serde_json::to_vec(&json!({"code":code,"language":"OTHER","version":373,"opaque":self.sequence,"flag":0,"extFields":fields})).map_err(|_|"RocketMQ 请求编码失败")?;
        let length = 4 + header.len() + body.len();
        if length > 32 << 20 {
            return Err("RocketMQ 请求超限".into());
        }
        stream
            .write_all(&(length as u32).to_be_bytes())
            .and_then(|_| stream.write_all(&(header.len() as u32).to_be_bytes()))
            .and_then(|_| stream.write_all(&header))
            .and_then(|_| stream.write_all(&body))
            .and_then(|_| stream.flush())
            .map_err(|_| "RocketMQ 发送失败")?;
        let mut raw = [0u8; 4];
        stream
            .read_exact(&mut raw)
            .map_err(|_| "RocketMQ 响应读取失败")?;
        let size = u32::from_be_bytes(raw) as usize;
        if !(4..=32 << 20).contains(&size) {
            return Err("RocketMQ 响应长度无效".into());
        }
        let mut raw = vec![0; size];
        stream
            .read_exact(&mut raw)
            .map_err(|_| "RocketMQ 响应中断")?;
        let mark = u32::from_be_bytes(raw[..4].try_into().unwrap());
        if mark >> 24 != 0 {
            return Err("RocketMQ 响应头编码不受支持".into());
        }
        let size = (mark & 0xffffff) as usize;
        if size > raw.len() - 4 {
            return Err("RocketMQ 响应头截断".into());
        }
        let header: Value =
            serde_json::from_slice(&raw[4..4 + size]).map_err(|_| "RocketMQ 响应头无效")?;
        if header["opaque"].as_u64() != Some(self.sequence.into()) {
            return Err("RocketMQ 请求标识不匹配".into());
        }
        Ok((header, raw[4 + size..].to_vec()))
    }
    fn route(&mut self, topic: &str) -> Result<Vec<(String, i32)>, String> {
        let address = self.nameserver.clone();
        let (header, body) = self.call(
            &address,
            105,
            fields([("topic", topic.to_string())]),
            vec![],
        )?;
        if header["code"] != 0 {
            return Err("RocketMQ 主题路由读取失败".into());
        }
        let route: Value = serde_json::from_slice(&body).map_err(|_| "RocketMQ 路由响应无效")?;
        let mut result = Vec::new();
        for broker in route["brokerDatas"]
            .as_array()
            .into_iter()
            .flatten()
            .take(100)
        {
            if let Some(address) = broker["brokerAddrs"]["0"].as_str() {
                let count = route["queueDatas"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|q| q["brokerName"] == broker["brokerName"])
                    .and_then(|q| q["readQueueNums"].as_i64())
                    .unwrap_or(1)
                    .clamp(1, 100);
                for id in 0..count {
                    result.push((address.into(), id as i32));
                }
            }
        }
        if result.is_empty() {
            return Err("RocketMQ 主题没有可用队列".into());
        }
        Ok(result)
    }
    pub fn tables(&mut self) -> Result<Vec<String>, String> {
        let address = self.nameserver.clone();
        let (header, body) = self.call(&address, 206, Default::default(), vec![])?;
        if header["code"] != 0 {
            return Err("RocketMQ 主题列表读取失败".into());
        }
        let value: Value = serde_json::from_slice(&body).map_err(|_| "RocketMQ 主题列表无效")?;
        Ok(crate::http::names(value["topicList"].clone(), &[]))
    }
    pub fn query(&mut self, text: &str, limit: u32) -> Result<ResultSet, String> {
        if text.trim_start().starts_with('{') {
            let c = crate::http::command(text)?;
            let topic = crate::http::target(&c, &["publish"])?;
            let route = self.route(topic)?;
            let (address, queue) = &route[0];
            let body = c["payload"].as_str().unwrap_or("").as_bytes().to_vec();
            if body.len() > 1 << 20 {
                return Err("消息超过 1 MiB".into());
            }
            let fields = fields([
                ("producerGroup", "SuperLink".into()),
                ("topic", topic.into()),
                ("defaultTopic", "TBW102".into()),
                ("defaultTopicQueueNums", "4".into()),
                ("queueId", queue.to_string()),
                ("sysFlag", "0".into()),
                (
                    "bornTimestamp",
                    chrono::Utc::now().timestamp_millis().to_string(),
                ),
                ("flag", "0".into()),
                ("properties", String::new()),
                ("reconsumeTimes", "0".into()),
                ("unitMode", "false".into()),
                ("batch", "false".into()),
            ]);
            let (header, _) = self.call(address, 10, fields, body)?;
            if header["code"] != 0 {
                return Err("RocketMQ 发布未成功确认".into());
            }
            return Ok(crate::rows::json_rows(header["extFields"].clone(), 1));
        }
        let (topic, count, latest) = crate::services::message_read(text)?;
        let count = count.min(limit.max(1));
        let mut messages = Vec::new();
        for (address, queue) in self.route(&topic)?.into_iter().take(100) {
            let common = fields([("topic", topic.clone()), ("queueId", queue.to_string())]);
            let (header, _) = self.call(&address, if latest { 30 } else { 31 }, common, vec![])?;
            if header["code"] != 0 {
                return Err("RocketMQ 位点读取失败".into());
            }
            let offset = header["extFields"]["offset"]
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .ok_or("RocketMQ 位点无效")?;
            let offset = if latest {
                offset.saturating_sub(i64::from(count)).max(0)
            } else {
                offset
            };
            let fields = fields([
                (
                    "consumerGroup",
                    format!("SuperLink-preview-{}", std::process::id()),
                ),
                ("topic", topic.clone()),
                ("queueId", queue.to_string()),
                ("queueOffset", offset.to_string()),
                ("maxMsgNums", (count as usize - messages.len()).to_string()),
                ("sysFlag", "4".into()),
                ("commitOffset", "0".into()),
                ("suspendTimeoutMillis", "1000".into()),
                ("subscription", "*".into()),
                (
                    "subVersion",
                    chrono::Utc::now().timestamp_millis().to_string(),
                ),
            ]);
            let (header, body) = self.call(&address, 11, fields, vec![])?;
            let code = header["code"].as_i64().unwrap_or(-1);
            if [19, 20, 21].contains(&code) {
                continue;
            }
            if code != 0 {
                return Err("RocketMQ 预览读取失败".into());
            }
            messages.extend(crate::rocketmq_decode::messages(
                &body,
                count as usize - messages.len(),
            )?);
            if messages.len() >= count as usize {
                break;
            }
        }
        Ok(crate::rows::json_rows(json!(messages), count))
    }
}
fn fields<const N: usize>(
    fields: [(&str, String); N],
) -> std::collections::BTreeMap<String, String> {
    fields.into_iter().map(|(k, v)| (k.into(), v)).collect()
}
