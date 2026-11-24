use super::*;

impl TerminalPage {
    pub fn new(
        hosts: Arc<HostService>,
        settings: Arc<superlink_app::SettingsService>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let host_list = Vec::new();
        let weak_page = cx.entity().downgrade();
        let search = cx.new(SqlEditor::new);
        search.update(cx, move |editor, cx| {
            editor.set_placeholder("搜索主机…", cx);
            let weak_page = weak_page.clone();
            editor.set_on_change(
                move |text, app| {
                    let weak_page = weak_page.clone();
                    let _ = weak_page.update(app, |page, cx| {
                        page.filter = text.to_string();
                        cx.notify();
                    });
                },
                cx,
            );
        });
        let mut page = Self {
            connect_dialog: false,
            connect_log: Vec::new(),
            changed_key: None,
            needs_focus: false,
            privacy: settings
                .get("shell.privacy")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            list_mode: settings
                .get("shell.listMode")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            connecting: false,
            connect_steps: [None; 4],
            host_menu: None,
            tag: String::new(),
            form_remember: false,
            form_key_mode: false,
            form_system: "Linux".into(),
            form_busy: false,
            form_generation: 0,
            form_cancel: CancelToken::new(),
            probe_pending: None,
            settings,
            open_handler: None,
            back_handler: None,
            terminal_handler: None,
            connect_cancel: CancelToken::new(),
            transport: None,
            auxiliary: auxiliary::Auxiliary::new(cx),
            editing_host: None,
            confirm_host_delete: false,
            screen_bounds: None,
            screen_lines: Vec::new(),
            write_queue: None,
            selection: None,
            selecting_text: false,
            hosts,
            theme,
            host_list,
            selected: 0,
            status: "选择主机或打开本地终端".into(),
            session: None,
            pending: None,
            screen: Screen::new(GEOMETRY.cols as usize, GEOMETRY.rows as usize),
            parser: vte::Parser::new(),
            focus: cx.focus_handle(),
            running: Arc::new(AtomicBool::new(false)),
            scroll_offset: 0,
            filter: String::new(),
            search,
            form_open: false,
            form_name: cx.new(SqlEditor::new),
            form_host: cx.new(SqlEditor::new),
            form_port: cx.new(SqlEditor::new),
            form_user: cx.new(SqlEditor::new),
            form_password: cx.new(SqlEditor::new),
            form_notes: cx.new(SqlEditor::new),
            form_group: cx.new(SqlEditor::new),
            form_tags: cx.new(SqlEditor::new),
            form_key_path: cx.new(SqlEditor::new),
            form_passphrase: cx.new(SqlEditor::new),
            form_error: None,
            error: None,
            composing: Composition::default(),
            target: None,
            auto_reconnect: false,
            attempts: 0,
            generation: 0,
            group: String::new(),
            rail_expanded: false,
            notice: None,
        };
        page.refresh_hosts(cx);
        page
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    pub fn open_host_form(&mut self, cx: &mut Context<Self>) {
        self.form_open = true;
        self.auxiliary.mode = auxiliary::Mode::Terminal;
        self.form_cancel.cancel();
        self.form_cancel = CancelToken::new();
        self.form_generation += 1;
        self.form_busy = false;
        self.form_remember = false;
        self.form_key_mode = false;
        self.form_system = "Linux".into();
        self.host_menu = None;
        self.probe_pending = None;
        self.confirm_host_delete = false;
        self.editing_host = None;
        self.form_error = None;
        for (editor, placeholder) in [
            (&self.form_name, "例如：开发服务器"),
            (&self.form_host, "主机名或 IP 地址"),
            (&self.form_port, "22"),
            (&self.form_user, "SSH 用户名"),
            (&self.form_password, "输入密码"),
            (&self.form_notes, "主机用途或备注"),
            (&self.form_group, "默认分组"),
            (&self.form_tags, "标签用逗号分隔"),
            (&self.form_key_path, "本机私钥的绝对路径"),
            (&self.form_passphrase, "私钥口令（可选）"),
        ] {
            editor.update(cx, |editor, cx| {
                editor.set_text(String::new(), cx);
                editor.set_placeholder(placeholder, cx);
                editor.set_masked(false, cx);
            });
        }
        self.form_password
            .update(cx, |editor, cx| editor.set_masked(true, cx));
        self.form_passphrase
            .update(cx, |e, cx| e.set_masked(true, cx));
        self.form_port
            .update(cx, |editor, cx| editor.set_text("22", cx));
        cx.notify();
    }

    pub fn close_host_form(&mut self, cx: &mut Context<Self>) {
        self.form_cancel.cancel();
        self.form_generation += 1;
        self.form_busy = false;
        self.probe_pending = None;
        self.form_password.update(cx, |e, cx| e.set_text("", cx));
        self.form_passphrase.update(cx, |e, cx| e.set_text("", cx));
        self.form_open = false;
        cx.notify();
    }

    pub fn save_host(&mut self, cx: &mut Context<Self>) {
        if self.form_busy {
            return;
        }
        use superlink_app::HostCredentials;

        self.form_error = None;
        let name = Self::editor_text(&self.form_name, cx);
        let host = Self::editor_text(&self.form_host, cx);
        let port_text = Self::editor_text(&self.form_port, cx);
        let user = Self::editor_text(&self.form_user, cx);
        let password = Self::editor_text(&self.form_password, cx);
        let notes = Self::editor_text(&self.form_notes, cx);

        let port: u16 = if port_text.trim().is_empty() {
            22
        } else {
            match port_text.trim().parse() {
                Ok(value) => value,
                Err(_) => {
                    self.form_error = Some("端口无效，请输入 1-65535".into());
                    cx.notify();
                    return;
                }
            }
        };
        let record = ShellHost {
            id: self
                .editing_host
                .as_ref()
                .map(|h| h.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name: name.trim().to_string(),
            host: host.trim().to_string(),
            user: user.trim().to_string(),
            port,
            notes: notes.trim().to_string(),
            group: Self::editor_text(&self.form_group, cx),
            tags: Self::editor_text(&self.form_tags, cx),
            key_path: if self.form_key_mode {
                Self::editor_text(&self.form_key_path, cx)
            } else {
                String::new()
            },
            operating_system: self.form_system.clone(),
            remember: self.form_remember,
            ..self.editing_host.clone().unwrap_or_default()
        };
        let credentials = HostCredentials {
            password,
            passphrase: Self::editor_text(&self.form_passphrase, cx),
        };
        let hosts = self.hosts.clone();
        self.form_busy = true;
        let generation = self.form_generation;
        let task = cx
            .background_executor()
            .spawn(async move { hosts.save(record, &credentials) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.form_generation != generation {
                    return;
                }
                this.form_busy = false;
                match result {
                    Ok(saved) => {
                        this.form_open = false;
                        this.editing_host = None;
                        this.status = format!("已保存主机「{}」", saved.name).into();
                        this.refresh_hosts(cx);
                    }
                    Err(e) => this.form_error = Some(e.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn editor_text(entity: &Entity<SqlEditor>, cx: &Context<Self>) -> String {
        entity.read(cx).text().to_string()
    }

    pub(super) fn reload_hosts(&mut self, cx: &mut Context<Self>) {
        self.refresh_hosts(cx);
    }

    pub(super) fn connect_host_at(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected = index;
        self.connect_selected(cx);
        self.focus.focus(window);
        cx.notify();
    }

    pub(super) fn connect_local(&mut self, cx: &mut Context<Self>) {
        if let Some(handler) = &self.open_handler {
            handler(None, cx);
            return;
        }
        self.attempts = 0;
        self.auto_reconnect = true;
        self.begin(Target::Local(None), true, cx);
    }

    pub(super) fn connect_selected(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host_list.get(self.selected).cloned() else {
            self.error = Some("还没有可连接的主机".into());
            cx.notify();
            return;
        };
        if let Some(handler) = &self.open_handler {
            handler(Some(host), cx);
            return;
        }
        let policy = if host.fingerprint.is_empty() {
            HostKeyPolicy::Ask
        } else {
            HostKeyPolicy::Pinned(host.fingerprint.clone())
        };
        self.attempts = 0;
        self.auto_reconnect = true;
        self.begin(
            Target::Remote {
                host: Box::new(host),
                policy,
            },
            true,
            cx,
        );
    }

    /// Manual reconnect after a drop, or a retry from the header button.
    pub(super) fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.connect_cancel.cancel();
        self.auxiliary.cancel.cancel();
        self.auxiliary.monitor.reset();
        self.auxiliary.busy = false;
        self.auxiliary.monitor_sampling = false;
        self.auxiliary.files.clear();
        self.auxiliary.selected = None;
        self.transport = None;
        self.write_queue = None;
        self.running.store(false, Ordering::Release);
        self.auto_reconnect = false;
        self.connecting = false;
        self.connect_dialog = false;
        self.pending = None;
        self.generation += 1;
        self.attempts = 0;
        if let Some(handle) = self.session.take() {
            cx.background_executor()
                .spawn(async move {
                    let _ = handle.close();
                })
                .detach();
        }
        self.status = "已断开".into();
        self.error = None;
        cx.notify();
    }

    pub(super) fn begin(&mut self, target: Target, clear_screen: bool, cx: &mut Context<Self>) {
        let target = match target {
            Target::Local(None) => Target::Local(self.settings.shell_default_program()),
            other => other,
        };
        if self.session.is_some() {
            return;
        }
        // Any new attempt invalidates a retry that is still counting down, so a
        // manual connect cannot be hijacked by the previous host's timer.
        self.connect_cancel.cancel();
        self.connect_cancel = CancelToken::new();
        self.generation += 1;
        self.auxiliary.cancel.cancel();
        self.auxiliary.monitor.reset();
        self.auxiliary.cancel = CancelToken::new();
        self.auxiliary.busy = false;
        self.auxiliary.metrics = None;
        self.auxiliary.monitor_sampling = false;
        self.auxiliary.files.clear();
        self.auxiliary.selected = None;
        self.transport = None;
        self.target = Some(target.clone());
        self.error = None;
        self.pending = None;
        self.connecting = true;
        self.connect_dialog = matches!(target, Target::Remote { .. });
        self.connect_log.clear();
        self.changed_key = None;
        self.connect_steps = [None; 4];
        self.status = "连接中…".into();
        self.needs_focus = true;
        if clear_screen {
            self.parser = vte::Parser::new();
            self.screen = Screen::new(GEOMETRY.cols as usize, GEOMETRY.rows as usize);
            self.scroll_offset = 0;
        }
        cx.notify();

        let (tx, rx) = mpsc::sync_channel(64);
        let running = Arc::new(AtomicBool::new(true));
        self.running = running.clone();
        spawn_worker(target, tx, self.connect_cancel.clone(), self.hosts.clone());
        self.poll(rx, running, cx);
    }
}
