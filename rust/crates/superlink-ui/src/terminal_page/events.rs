use super::*;

impl TerminalPage {
    pub(super) fn should_auto_reconnect(&self) -> bool {
        self.auto_reconnect
            && self.settings.shell_reconnect_enabled()
            && matches!(self.target, Some(Target::Remote { .. }))
    }

    pub(super) fn schedule_reconnect(&mut self, reason: String, cx: &mut Context<Self>) {
        if !self.should_auto_reconnect() {
            self.status = SharedString::from(reason);
            return;
        }
        let Some(target) = self.target.clone() else {
            return;
        };
        let attempt = self.attempts + 1;
        let Some(delay) = backoff_delay(attempt) else {
            self.status = "重连失败，已达上限，请手动重连".into();
            self.error = Some(reason.into());
            cx.notify();
            return;
        };
        self.attempts = attempt;
        self.generation += 1;
        let generation = self.generation;
        self.screen.feed(
            &mut self.parser,
            format!("\r\n\x1b[33m[{reason}]\x1b[0m\r\n").as_bytes(),
        );
        self.status =
            format!("已断开，{delay} 秒后重连（{attempt}/{MAX_RECONNECT_ATTEMPTS}）").into();
        cx.notify();

        cx.spawn(async move |this, cx| {
            for remaining in (1..=delay).rev() {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return false;
                    }
                    this.status = format!(
                        "已断开，{remaining} 秒后重连（{attempt}/{MAX_RECONNECT_ATTEMPTS}）"
                    )
                    .into();
                    cx.notify();
                    true
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation && this.should_auto_reconnect() {
                    this.begin(target, false, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn poll(
        &mut self,
        rx: Receiver<Event>,
        running: Arc<AtomicBool>,
        cx: &mut Context<Self>,
    ) {
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            while running.load(Ordering::Relaxed) {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let events: Vec<Event> = rx.try_iter().take(64).collect();
                if events.is_empty() {
                    continue;
                }
                let alive = this.update(cx, |this, cx| {
                    if this.generation != generation {
                        for event in events {
                            if let Event::Opened { handle, .. } = event {
                                cx.background_executor()
                                    .spawn(async move {
                                        let _ = handle.close();
                                    })
                                    .detach();
                            }
                        }
                        return false;
                    }
                    this.apply(events, cx);
                    true
                });
                if !matches!(alive, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn apply(&mut self, events: Vec<Event>, cx: &mut Context<Self>) {
        let mut changed = false;
        for event in events {
            changed = true;
            match event {
                Event::HostKeyChanged { expected, observed } => {
                    self.connecting = false;
                    self.connect_dialog = true;
                    self.changed_key = Some((expected, observed));
                    self.session = None;
                    self.running.store(false, Ordering::SeqCst);
                    self.status = "连接失败：主机指纹未匹配".into();
                    self.error = Some("SSH 主机指纹发生变化，连接已阻止，请核实服务器身份".into());
                }
                Event::Progress(event) => {
                    this_progress(self, event);
                }
                Event::Opened {
                    handle,
                    fingerprint,
                    client,
                } => {
                    self.connecting = false;
                    self.connect_dialog = false;
                    self.needs_focus = true;
                    self.transport = client;
                    if self.transport.is_some() {
                        self.auxiliary.mode = auxiliary::Mode::Monitor;
                        self.refresh_monitor(cx);
                    }
                    let (writer_tx, writer_rx) = mpsc::sync_channel::<Vec<u8>>(64);
                    let writer = handle.clone();
                    std::thread::spawn(move || {
                        while let Ok(bytes) = writer_rx.recv() {
                            if writer.write(&bytes).is_err() {
                                break;
                            }
                        }
                    });
                    self.write_queue = Some(writer_tx);
                    let resize = handle.clone();
                    let cols = self.screen.cols() as u16;
                    let rows = self.screen.rows() as u16;
                    cx.background_executor()
                        .spawn(async move {
                            let _ = resize.resize(cols, rows);
                        })
                        .detach();
                    self.session = Some(handle);
                    self.attempts = 0;
                    self.error = None;
                    self.status = match fingerprint {
                        Some(_) => "已连接".into(),
                        None => "已连接".into(),
                    };
                    self.pending = None;
                }
                Event::Data(bytes) => {
                    let before = self.screen.scrollback_len();
                    self.screen.feed(&mut self.parser, &bytes[..]);
                    if self.scroll_offset > 0 {
                        self.scroll_offset = (self.scroll_offset
                            + self.screen.scrollback_len().saturating_sub(before))
                        .min(self.screen.scrollback_len());
                    }
                    let replies = self.screen.take_replies();
                    if !replies.is_empty() {
                        self.send(&replies, cx);
                    }
                }
                Event::Closed(message) => {
                    self.connecting = false;
                    self.session = None;
                    self.transport = None;
                    self.write_queue = None;
                    self.running.store(false, Ordering::SeqCst);
                    self.schedule_reconnect(message, cx);
                }
                Event::Failed(message) => {
                    self.connecting = false;
                    self.session = None;
                    self.running.store(false, Ordering::SeqCst);
                    self.status = "连接失败".into();
                    self.error = Some(message.into());
                }
                Event::HostKeyUnknown {
                    fingerprint,
                    message,
                } => {
                    self.connecting = false;
                    self.connect_dialog = false;
                    self.session = None;
                    self.running.store(false, Ordering::SeqCst);
                    self.status = "等待确认主机指纹".into();
                    self.error = Some(message.into());
                    if let Some(Target::Remote { host, .. }) = self.target.as_ref() {
                        self.pending = Some((host.as_ref().clone(), fingerprint));
                    }
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub(super) fn trust(&mut self, remember: bool, cx: &mut Context<Self>) {
        let Some((mut host, fingerprint)) = self.pending.take() else {
            return;
        };
        if remember {
            host.fingerprint = fingerprint.clone();
            let hosts = self.hosts.clone();
            let confirmed = fingerprint.clone();
            let task = cx
                .background_executor()
                .spawn(async move { hosts.trust(&host.id, host.revision, &confirmed) });
            cx.spawn(async move |this, cx| {
                let outcome = task.await;
                let _ = this.update(cx, |this, cx| match outcome {
                    Ok(host) => this.begin(
                        Target::Remote {
                            host: Box::new(host),
                            policy: HostKeyPolicy::Pinned(fingerprint),
                        },
                        true,
                        cx,
                    ),
                    Err(error) => {
                        this.error = Some(format!("保存指纹失败：{error}").into());
                        cx.notify();
                    }
                });
            })
            .detach();
            return;
        }
        let policy = if remember {
            HostKeyPolicy::TrustAndRemember(fingerprint)
        } else {
            HostKeyPolicy::TrustOnce(fingerprint)
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
}
