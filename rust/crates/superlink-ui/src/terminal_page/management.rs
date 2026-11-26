use super::*;
impl TerminalPage {
    pub(crate) fn open_hosts(&mut self, cx: &mut Context<Self>) {
        self.close_host_form(cx);
        self.auxiliary.mode = auxiliary::Mode::Terminal;
        self.refresh_hosts(cx);
    }
    pub(crate) fn request_focus(&mut self, cx: &mut Context<Self>) {
        self.needs_focus = true;
        cx.notify();
    }
    pub(crate) fn set_open_handler(
        &mut self,
        handler: impl Fn(Option<ShellHost>, &mut App) + 'static,
    ) {
        self.open_handler = Some(Box::new(handler));
    }
    pub(crate) fn set_back_handler(&mut self, handler: impl Fn(&mut App) + 'static) {
        self.back_handler = Some(Box::new(handler));
    }
    pub(crate) fn refresh_hosts(&mut self, cx: &mut Context<Self>) {
        let hosts = self.hosts.clone();
        let task = cx.background_executor().spawn(async move { hosts.list() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(hosts) => {
                        this.host_list = hosts;
                        this.host_menu = None;
                        this.selected = this.selected.min(this.host_list.len().saturating_sub(1));
                    }
                    Err(e) => this.error = Some(e.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(crate) fn start_session(&mut self, host: Option<ShellHost>, cx: &mut Context<Self>) {
        self.auto_reconnect = true;
        let target = match host {
            None => Target::Local(None),
            Some(host) => {
                let policy = if host.fingerprint.is_empty() {
                    HostKeyPolicy::Ask
                } else {
                    HostKeyPolicy::Pinned(host.fingerprint.clone())
                };
                Target::Remote {
                    host: Box::new(host),
                    policy,
                }
            }
        };
        self.begin(target, true, cx);
    }
    pub(crate) fn start_local_program(&mut self, program: String, cx: &mut Context<Self>) {
        self.begin(Target::Local(Some(program)), true, cx);
    }
    pub(crate) fn open_terminal_settings(&mut self, cx: &mut Context<Self>) {
        self.auxiliary.mode = auxiliary::Mode::Settings;
        cx.notify();
    }
    pub(crate) fn close_session(&mut self, cx: &mut Context<Self>) {
        self.disconnect(cx);
    }
    pub(super) fn edit_host(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(host) = self.host_list.get(index).cloned() else {
            return;
        };
        self.open_host_form(cx);
        self.editing_host = Some(host.clone());
        self.form_remember = host.remember;
        self.form_key_mode = !host.key_path.is_empty();
        self.form_system = if host.operating_system.is_empty() {
            "Linux".into()
        } else {
            host.operating_system.clone()
        };
        for (editor, value) in [
            (&self.form_name, host.name),
            (&self.form_host, host.host),
            (&self.form_port, host.port.to_string()),
            (&self.form_user, host.user),
            (&self.form_notes, host.notes),
            (&self.form_group, host.group),
            (&self.form_tags, host.tags),
            (&self.form_key_path, host.key_path),
        ] {
            editor.update(cx, |e, cx| e.set_text(value, cx));
        }
        cx.notify();
    }
}

impl TerminalPage {
    pub(super) fn delete_edited_host(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.editing_host.clone() else {
            return;
        };
        if !self.confirm_host_delete {
            self.confirm_host_delete = true;
            cx.notify();
            return;
        }
        let hosts = self.hosts.clone();
        let id = host.id.clone();
        let task = cx
            .background_executor()
            .spawn(async move { hosts.delete(&host.id, host.revision) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.editing_host.as_ref().map(|h| &h.id) != Some(&id) {
                    return;
                }
                match outcome {
                    Ok(()) => {
                        this.form_open = false;
                        this.editing_host = None;
                        this.confirm_host_delete = false;
                        this.refresh_hosts(cx);
                    }
                    Err(error) => this.form_error = Some(error.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl TerminalPage {
    pub(crate) fn take_session_for_quit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<SessionHandle> {
        let handle = self.session.take();
        self.disconnect(cx);
        handle
    }
}
impl Drop for TerminalPage {
    fn drop(&mut self) {
        self.connect_cancel.cancel();
        self.form_cancel.cancel();
        self.auxiliary.cancel.cancel();
        self.auxiliary.monitor.reset();
        self.running.store(false, Ordering::Release);
        if let Some(handle) = self.session.take() {
            std::thread::spawn(move || {
                let _ = handle.close();
            });
        }
    }
}
