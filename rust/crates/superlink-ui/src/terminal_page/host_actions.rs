use super::*;
use std::collections::BTreeSet;
use superlink_app::HostCredentials;
type HostAction = fn(&mut TerminalPage, usize, &mut Context<TerminalPage>);

fn tags(text: &str) -> BTreeSet<String> {
    text.split([',', '，', ';', '；', '\n'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

fn matches_host(host: &ShellHost, search: &str, group: &str, tag: &str) -> bool {
    let needle = search.trim().to_lowercase();
    (group.is_empty() || group_label(&host.group) == group)
        && (tag.is_empty() || tags(&host.tags).contains(tag))
        && (needle.is_empty()
            || [
                &host.name,
                &host.host,
                &host.user,
                &host.notes,
                &host.tags,
                &host.group,
                &host.operating_system,
            ]
            .iter()
            .any(|s| s.to_lowercase().contains(&needle)))
}

impl TerminalPage {
    pub(super) fn host_matches(&self, host: &ShellHost) -> bool {
        matches_host(host, &self.filter, &self.group, &self.tag)
    }
    pub(super) fn host_tags(&self) -> BTreeSet<String> {
        self.host_list.iter().flat_map(|h| tags(&h.tags)).collect()
    }
    pub(super) fn host_menu_button(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .id(("host-menu", index))
            .size(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .hover(move |s| s.bg(theme.accent_soft))
            .child(icons::icon("shell/ellipsis.svg", 16., theme.muted))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.host_menu = if this.host_menu == Some(index) {
                    None
                } else {
                    Some(index)
                };
                cx.notify();
            }))
            .into_any_element()
    }
    pub(super) fn host_actions_menu(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let rows: [(&str, HostAction); 4] = [
            ("编辑主机", |this, i, cx| this.edit_host(i, cx)),
            ("测试连接", |this, i, cx| {
                this.edit_host(i, cx);
                this.test_host(false, cx);
            }),
            ("复制密码", Self::copy_host_password),
            ("删除主机", |this, i, cx| {
                this.edit_host(i, cx);
                this.confirm_host_delete = true;
            }),
        ];
        div()
            .absolute()
            .top(px(40.))
            .right(px(12.))
            .w(px(170.))
            .p_1()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .occlude()
            .children(rows.into_iter().enumerate().map(|(i, (label, action))| {
                div()
                    .id(("host-action", i))
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(if i == 3 { theme.danger } else { theme.text })
                    .hover(move |s| s.bg(theme.accent_soft))
                    .child(label.to_string())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.host_menu = None;
                        action(this, index, cx);
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }
    fn copy_host_password(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(host) = self.host_list.get(index).cloned() else {
            return;
        };
        let hosts = self.hosts.clone();
        let task = cx
            .background_executor()
            .spawn(async move { hosts.credentials(&host) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Some(credentials)) if !credentials.password.is_empty() => {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            credentials.password,
                        ));
                        this.notice = Some("已复制密码".into());
                    }
                    Ok(_) => this.notice = Some("此主机没有保存密码".into()),
                    Err(error) => this.notice = Some(error.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn pick_private_key(&mut self, cx: &mut Context<Self>) {
        let generation = self.form_generation;
        let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择 SSH 私钥".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.first()
            {
                let value = path.to_string_lossy().to_string();
                let _ = this.update(cx, |this, cx| {
                    if this.form_generation == generation {
                        this.form_key_path.update(cx, |e, cx| e.set_text(value, cx));
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
    pub(super) fn test_host(&mut self, confirmed: bool, cx: &mut Context<Self>) {
        if self.form_busy {
            return;
        }
        let port = Self::editor_text(&self.form_port, cx).trim().parse::<u16>();
        let Ok(port) = port else {
            self.form_error = Some("端口必须为 1–65535".into());
            cx.notify();
            return;
        };
        let mut host = ShellHost {
            id: self
                .editing_host
                .as_ref()
                .map(|h| h.id.clone())
                .unwrap_or_default(),
            name: Self::editor_text(&self.form_name, cx),
            host: Self::editor_text(&self.form_host, cx).trim().into(),
            port,
            user: Self::editor_text(&self.form_user, cx).trim().into(),
            key_path: if self.form_key_mode {
                Self::editor_text(&self.form_key_path, cx)
            } else {
                String::new()
            },
            ..self.editing_host.clone().unwrap_or_default()
        };
        if let Err(error) = host.validate() {
            self.form_error = Some(error.to_string().into());
            cx.notify();
            return;
        }
        if let Some(old) = &self.editing_host
            && (old.host != host.host || old.port != host.port)
        {
            host.fingerprint.clear();
        }
        let policy = if !host.fingerprint.is_empty() {
            HostKeyPolicy::Pinned(host.fingerprint.clone())
        } else if confirmed
            && let Some((address, port, user, fingerprint)) = &self.probe_pending
            && (&host.host, host.port, &host.user) == (address, *port, user)
        {
            HostKeyPolicy::TrustOnce(fingerprint.clone())
        } else {
            HostKeyPolicy::Ask
        };
        let credentials = HostCredentials {
            password: Self::editor_text(&self.form_password, cx),
            passphrase: Self::editor_text(&self.form_passphrase, cx),
        };
        let hosts = self.hosts.clone();
        let cancel = self.form_cancel.clone();
        let generation = self.form_generation;
        let identity = (host.host.clone(), host.port, host.user.clone());
        self.form_busy = true;
        self.form_error = Some("正在测试连接（只验证身份，不启动远程终端）…".into());
        let task = cx.background_executor().spawn(async move {
            let stored = hosts
                .credentials(&host)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            host.password = if credentials.password.is_empty() {
                stored.password
            } else {
                credentials.password
            };
            host.passphrase = if credentials.passphrase.is_empty() {
                stored.passphrase
            } else {
                credentials.passphrase
            };
            Ok::<_, String>(SshClient::probe(&host, policy, &cancel))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.form_generation != generation {
                    return;
                }
                this.form_busy = false;
                this.probe_pending = None;
                this.form_error = Some(match result {
                    Ok(Ok(_)) => "测试成功：SSH 身份验证通过".into(),
                    Ok(Err(TerminalError::HostKeyUnknown { fingerprint })) => {
                        this.probe_pending =
                            Some((identity.0, identity.1, identity.2, fingerprint.clone()));
                        format!("首次连接，请通过可信渠道核对指纹：{fingerprint}").into()
                    }
                    Ok(Err(error)) => error.to_string().into(),
                    Err(error) => error.into(),
                });
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_search_covers_notes_tags_and_system_with_combined_filters() {
        let host = ShellHost {
            name: "Prod".into(),
            group: "Production".into(),
            notes: "支付服务".into(),
            tags: "开发，DB;开发".into(),
            operating_system: "Linux".into(),
            ..Default::default()
        };
        assert_eq!(tags(&host.tags).len(), 2);
        assert!(matches_host(&host, "支付", "Production", "DB"));
        assert!(matches_host(&host, "linux", "", ""));
        assert!(!matches_host(&host, "", "Other", "DB"));
        assert!(!matches_host(&host, "", "", "D"));
    }
}
