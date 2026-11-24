use rskafka::{
    client::{
        Client, ClientBuilder, Credentials, SaslConfig,
        partition::{OffsetAt, UnknownTopicHandling},
    },
    record::Record,
};
use serde_json::{Value, json};
use superlink_domain::{ConnectionConfig, ResultSet};
use superlink_drivers::QueryCancel;
pub struct Kafka {
    client: Client,
    runtime: tokio::runtime::Runtime,
    cancel: QueryCancel,
}
impl Kafka {
    pub fn open(config: &ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "Kafka 运行时初始化失败")?;
        let hosts = if config.hosts.is_empty() {
            vec![format!(
                "{}:{}",
                config.host,
                if config.port == 0 { 9092 } else { config.port }
            )]
        } else {
            config.hosts.clone()
        };
        let mut builder = ClientBuilder::new(hosts)
            .client_id("SuperLink")
            .max_message_size(32 << 20);
        if !config.user.is_empty() {
            let credentials = Credentials {
                username: config.user.clone(),
                password: config.password.clone(),
            };
            let params: std::collections::HashMap<_, _> =
                url::form_urlencoded::parse(config.connection_params.as_bytes())
                    .into_owned()
                    .collect();
            let mechanism = params
                .get("saslMechanism")
                .or_else(|| params.get("sasl.mechanism"))
                .map(|v| v.to_ascii_uppercase())
                .unwrap_or_else(|| "PLAIN".into());
            builder = builder.sasl_config(match mechanism.as_str() {
                "PLAIN" => SaslConfig::Plain(credentials),
                "SCRAM-SHA-256" => SaslConfig::ScramSha256(credentials),
                "SCRAM-SHA-512" => SaslConfig::ScramSha512(credentials),
                _ => return Err("Kafka SASL 机制无效".into()),
            });
        }
        if config.use_ssl {
            builder = builder.tls_config(crate::tls::client(config)?);
        }
        let client = runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(config.timeout.max(1).into()),
                builder.build(),
            )
            .await
            .map_err(|_| "Kafka 连接超时")?
            .map_err(|_| "Kafka 连接或认证失败")
        })?;
        Ok(Self {
            client,
            runtime,
            cancel: cancel.clone(),
        })
    }
    pub fn query(&mut self, text: &str, limit: u32) -> Result<ResultSet, String> {
        self.runtime.block_on(async{
            let work=async {
                if text.trim_start().starts_with('{'){
                    let c=crate::http::command(text)?;let topic=crate::http::target(&c,&["publish"])?;
                    let partition=i32::try_from(c["partition"].as_i64().unwrap_or(0)).ok().filter(|n|*n>=0).ok_or("Kafka 分区无效")?;
                    let client=self.client.partition_client(topic,partition,UnknownTopicHandling::Error).await.map_err(|_|"Kafka 分区不可用")?;
                    let payload=c.get("value").or_else(||c.get("payload")).cloned().unwrap_or(Value::Null);
                    let value=payload.as_str().map(|s|s.as_bytes().to_vec()).unwrap_or_else(||payload.to_string().into_bytes());
                    if value.len()>1<<20{return Err("消息超过 1 MiB".into());}
                    let offsets=client.produce(vec![Record{key:c["key"].as_str().map(|s|s.as_bytes().to_vec()),value:Some(value),headers:Default::default(),timestamp:chrono::Utc::now()}],Default::default()).await.map_err(|_|"Kafka 发布失败")?;
                    return Ok(crate::rows::json_rows(json!({"offsets":offsets}),1));
                }
                let (topic,count,latest)=crate::services::message_read(text)?;let count=count.min(limit.max(1)) as usize;
                let topics=self.client.list_topics().await.map_err(|_|"Kafka 主题读取失败")?;
                let topic_info=topics.iter().find(|t|t.name==topic).ok_or("Kafka 主题不存在")?;
                let mut messages=Vec::new();
                for partition in topic_info.partitions.iter().take(100){
                    let client=self.client.partition_client(topic.clone(),*partition,UnknownTopicHandling::Error).await.map_err(|_|"Kafka 分区读取失败")?;
                    let start=client.get_offset(if latest{OffsetAt::Latest}else{OffsetAt::Earliest}).await.map_err(|_|"Kafka 位点读取失败")?;
                    let earliest=client.get_offset(OffsetAt::Earliest).await.map_err(|_|"Kafka 最早位点读取失败")?;
                    let offset=if latest{(start-count as i64).max(earliest)}else{start};
                    let (records,_)=client.fetch_records(offset,1..(1<<20),1000).await.map_err(|_|"Kafka 消息读取失败")?;
                    for record in records.into_iter().take(count-messages.len()){
                        let r=record.record;messages.push(json!({"partition":partition,"offset":record.offset,"timestamp":r.timestamp.to_rfc3339(),"key":r.key.map(|b|String::from_utf8_lossy(&b).into_owned()),"value":r.value.map(|b|String::from_utf8_lossy(&b).into_owned())}));
                    }if messages.len()>=count{break;}
                }Ok(crate::rows::json_rows(json!(messages),count as u32))
            };
            tokio::select!{result=work=>result,_=crate::tls::cancelled(&self.cancel)=>Err("Kafka 请求已停止".into())}
        })
    }
    pub fn tables(&self) -> Result<Vec<String>, String> {
        self.runtime.block_on(async {
            self.client
                .list_topics()
                .await
                .map(|v| v.into_iter().take(10000).map(|t| t.name).collect())
                .map_err(|_| "Kafka 主题读取失败".into())
        })
    }
}
