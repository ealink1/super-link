use super::*;
use serde_json::json;
pub(super) struct NetworkFields {
    ssh: bool,
    proxy: bool,
    tunnel: bool,
    fields: std::collections::HashMap<&'static str, Entity<SqlEditor>>,
}
const FIELDS: [(&str, &str); 18] = [
    ("sshHost", "SSH 主机"),
    ("sshPort", "SSH 端口"),
    ("sshUser", "SSH 用户"),
    ("sshPassword", "SSH 密码"),
    ("sshKey", "SSH 私钥路径"),
    ("sshFingerprint", "SSH 主机指纹"),
    ("proxyType", "代理类型（socks5/http）"),
    ("proxyHost", "代理主机"),
    ("proxyPort", "代理端口"),
    ("proxyUser", "代理用户"),
    ("proxyPassword", "代理密码"),
    ("tunnelHost", "HTTP 隧道地址"),
    ("tunnelPort", "HTTP 隧道端口"),
    ("tunnelUser", "HTTP 隧道用户"),
    ("tunnelPassword", "HTTP 隧道密码"),
    ("uri", "连接 URI"),
    ("dsn", "自定义 DSN"),
    ("driver", "自定义驱动"),
];
impl NetworkFields {
    pub fn new(cx: &mut Context<ConnectionForm>) -> Self {
        let mut fields = std::collections::HashMap::new();
        for (key, label) in FIELDS {
            let editor = cx.new(SqlEditor::new);
            editor.update(cx, |e, cx| {
                e.set_placeholder(label, cx);
                e.set_masked(
                    key.ends_with("Password") || matches!(key, "uri" | "dsn"),
                    cx,
                );
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
                "uri" => "/uri",
                "dsn" => "/dsn",
                _ => "/driver",
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
    fn text(&self, key: &str, cx: &Context<ConnectionForm>) -> String {
        self.fields[key].read(cx).text().to_string()
    }
    pub fn apply(&self, profile: &mut Profile, cx: &Context<ConnectionForm>) -> Result<(), String> {
        let port = |key: &str, default: u16| -> Result<u16, String> {
            let text = self.text(key, cx);
            if text.trim().is_empty() {
                return Ok(default);
            }
            let port = text.parse::<u16>().map_err(|_| "网络端口必须是 1–65535")?;
            if port == 0 {
                return Err("网络端口不能为 0".into());
            }
            Ok(port)
        };
        let config = &mut profile.config;
        config.use_ssh = self.ssh;
        config.use_proxy = self.proxy;
        config.use_http_tunnel = self.tunnel;
        if self.ssh {
            let mut ssh = config.ssh.clone().unwrap_or_else(|| json!({}));
            let patch = json!({"host":self.text("sshHost",cx).trim(),"port":port("sshPort",22)?,"user":self.text("sshUser",cx).trim(),"password":self.text("sshPassword",cx),"keyPath":self.text("sshKey",cx).trim(),"hostKeyFingerprint":self.text("sshFingerprint",cx).trim()});
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
            config.proxy = Some(
                json!({"type":kind.trim(),"host":self.text("proxyHost",cx).trim(),"port":port("proxyPort",1080)?,"user":self.text("proxyUser",cx),"password":self.text("proxyPassword",cx)}),
            );
        }
        if self.tunnel {
            config.http_tunnel = Some(
                json!({"host":self.text("tunnelHost",cx).trim(),"port":port("tunnelPort",80)?,"user":self.text("tunnelUser",cx),"password":self.text("tunnelPassword",cx)}),
            );
        }
        config.uri = self.text("uri", cx).trim().into();
        config.dsn = self.text("dsn", cx);
        config.driver = self.text("driver", cx).trim().into();
        Ok(())
    }
}
impl ConnectionForm {
    pub(super) fn check(
        &self,
        id: &'static str,
        label: &'static str,
        on: bool,
        handler: impl Fn(&mut Self) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .text_color(theme.text)
            .text_size(px(13.0))
            .on_click(cx.listener(move |this, _, _, cx| {
                handler(this);
                cx.notify();
            }))
            .child(if on { "☑" } else { "☐" })
            .child(label)
            .into_any_element()
    }
    pub(super) fn network_view(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut panel = div().flex().flex_col().gap_2().child(self.check(
            "connection-ssh",
            "通过 SSH 隧道连接",
            self.network.ssh,
            |this| this.network.ssh = !this.network.ssh,
            cx,
        ));
        if self.network.ssh {
            for (key, label) in &FIELDS[..6] {
                panel = panel.child(self.row(label, &self.network.fields[key]));
            }
        }
        panel = panel.child(self.check(
            "connection-proxy",
            "使用网络代理",
            self.network.proxy,
            |this| this.network.proxy = !this.network.proxy,
            cx,
        ));
        if self.network.proxy {
            for (key, label) in &FIELDS[6..11] {
                panel = panel.child(self.row(label, &self.network.fields[key]));
            }
        }
        panel = panel.child(self.check(
            "connection-http-tunnel",
            "使用 HTTP 隧道",
            self.network.tunnel,
            |this| this.network.tunnel = !this.network.tunnel,
            cx,
        ));
        if self.network.tunnel {
            for (key, label) in &FIELDS[11..15] {
                panel = panel.child(self.row(label, &self.network.fields[key]));
            }
        }
        for (key, label) in &FIELDS[15..] {
            panel = panel.child(self.row(label, &self.network.fields[key]));
        }
        panel.into_any_element()
    }
}
