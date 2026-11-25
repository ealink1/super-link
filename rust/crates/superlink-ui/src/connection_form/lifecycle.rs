//! 表单打开/关闭与字段回填逻辑。

use super::*;

impl ConnectionForm {
    pub fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_test(cx);
        self.open = true;
        self.editing = None;
        self.generation = self.generation.wrapping_add(1);
        self.picker.update(cx, |picker, cx| picker.show(window, cx));
        cx.notify();
    }

    /// 选择器选定类型：以该类型默认值进入表单。
    pub fn choose_kind(&mut self, kind: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_fields(cx);
        // 选类型总是开始一条新连接，不覆盖正在编辑的那条。
        self.editing = None;
        self.kind = kind.to_string();
        self.icon_type = kind.to_string();
        self.picker.update(cx, |picker, cx| picker.hide(cx));
        if let Ok(descriptor) = resolve(kind) {
            let port = descriptor
                .port
                .map(|port| port.to_string())
                .unwrap_or_default();
            self.port.update(cx, |e, cx| e.set_text(port, cx));
            if matches!(kind, "sqlite" | "duckdb") {
                self.database.update(cx, |e, cx| e.set_text(":memory:", cx));
            }
        }
        self.apply_default_group(cx);
        self.focus_first(window, cx);
        cx.notify();
    }

    fn reset_fields(&mut self, cx: &mut Context<Self>) {
        self.remember_password = true;
        self.connect_after_save = true;
        self.read_only = false;
        self.protect_edit = false;
        self.protect_structure = false;
        self.protect_script = false;
        self.protect_import = false;
        self.network.clear(cx);
        self.extra.update(cx, |e, cx| e.set_text("", cx));
        self.group.update(cx, |e, cx| e.set_text("", cx));
        self.section = Section::Basic;
        self.use_ssl = false;
        self.ssl_mode = "required".into();
        self.environment = "dev".into();
        self.topology = "single".into();
        self.icon_type = String::new();
        self.icon_color = String::new();
        self.error = None;
        self.notice = None;
        self.testing = false;
        for editor in [
            &self.name,
            &self.host,
            &self.port,
            &self.user,
            &self.database,
            &self.password,
            &self.uri,
            &self.params,
            &self.dsn,
            &self.driver,
            &self.allow,
            &self.include,
            &self.exclude,
            &self.hosts,
            &self.ssl_ca,
            &self.ssl_cert,
            &self.ssl_key,
            &self.timeout,
            &self.query_timeout,
        ] {
            editor.update(cx, |editor, cx| editor.set_text(String::new(), cx));
        }
        self.set_placeholders(cx);
    }

    /// 新建时自动填入默认分组：清单里的第一个自定义分组，没有则留空（归入默认分组）。
    fn apply_default_group(&self, cx: &mut Context<Self>) {
        let groups = self
            .settings
            .get("connection.groups")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        if let Some(name) = groups.iter().find_map(|item| item.as_str()) {
            self.group
                .update(cx, |e, cx| e.set_text(name.to_string(), cx));
        }
    }

    pub fn open_edit(&mut self, profile: Profile, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_test(cx);
        self.open = true;
        self.picker.update(cx, |picker, cx| picker.hide(cx));
        self.reset_fields(cx);
        self.generation = self.generation.wrapping_add(1);
        self.editing = Some(profile.clone());
        self.remember_password = profile.config.save_password;
        self.connect_after_save = false;
        self.read_only = profile.read_only || profile.config.read_only;
        let protection = profile.config.protection.clone().unwrap_or_default();
        self.protect_edit = protection["restrictDataEdit"].as_bool().unwrap_or(false);
        self.protect_structure = protection["restrictStructureEdit"]
            .as_bool()
            .unwrap_or(false);
        self.protect_script = protection["restrictScriptExecution"]
            .as_bool()
            .unwrap_or(false);
        self.protect_import = protection["restrictDataImport"].as_bool().unwrap_or(false);
        self.network.load(&profile.config, cx);
        self.network.ssh = profile.config.use_ssh;
        self.network.proxy = profile.config.use_proxy;
        self.network.tunnel = profile.config.use_http_tunnel;
        self.group
            .update(cx, |e, cx| e.set_text(profile.group.clone(), cx));
        self.kind = profile.config.kind.clone();
        if self.kind.is_empty() {
            self.kind = profile.sql_dialect().to_string();
        }
        self.icon_type = if profile.icon_type.is_empty() {
            self.kind.clone()
        } else {
            profile.icon_type.clone()
        };
        self.icon_color = profile.icon_color.clone();
        self.environment = if profile.environment.is_empty() {
            "dev".into()
        } else {
            profile.environment.clone()
        };
        self.topology = if profile.config.topology.is_empty() {
            "single".into()
        } else {
            profile.config.topology.clone()
        };
        self.use_ssl = profile.config.use_ssl;
        self.ssl_mode = if profile.config.ssl_mode.trim().is_empty() {
            "required".into()
        } else {
            profile.config.ssl_mode.clone()
        };
        self.load_editable_fields(&profile, cx);

        // 密码与嵌套凭据来自加密库，异步读取后回填；代次不符说明已切换表单。
        let loader = self.service.clone();
        let snapshot = profile.clone();
        let generation = self.generation;
        let task = cx.background_executor().spawn(async move {
            loader
                .credentials(&snapshot)
                .map(|credentials| credentials.unwrap_or_default().apply(&snapshot.config))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation || !this.open {
                    return;
                }
                match result {
                    Ok(config) => {
                        this.password
                            .update(cx, |e, cx| e.set_text(config.password.clone(), cx));
                        this.network.load(&config, cx);
                    }
                    Err(e) => this.error = Some(e.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
        self.set_placeholders(cx);
        self.focus_first(window, cx);
        cx.notify();
    }

    fn load_editable_fields(&self, profile: &Profile, cx: &mut Context<Self>) {
        let config = &profile.config;
        let port = if config.port == 0 {
            String::new()
        } else {
            config.port.to_string()
        };
        let set = |editor: &Entity<SqlEditor>, text: String, cx: &mut Context<Self>| {
            editor.update(cx, |editor, cx| editor.set_text(text, cx));
        };
        set(&self.name, profile.name.clone(), cx);
        set(&self.host, config.host.clone(), cx);
        set(&self.port, port, cx);
        set(&self.user, config.user.clone(), cx);
        set(&self.database, config.database.clone(), cx);
        set(&self.password, String::new(), cx);
        set(&self.uri, config.uri.clone(), cx);
        set(&self.params, config.connection_params.clone(), cx);
        set(&self.dsn, config.dsn.clone(), cx);
        set(&self.driver, config.driver.clone(), cx);
        set(
            &self.allow,
            superlink_domain::join_patterns(&profile.database_allow),
            cx,
        );
        set(
            &self.include,
            superlink_domain::join_patterns(&profile.database_include),
            cx,
        );
        set(
            &self.exclude,
            superlink_domain::join_patterns(&profile.database_exclude),
            cx,
        );
        set(
            &self.hosts,
            superlink_domain::join_patterns(&config.hosts),
            cx,
        );
        set(&self.ssl_ca, config.ssl_ca_path.clone(), cx);
        set(&self.ssl_cert, config.ssl_cert_path.clone(), cx);
        set(&self.ssl_key, config.ssl_key_path.clone(), cx);
        set(&self.timeout, seconds(config.timeout), cx);
        set(&self.query_timeout, seconds(config.query_timeout), cx);
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.cancel_test(cx);
        self.open = false;
        self.picker.update(cx, |picker, cx| picker.hide(cx));
        self.generation = self.generation.wrapping_add(1);
        self.testing = false;
        cx.notify();
    }

    /// 表单页脚的「上一步」：回到数据源选择器。
    pub fn back_to_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_test(cx);
        self.picker.update(cx, |picker, cx| picker.show(window, cx));
        cx.notify();
    }

    fn focus_first(&self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.name.read(cx).focus_handle(cx);
        window.focus(&handle);
    }
}
