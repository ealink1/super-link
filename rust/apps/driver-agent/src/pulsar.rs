use futures_util::TryStreamExt;
use pulsar::{Authentication, Pulsar, TokioExecutor, consumer::ConsumerOptions, reader::Reader};
use serde_json::json;
use superlink_domain::{ConnectionConfig, ResultSet};
use superlink_drivers::QueryCancel;
pub struct PulsarDriver {
    client: Pulsar<TokioExecutor>,
    runtime: tokio::runtime::Runtime,
    cancel: QueryCancel,
    topic: String,
    namespace: String,
}
impl PulsarDriver {
    pub fn open(config: &ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|_| "Pulsar 运行时初始化失败")?;
        let address = if config.uri.starts_with("pulsar") {
            config.uri.clone()
        } else {
            format!(
                "{}://{}:{}",
                if config.use_ssl {
                    "pulsar+ssl"
                } else {
                    "pulsar"
                },
                config.host,
                if config.port == 0 { 6650 } else { config.port }
            )
        };
        let params: std::collections::HashMap<_, _> =
            url::form_urlencoded::parse(config.connection_params.as_bytes())
                .into_owned()
                .collect();
        let mut builder = Pulsar::builder(address, TokioExecutor);
        let topic = config
            .database
            .trim_start_matches("persistent://")
            .trim_start_matches("non-persistent://");
        let parts = topic.split('/').collect::<Vec<_>>();
        let tenant = params
            .get("tenant")
            .map(String::as_str)
            .unwrap_or(if parts.len() >= 3 { parts[0] } else { "public" });
        let namespace =
            params
                .get("namespace")
                .map(String::as_str)
                .unwrap_or(if parts.len() >= 3 {
                    parts[1]
                } else {
                    "default"
                });
        let namespace = format!("{tenant}/{namespace}");
        if !config.ssl_ca_path.is_empty() {
            builder = builder.with_certificate_chain(crate::tls::pem(&config.ssl_ca_path)?);
        }
        if let Some(token) = params.get("token") {
            builder = builder.with_auth(Authentication {
                name: "token".into(),
                data: token.as_bytes().to_vec(),
            });
        } else if !config.user.is_empty() {
            builder = builder.with_auth(Authentication {
                name: "basic".into(),
                data: json!({"username":config.user,"password":config.password})
                    .to_string()
                    .into_bytes(),
            });
        }
        let client = runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(config.timeout.max(1).into()),
                builder.build(),
            )
            .await
            .map_err(|_| "Pulsar 连接超时")?
            .map_err(|_| "Pulsar 连接或认证失败")
        })?;
        Ok(Self {
            client,
            runtime,
            cancel: cancel.clone(),
            topic: config.database.clone(),
            namespace,
        })
    }
    pub fn query(&mut self, text: &str, limit: u32) -> Result<ResultSet, String> {
        if text
            .trim()
            .trim_end_matches(';')
            .eq_ignore_ascii_case("SHOW TOPICS")
        {
            return Ok(crate::rows::json_rows(
                json!(
                    self.tables()?
                        .into_iter()
                        .map(|topic| json!({"topic":topic}))
                        .collect::<Vec<_>>()
                ),
                limit,
            ));
        }
        self.runtime.block_on(async{
            let work=async{
                if text.trim_start().starts_with('{'){
                    let c=crate::http::command(text)?;let topic=crate::http::target(&c,&["publish","topic"]).unwrap_or(&self.topic);if topic.is_empty(){return Err("Pulsar 命令缺少主题".into());}
                    let payload=c.get("value").or_else(||c.get("payload")).cloned().unwrap_or(serde_json::Value::Null);
                    let payload=payload.as_str().map(|s|s.as_bytes().to_vec()).unwrap_or_else(||payload.to_string().into_bytes());
                    if payload.len()>1<<20{return Err("消息超过 1 MiB".into());}
                    let mut producer=self.client.producer().with_topic(topic).build().await.map_err(|_|"Pulsar 生产者创建失败")?;
                    producer.send_non_blocking(payload).await.map_err(|_|"Pulsar 发布失败")?.await.map_err(|_|"Pulsar 发布确认失败")?;
                    producer.close().await.map_err(|_|"Pulsar 生产者关闭失败")?;return Ok(crate::rows::accepted());
                }
                let (topic,count,latest)=crate::services::message_read(text)?;let count=count.min(limit.max(1));
                let id=pulsar::proto::MessageIdData{ledger_id:if latest{u64::MAX}else{0},entry_id:if latest{u64::MAX}else{0},..Default::default()};
                let options=ConsumerOptions::default().durable(false).starting_on_message(id);
                let mut reader:Reader<Vec<u8>,_>=self.client.reader().with_topic(topic).with_consumer_name(format!("SuperLink-{}",std::process::id())).with_options(options).into_reader().await.map_err(|_|"Pulsar 预览 Reader 创建失败")?;
                let until=tokio::time::Instant::now()+std::time::Duration::from_secs(5);let mut rows=Vec::new();
                while rows.len()<(count as usize){match tokio::time::timeout_at(until,reader.try_next()).await{Ok(Ok(Some(message)))=>rows.push(json!({"payload":String::from_utf8_lossy(&message.payload.data),"messageId":format!("{:?}",message.message_id())})),Ok(Ok(None))|Err(_)=>break,Ok(Err(_))=>return Err("Pulsar 读取失败".into())}}
                Ok(crate::rows::json_rows(json!(rows),count))
            };
            tokio::select!{result=work=>result,_=crate::tls::cancelled(&self.cancel)=>Err("Pulsar 请求已停止".into())}
        })
    }
    pub fn tables(&self) -> Result<Vec<String>, String> {
        let namespace = &self.namespace;
        self.runtime.block_on(async {
            self.client
                .get_topics_of_namespace(
                    namespace.into(),
                    pulsar::proto::command_get_topics_of_namespace::Mode::All,
                )
                .await
                .map(|v| v.into_iter().take(10000).collect())
                .map_err(|_| "Pulsar 主题目录读取失败".into())
        })
    }
}
