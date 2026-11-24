use super::*;
impl TerminalPage {
    pub(super) fn scan_large_files(&mut self, cx: &mut Context<Self>) {
        if self.auxiliary.monitor.busy {
            return;
        }
        let Some(client) = self.transport.clone() else {
            self.auxiliary.monitor.notice = "大文件扫描需要已连接的 SSH 会话".into();
            cx.notify();
            return;
        };
        let state = &mut self.auxiliary.monitor;
        let paths = state
            .paths
            .read(cx)
            .text()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let (minimum, count) = (state.minimum, state.count);
        state.cancel = CancelToken::new();
        state.job = state.job.wrapping_add(1);
        state.busy = true;
        state.notice = "正在扫描，最多运行 30 秒…".into();
        let (cancel, job, generation) = (state.cancel.clone(), state.job, self.generation);
        let task = cx
            .background_executor()
            .spawn(async move { client.large_files(&paths, minimum, count, &cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation || this.auxiliary.monitor.job != job {
                    return;
                }
                let state = &mut this.auxiliary.monitor;
                state.busy = false;
                match result {
                    Ok(files) => {
                        state.notice = format!("找到 {} 个文件", files.len());
                        state.files = files;
                    }
                    Err(error) => state.notice = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn stop_monitor_process(&mut self, process: ProcessMetric, cx: &mut Context<Self>) {
        if self.auxiliary.monitor.busy {
            return;
        }
        if self.auxiliary.monitor.confirm.as_ref() != Some(&process) {
            self.auxiliary.monitor.confirm = Some(process);
            cx.notify();
            return;
        }
        let Some(client) = self.transport.clone() else {
            return;
        };
        let state = &mut self.auxiliary.monitor;
        state.confirm = None;
        state.cancel = CancelToken::new();
        state.job = state.job.wrapping_add(1);
        state.busy = true;
        state.notice = "正在发送 SIGTERM…".into();
        let (cancel, job, generation) = (state.cancel.clone(), state.job, self.generation);
        let task = cx
            .background_executor()
            .spawn(async move { client.stop_process(&process, &cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation || this.auxiliary.monitor.job != job {
                    return;
                }
                this.auxiliary.monitor.busy = false;
                this.auxiliary.monitor.notice = result
                    .map(|_| "已发送 SIGTERM".into())
                    .unwrap_or_else(|e| e.to_string());
                this.refresh_monitor(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn open_monitor_directory(&mut self, path: String, cx: &mut Context<Self>) {
        let directory = path
            .rsplit_once('/')
            .map(|(p, _)| if p.is_empty() { "/" } else { p })
            .unwrap_or("/");
        self.auxiliary
            .path
            .update(cx, |e, cx| e.set_text(directory.to_string(), cx));
        self.auxiliary.mode = Mode::Files;
        self.list_files(cx);
        cx.notify();
    }
}
