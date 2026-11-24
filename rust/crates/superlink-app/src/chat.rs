//! Compatible encrypted AI settings and bounded cancellable text-only chat.
use crate::QueryCancel;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex, OnceLock};
use superlink_infra::{LocalVault, Store};
const KEY: &str = "ai.chat.config.ref";
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ChatConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    #[serde(default)]
    pub stream: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}
pub struct ChatService {
    store: Arc<Store>,
    vault: Arc<LocalVault>,
    lock: Mutex<()>,
}
impl ChatConfig {
    pub fn validate(&self) -> Result<(), String> {
        let url = reqwest::Url::parse(&self.base_url).map_err(|_| "API 基础地址无效")?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.host_str().is_none()
        {
            return Err("API 地址不能包含账号、查询参数或片段".into());
        }
        let host = url.host_str().unwrap_or_default();
        let private = host.eq_ignore_ascii_case("localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| match ip {
                    std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
                    std::net::IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
                });
        if url.scheme() != "https" && !(url.scheme() == "http" && private) {
            return Err("远端 API 请使用 HTTPS；HTTP 仅限本机或私有 IP".into());
        }
        if self.base_url.len() > 2048
            || self.model.trim().is_empty()
            || self.model.len() > 128
            || self.api_key.len() > 4096
            || self.model.contains(['\r', '\n', '\0'])
            || self.api_key.contains(['\r', '\n', '\0'])
        {
            return Err("模型名称或 API Key 无效".into());
        }
        Ok(())
    }
}
impl ChatService {
    pub fn new(store: Arc<Store>, vault: Arc<LocalVault>) -> Self {
        Self {
            store,
            vault,
            lock: Mutex::new(()),
        }
    }
    pub fn load(&self) -> Result<ChatConfig, String> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let reference = self
            .store
            .setting(KEY)
            .map_err(|e| e.to_string())?
            .and_then(|v| v.as_str().map(str::to_owned));
        let Some(reference) = reference else {
            return Ok(ChatConfig::default());
        };
        let raw = self
            .vault
            .get(&reference)
            .map_err(|_| "AI 配置无法解密，请恢复工作区备份或重新配置")?;
        if raw.len() > 16 << 10 {
            return Err("AI 配置超过大小限制".into());
        }
        let config: ChatConfig =
            serde_json::from_slice(&raw).map_err(|_| "AI 配置损坏，请重新配置")?;
        config.validate()?;
        Ok(config)
    }
    pub fn save(&self, config: &ChatConfig) -> Result<(), String> {
        config.validate()?;
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let old = self.store.setting(KEY).map_err(|e| e.to_string())?;
        let reference = format!("local:v1:{}", uuid::Uuid::new_v4());
        let raw = serde_json::to_vec(config).map_err(|_| "AI 配置无法编码")?;
        self.vault
            .put(&reference, &raw)
            .map_err(|_| "AI 配置加密保存失败")?;
        if let Err(error) = self
            .store
            .set_setting(KEY, &Value::String(reference.clone()))
        {
            let _ = self.vault.delete(&reference);
            return Err(error.to_string());
        }
        if let Some(old) = old.and_then(|v| v.as_str().map(str::to_owned)) {
            let _ = self.vault.delete(&old);
        }
        Ok(())
    }
    pub fn reply(
        &self,
        config: &ChatConfig,
        messages: &[ChatMessage],
        cancel: &QueryCancel,
        on_text: impl Fn(&str),
    ) -> Result<String, String> {
        config.validate()?;
        validate_messages(messages)?;
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .expect("AI network runtime")
        });
        let _operation = crate::execution::Operation::new(cancel, 180);
        let (sender, mut cancelled) = tokio::sync::oneshot::channel();
        cancel.on_cancel(move || {
            let _ = sender.send(());
        });
        runtime.block_on(async{
            let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).connect_timeout(std::time::Duration::from_secs(10)).timeout(std::time::Duration::from_secs(180)).build().map_err(|_|"AI 网络客户端无法初始化")?;
            let base=config.base_url.trim_end_matches('/');let endpoint=if base.ends_with("/chat/completions"){base.to_string()}else{format!("{base}/chat/completions")};
            let mut request=client.post(endpoint).json(&serde_json::json!({"model":config.model,"messages":messages,"stream":config.stream}));if !config.api_key.is_empty(){request=request.bearer_auth(&config.api_key);}
            let mut response=tokio::select!{biased;_=&mut cancelled=>return Err("AI 请求已停止".into()),response=request.send()=>response.map_err(|_|"无法连接 AI 服务，请检查地址、网络和证书")?};
            if !response.status().is_success(){return Err(format!("AI 服务返回 HTTP {}，请检查接口、模型权限或额度",response.status().as_u16()));}
            let streaming=response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v|v.to_str().ok()).is_some_and(|s|s.to_ascii_lowercase().contains("text/event-stream"));
            let mut raw=Vec::new();let mut wire=0;let mut text=String::new();let mut completed=false;
            loop{
                let chunk=tokio::select!{biased;_=&mut cancelled=>return Err("AI 请求已停止".into()),chunk=response.chunk()=>chunk.map_err(|_|"AI 响应读取失败")?};
                let Some(chunk)=chunk else{break;};wire+=chunk.len();if wire>2<<20{return Err("AI 响应超过读取限制".into());}raw.extend_from_slice(&chunk);
                if streaming{
                    while let Some(end)=raw.iter().position(|b|*b==b'\n'){
                        let line=raw.drain(..=end).collect::<Vec<_>>();let line=std::str::from_utf8(&line).map_err(|_|"AI 响应不是有效 UTF-8")?.trim();
                        let Some(data)=line.strip_prefix("data:").map(str::trim)else{continue;};if data=="[DONE]"{completed=true;break;}
                        if data.is_empty(){continue;}let value:Value=serde_json::from_str(data).map_err(|_|"AI 流式响应无效")?;if value.get("error").is_some_and(|e|!e.is_null()){return Err("AI 服务报告请求失败".into());}
                        if let Some(choice)=value.get("choices").and_then(Value::as_array).and_then(|c|c.first()){
                            if let Some(part)=choice.pointer("/delta/content").and_then(Value::as_str){if text.len()+part.len()>32<<10{return Err("AI 回复超过 32 KiB 限制".into());}text.push_str(part);on_text(&text);}
                            completed|=choice.get("finish_reason").is_some_and(|r|!r.is_null());
                        }
                    }
                    if completed{break;}
                    if raw.len()>64<<10{return Err("AI 流式帧超过大小限制".into());}
                }
            }
            if !streaming{let value:Value=serde_json::from_slice(&raw).map_err(|_|"AI 响应格式无效")?;text=value.pointer("/choices/0/message/content").and_then(Value::as_str).ok_or("AI 没有返回文字回复")?.into();completed=true;}
            if !completed||text.trim().is_empty()||text.len()>32<<10{return Err("AI 回复不完整或超过大小限制，请重试".into());}Ok(text)
        })
    }
}
fn validate_messages(messages: &[ChatMessage]) -> Result<(), String> {
    if messages.is_empty() || messages.len() > 40 || messages.len() % 2 != 1 {
        return Err("对话消息数量超过限制，请开始新对话".into());
    }
    let mut bytes = 0;
    for (i, message) in messages.iter().enumerate() {
        let expected = if i % 2 == 0 { "user" } else { "assistant" };
        if message.role != expected
            || message.content.is_empty()
            || message.content.len()
                > if expected == "user" {
                    8 << 10
                } else {
                    32 << 10
                }
        {
            return Err("对话消息无效或超过长度限制".into());
        }
        bytes += message.content.len();
    }
    if bytes > 64 << 10 {
        return Err("对话超过上下文限制，请开始新对话".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_credentials_in_url_and_remote_plaintext() {
        for base_url in [
            "https://user:secret@example.test/v1",
            "http://example.test/v1",
            "https://example.test/v1?token=secret",
        ] {
            assert!(
                ChatConfig {
                    base_url: base_url.into(),
                    model: "model".into(),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            ChatConfig {
                base_url: "http://127.0.0.1:9000/v1".into(),
                model: "model".into(),
                ..Default::default()
            }
            .validate()
            .is_ok()
        );
    }
}
