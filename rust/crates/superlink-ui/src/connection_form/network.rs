//! 「网络与安全」页的字段：TLS、SSH 隧道、代理与 HTTP 隧道。
//!
//! 与 Fyne 版一致：三组开关各自控制明细字段是否出现，未启用时保留原值
//! 但不覆盖配置；known_hosts 与主机指纹属于 SSH 明细。

use super::*;
use serde_json::json;

pub(crate) struct NetworkFields {
    pub ssh: bool,
    pub proxy: bool,
    pub tunnel: bool,
    fields: std::collections::HashMap<&'static str, Entity<SqlEditor>>,
}

const FIELDS: [(&str, &str); 16] = [
    ("sshHost", "SSH 主机"),
    ("sshPort", "端口"),
    ("sshUser", "用户名"),
    ("sshPassword", "密码"),
    ("sshKey", "私钥路径"),
    ("sshKnownHosts", "known_hosts 文件"),
    ("sshFingerprint", "主机指纹"),
    ("proxyType", "代理类型（socks5/http）"),
    ("proxyHost", "主机"),
    ("proxyPort", "端口"),
    ("proxyUser", "用户名"),
    ("proxyPassword", "密码"),
    ("tunnelHost", "HTTP 隧道地址"),
    ("tunnelPort", "HTTP 隧道端口"),
    ("tunnelUser", "HTTP 隧道用户"),
    ("tunnelPassword", "HTTP 隧道密码"),
];

/// 三组明细各自包含的字段键。
pub(super) const SSH_FIELDS: &[&str] = &[
    "sshHost",
    "sshPort",
    "sshUser",
    "sshPassword",
    "sshKey",
    "sshKnownHosts",
    "sshFingerprint",
];
pub(super) const PROXY_FIELDS: &[&str] = &[
    "proxyType",
    "proxyHost",
    "proxyPort",
    "proxyUser",
    "proxyPassword",
];
pub(super) const TUNNEL_FIELDS: &[&str] =
    &["tunnelHost", "tunnelPort", "tunnelUser", "tunnelPassword"];

impl NetworkFields {
    pub fn new(cx: &mut Context<ConnectionForm>) -> Self {
        let mut fields = std::collections::HashMap::new();
        for (key, label) in FIELDS {
            let editor = cx.new(SqlEditor::new);
            editor.update(cx, |e, cx| {
                e.set_placeholder(label, cx);
                e.set_masked(key.ends_with("Password"), cx);
            });
            fields.insert(key, editor);
        }
        Self {
            ssh: false,
            proxy: false,
            tunnel: false,
            fields,
        }
    }

    pub fn load(
        &mut self,
        config: &superlink_domain::ConnectionConfig,
        cx: &mut Context<ConnectionForm>,
    ) {
        self.ssh = config.use_ssh;
        self.proxy = config.use_proxy;
        self.tunnel = config.use_http_tunnel;
        let object = serde_json::to_value(config).unwrap_or_default();
        for (key, _) in FIELDS {
            let path = match key {
                "sshHost" => "/ssh/host",
                "sshPort" => "/ssh/port",
                "sshUser" => "/ssh/user",
                "sshPassword" => "/ssh/password",
                "sshKey" => "/ssh/keyPath",
                "sshKnownHosts" => "/ssh/knownHostsPath",
                "sshFingerprint" => "/ssh/hostKeyFingerprint",
                "proxyType" => "/proxy/type",
                "proxyHost" => "/proxy/host",
                "proxyPort" => "/proxy/port",
                "proxyUser" => "/proxy/user",
                "proxyPassword" => "/proxy/password",
                "tunnelHost" => "/httpTunnel/host",
                "tunnelPort" => "/httpTunnel/port",
                "tunnelUser" => "/httpTunnel/user",
                "tunnelPassword" => "/httpTunnel/password",
                _ => "/ssh/hostKeyFingerprint",
            };
            let text = object
                .pointer(path)
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string())
                })
                .unwrap_or_default();
            self.fields[key].update(cx, |e, cx| e.set_text(text, cx));
        }
    }

    pub fn clear(&mut self, cx: &mut Context<ConnectionForm>) {
        self.ssh = false;
        self.proxy = false;
        self.tunnel = false;
        let keys: Vec<&'static str> = self.fields.keys().copied().collect();
        for key in keys {
            self.fields[key].update(cx, |e, cx| e.set_text(String::new(), cx));
        }
    }

    pub(super) fn editor(&self, key: &str) -> Entity<SqlEditor> {
        self.fields[key].clone()
    }

    pub(super) fn text(&self, key: &str, cx: &Context<ConnectionForm>) -> String {
        self.fields[key].read(cx).text().to_string()
    }

    pub(super) fn label(key: &str) -> &'static str {
        FIELDS
            .iter()
            .find(|(field, _)| *field == key)
            .map(|(_, label)| *label)
            .unwrap_or("")
    }

    fn port(text: &str, default: u16) -> Result<u16, String> {
        if text.trim().is_empty() {
            return Ok(default);
        }
        match text.trim().parse::<u32>() {
            Ok(value) if (1..=65535).contains(&value) => Ok(value as u16),
            _ => Err("网络端口必须是 1–65535".into()),
        }
    }

    /// 只有启用中的组合才写回配置；关闭的组保留原值以免丢失凭据路径。
    pub fn apply(&self, profile: &mut Profile, cx: &Context<ConnectionForm>) -> Result<(), String> {
        let config = &mut profile.config;
        config.use_ssh = self.ssh;
        config.use_proxy = self.proxy;
        config.use_http_tunnel = self.tunnel;
        if self.ssh {
            let mut ssh = config.ssh.clone().unwrap_or_else(|| json!({}));
            let patch = json!({
                "host": self.text("sshHost", cx).trim(),
                "port": port_of(self.text("sshPort", cx), 22)?,
                "user": self.text("sshUser", cx).trim(),
                "password": self.text("sshPassword", cx),
                "keyPath": self.text("sshKey", cx).trim(),
                "knownHostsPath": self.text("sshKnownHosts", cx).trim(),
                "hostKeyFingerprint": self.text("sshFingerprint", cx).trim(),
            });
            superlink_domain::secrets::merge(
                ssh.as_object_mut().ok_or("SSH 配置无效")?,
                patch.as_object().unwrap(),
            );
            config.ssh = Some(ssh);
        }
        if self.proxy {
            let kind = self.text("proxyType", cx);
            if !matches!(kind.trim(), "http" | "socks5") {
                return Err("代理类型应为 http 或 socks5".into());
            }
            config.proxy = Some(json!({
                "type": kind.trim(),
                "host": self.text("proxyHost", cx).trim(),
                "port": port_of(self.text("proxyPort", cx), 1080)?,
                "user": self.text("proxyUser", cx),
                "password": self.text("proxyPassword", cx),
            }));
        }
        if self.tunnel {
            config.http_tunnel = Some(json!({
                "host": self.text("tunnelHost", cx).trim(),
                "port": port_of(self.text("tunnelPort", cx), 80)?,
                "user": self.text("tunnelUser", cx),
                "password": self.text("tunnelPassword", cx),
            }));
        }
        Ok(())
    }
}

fn port_of(text: String, default: u16) -> Result<u16, String> {
    NetworkFields::port(&text, default)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 视图按组字段键取值，before 在旧版里因 `for (_, editor)` 把标签当键而崩溃。
    #[test]
    fn every_group_field_is_declared_once_with_a_label() {
        let keys: Vec<&str> = FIELDS.iter().map(|(key, _)| *key).collect();
        let mut unique = keys.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), keys.len(), "字段键重复");
        for (key, label) in FIELDS {
            assert!(!label.is_empty(), "{key} 缺少标签");
        }
        for key in SSH_FIELDS.iter().chain(PROXY_FIELDS).chain(TUNNEL_FIELDS) {
            assert!(keys.contains(key), "分组引用了未声明的字段 {key}");
        }
    }
}
