use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use serde_json::json;
use superlink_domain::{ConnectionConfig, ResultSet};
use superlink_drivers::QueryCancel;
pub struct Mqtt {
    client: AsyncClient,
    events: EventLoop,
    runtime: tokio::runtime::Runtime,
    cancel: QueryCancel,
    topic: String,
    topics: Vec<String>,
}
impl Mqtt {
    pub fn open(config: &ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        let mut options = MqttOptions::new(
            format!("SuperLink-{}", std::process::id()),
            &config.host,
            if config.port == 0 { 1883 } else { config.port },
        );
        options
            .set_clean_session(true)
            .set_keep_alive(std::time::Duration::from_secs(30))
            .set_max_packet_size(1 << 20, 1 << 20);
        if !config.user.is_empty() {
            options.set_credentials(&config.user, &config.password);
        }
        if config.use_ssl {
            options.set_transport(rumqttc::Transport::tls_with_config(
                rumqttc::TlsConfiguration::Rustls(crate::tls::client(config)?),
            ));
        }
        let (client, mut events) = AsyncClient::new(options, 10);
        let params = url::form_urlencoded::parse(config.connection_params.as_bytes())
            .into_owned()
            .collect::<Vec<_>>();
        let topic = if config.database.is_empty() {
            params
                .iter()
                .find(|(k, _)| ["defaultTopic", "default_topic", "topic"].contains(&k.as_str()))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        } else {
            config.database.clone()
        };
        let mut topics = Vec::new();
        if !topic.is_empty() {
            topics.push(topic.clone());
        }
        for (key, value) in &params {
            if [
                "topics",
                "topicFilters",
                "topic_filters",
                "subscriptions",
                "subscription",
                "subscribe",
            ]
            .contains(&key.as_str())
            {
                for filter in value
                    .split([',', ';', '\n', '\r'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    if topics.len() >= 10000 {
                        return Err("MQTT 主题超过 10000".into());
                    }
                    if !topics.iter().any(|t| t == filter) {
                        topics.push(filter.into());
                    }
                }
            }
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "MQTT 运行时初始化失败")?;
        runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(config.timeout.max(1).into()),
                async {
                    loop {
                        if let Event::Incoming(Packet::ConnAck(_)) =
                            events.poll().await.map_err(|_| "MQTT 连接或认证失败")?
                        {
                            return Ok::<_, String>(());
                        }
                    }
                },
            )
            .await
            .map_err(|_| "MQTT 连接超时")?
        })?;
        Ok(Self {
            client,
            events,
            runtime,
            cancel: cancel.clone(),
            topic,
            topics,
        })
    }
    pub fn query(&mut self, text: &str, limit: u32) -> Result<ResultSet, String> {
        if text
            .trim()
            .trim_end_matches(';')
            .eq_ignore_ascii_case("SHOW TOPICS")
        {
            return Ok(crate::rows::json_rows(json!(self.topics.iter().map(|topic|json!({"topic":topic,"default":topic==&self.topic,"wildcard":topic.contains(['#','+'])})).collect::<Vec<_>>()),limit));
        }
        self.runtime.block_on(async{
            let work=async{
                if text.trim_start().starts_with('{'){
                    let c=crate::http::command(text)?;let topic=crate::http::target(&c,&["publish","topic"]).unwrap_or(&self.topic);if topic.is_empty(){return Err("MQTT 命令缺少主题".into());}
                    let qos=match c["qos"].as_u64().unwrap_or(0){0=>QoS::AtMostOnce,1=>QoS::AtLeastOnce,2=>QoS::ExactlyOnce,_=>return Err("MQTT QoS 无效".into())};
                    self.client.publish(topic,qos,c["retain"].as_bool().unwrap_or(false),c["payload"].as_str().unwrap_or("")).await.map_err(|_|"MQTT 发布入队失败")?;
                    loop{let event=self.events.poll().await.map_err(|_|"MQTT 发布失败")?;if matches!(event,Event::Outgoing(rumqttc::Outgoing::Publish(_)))&&qos==QoS::AtMostOnce||matches!(event,Event::Incoming(Packet::PubAck(_)|Packet::PubComp(_))){break;}}
                    return Ok(crate::rows::accepted());
                }
                let (topic,count,_)=crate::services::message_read(text)?;let count=count.min(limit.max(1));
                self.client.subscribe(&topic,QoS::AtMostOnce).await.map_err(|_|"MQTT 订阅失败")?;
                let until=tokio::time::Instant::now()+std::time::Duration::from_secs(5);let mut rows=Vec::new();
                while rows.len()<(count as usize){match tokio::time::timeout_at(until,self.events.poll()).await{Ok(Ok(Event::Incoming(Packet::Publish(p))))=>rows.push(json!({"topic":p.topic,"payload":String::from_utf8_lossy(&p.payload),"qos":format!("{:?}",p.qos),"retain":p.retain})),Ok(Ok(_))=>{},Ok(Err(_))=>return Err("MQTT 接收失败".into()),Err(_)=>break}}
                self.client.unsubscribe(topic).await.map_err(|_|"MQTT 取消订阅失败")?;
                tokio::time::timeout(std::time::Duration::from_secs(3),async{loop{if matches!(self.events.poll().await.map_err(|_|"MQTT 取消订阅确认失败")?,Event::Incoming(Packet::UnsubAck(_))){return Ok::<_,String>(());}}}).await.map_err(|_|"MQTT 取消订阅确认超时")??;
                Ok(crate::rows::json_rows(json!(rows),count))
            };
            tokio::select!{result=work=>result,_=crate::tls::cancelled(&self.cancel)=>Err("MQTT 请求已停止".into())}
        })
    }
    pub fn tables(&self) -> Vec<String> {
        self.topics.clone()
    }
}
