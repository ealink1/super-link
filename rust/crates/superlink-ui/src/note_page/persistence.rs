use super::*;
pub(super) type PendingSave = Arc<(
    std::sync::Mutex<Option<Result<i64, String>>>,
    std::sync::Condvar,
)>;

impl NotePage {
    pub(super) fn changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.change = self.change.wrapping_add(1);
        let change = self.change;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(400))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.change == change {
                    this.save(cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) -> bool {
        if self.closing || self.saving || !self.loaded || !self.dirty {
            return false;
        }
        if let Err(error) = self.notebook.validate() {
            self.error = Some(error.to_string().into());
            cx.notify();
            return false;
        }
        self.saving = true;
        let change = self.change;
        let snapshot = self.notebook.clone();
        let service = self.service.clone();
        let pending = Arc::new((std::sync::Mutex::new(None), std::sync::Condvar::new()));
        self.pending_save = Some(pending.clone());
        let task = cx.background_executor().spawn(async move {
            let result = service.save(&snapshot).map_err(|e| e.to_string());
            *pending.0.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(result.as_ref().map(|b| b.revision).map_err(Clone::clone));
            pending.1.notify_all();
            result
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                this.pending_save = None;
                match result {
                    Ok(saved) => {
                        this.notebook.revision = saved.revision;
                        this.error = None;
                        if this.change == change {
                            this.dirty = false;
                            this.notice = Some("本地笔记已保存".into());
                        } else {
                            this.save(cx);
                        }
                    }
                    Err(error) => {
                        this.error =
                            Some(format!("保存失败，修改仍保留在当前窗口：{error}").into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        true
    }
}

impl NotePage {
    pub(crate) fn flush_for_quit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<(), String>> {
        self.closing = true;
        let mut snapshot = self.notebook.clone();
        let pending = self.pending_save.clone();
        let dirty = self.dirty;
        let service = self.service.clone();
        cx.background_executor().spawn(async move {
            if let Some(pending) = pending {
                let mut result = pending.0.lock().unwrap_or_else(|e| e.into_inner());
                while result.is_none() {
                    result = pending.1.wait(result).unwrap_or_else(|e| e.into_inner());
                }
                snapshot.revision = result.as_ref().unwrap().clone()?;
            }
            if dirty {
                service.save(&snapshot).map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }
}
