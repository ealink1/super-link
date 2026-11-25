//! 校验、保存与连接测试。

use super::*;

impl ConnectionForm {
    /// 校验并生成待保存的连接；失败时把提示写进表单。
    pub(super) fn build_profile(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(Profile, Credentials)> {
        self.error = None;
        let values = self.values(cx);
        // 连接串一旦填写就优先，和旧版 preferConnectionURI 一致；本地库仍用文件路径。
        let local = matches!(self.kind.as_str(), "sqlite" | "duckdb");
        let use_uri = uri_takes_over(&values.uri) && !local;
        match model::compose(&values, self.editing.as_ref(), use_uri) {
            Ok((mut profile, credentials)) => {
                if let Err(message) = self.network.apply(&mut profile, cx) {
                    self.error = Some(message.into());
                    return None;
                }
                let extra = self.extra.read(cx).text().to_string();
                if let Err(message) = overlay_extra(&mut profile, &extra) {
                    self.error = Some(message.into());
                    return None;
                }
                profile.config.save_password = self.remember_password;
                Some((profile, credentials))
            }
            Err(message) => {
                self.error = Some(message.into());
                None
            }
        }
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        let Some((profile, credentials)) = self.build_profile(cx) else {
            cx.notify();
            return;
        };
        if self.saving {
            return;
        }
        self.saving = true;
        let connect = self.connect_after_save;
        let service = self.service.clone();
        let remember = self.remember_password;
        let generation = self.generation;
        let task = cx
            .background_executor()
            .spawn(async move { service.save_with_remember(profile, &credentials, remember) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.saving = false;
                match result {
                    Ok(saved) => {
                        this.open = false;
                        if connect && let Some(handler) = this.on_saved.as_ref() {
                            handler(&saved.id, cx);
                        }
                    }
                    Err(e) => this.error = Some(e.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn cancel_test(&mut self, cx: &mut Context<Self>) {
        self.testing = false;
        if let Some(cancel) = self.test_cancel.take() {
            cx.background_executor()
                .spawn(async move {
                    cancel.cancel();
                })
                .detach();
        }
    }

    pub fn test(&mut self, cx: &mut Context<Self>) {
        if self.testing {
            return;
        }
        let Some((profile, credentials)) = self.build_profile(cx) else {
            cx.notify();
            return;
        };
        self.testing = true;
        let cancel = superlink_app::QueryCancel::new();
        self.test_cancel = Some(cancel.clone());
        let generation = self.generation;
        self.notice = None;
        self.error = None;
        let queries = self.queries.clone();
        cx.spawn(async move |this, cx| {
            let task = cx.background_executor().spawn(async move {
                queries.test_connection_with_cancel(&profile, &credentials, &cancel)
            });
            let outcome = task.await;
            this.update(cx, |form, cx| {
                if form.generation != generation || !form.open {
                    return;
                }
                form.test_cancel = None;
                form.testing = false;
                match outcome {
                    Ok(ms) => form.notice = Some(format!("连接测试通过 · {ms} ms").into()),
                    Err(err) => form.error = Some(format!("测试失败：{err}").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn set_placeholders(&self, cx: &mut Context<Self>) {
        let placeholders = [
            (&self.name, "例如：本地测试库"),
            (&self.host, "例如 127.0.0.1"),
            (&self.port, "3306"),
            (&self.user, "例如 root"),
            (&self.database, "数据库"),
            (&self.password, "密码"),
            (&self.uri, "例如 mysql://root@127.0.0.1:3306/app"),
            (&self.params, "URI / DSN query 参数，例如 charset=utf8mb4"),
            (&self.dsn, "自定义驱动 DSN"),
            (&self.driver, "例如 pgx"),
            (&self.allow, "可选：精确库名，逗号分隔；空表示不限制"),
            (&self.include, "例如 tenant_%, reporting*"),
            (&self.exclude, "例如 *_archive, test_%"),
            (&self.hosts, "多个 host:port，逗号分隔"),
            (&self.ssl_ca, "例如 /etc/ssl/ca.pem"),
            (&self.ssl_cert, "客户端证书（可选）"),
            (&self.ssl_key, "客户端私钥（可选）"),
            (&self.timeout, "30"),
            (&self.query_timeout, "30"),
        ];
        for (editor, hint) in placeholders {
            editor.update(cx, |editor, cx| editor.set_placeholder(hint, cx));
        }
    }
}
