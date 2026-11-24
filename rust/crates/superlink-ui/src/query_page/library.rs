use super::*;
impl QueryPage {
    pub(super) fn reload_history(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.clone() else {
            self.entries.clear();
            return;
        };
        let service = self.history.clone();
        let id = profile.id.clone();
        let task = cx
            .background_executor()
            .spawn(async move { service.list(&profile.id) });
        cx.spawn(async move |this, cx| {
            let entries = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.profile.as_ref().map(|p| &p.id) == Some(&id) {
                    this.entries = entries;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn clear_history(&mut self, cx: &mut Context<Self>) {
        if let Some(profile) = self.profile.clone() {
            let service = self.history.clone();
            cx.background_executor()
                .spawn(async move {
                    service.clear(&profile.id);
                })
                .detach();
        }
        self.entries.clear();
        cx.notify();
    }
    fn saved_job(
        &mut self,
        job: impl FnOnce(Arc<SavedQueryService>) -> Result<Option<String>, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.library_busy {
            return;
        }
        self.library_busy = true;
        self.saved_note = Some("正在更新查询库…".into());
        cx.notify();
        self.library_generation = self.library_generation.wrapping_add(1);
        let generation = self.library_generation;
        let service = self.saved.clone();
        let task = cx.background_executor().spawn(async move {
            let notice = job(service.clone())?;
            let items = service.list().map_err(|e| e.to_string())?;
            Ok::<_, String>((items, notice))
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.library_generation != generation {
                    return;
                }
                this.library_busy = false;
                match outcome {
                    Ok((items, notice)) => {
                        this.saved_items = items;
                        this.saved_note = notice.map(Into::into);
                    }
                    Err(error) => this.saved_note = Some(error.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn refresh_saved(&mut self, cx: &mut Context<Self>) {
        self.saved_job(|_| Ok(None), cx);
    }
    pub(super) fn save_current_query(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.clone() else {
            self.saved_note = Some("请先选择一个连接".into());
            cx.notify();
            return;
        };
        let sql = self.editor.read(cx).text().to_string();
        if sql.trim().is_empty() {
            self.saved_note = Some("当前查询为空，无法保存".into());
            cx.notify();
            return;
        }
        let typed = Self::editor_text(&self.saved_title, cx).trim().to_string();
        let title = if typed.is_empty() {
            sql.lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("未命名查询")
                .chars()
                .take(40)
                .collect()
        } else {
            typed
        };
        let scope = self.scope.clone();
        let schema = self.schema.clone();
        self.saved_job(
            move |service| {
                let saved = service
                    .save_with_context(&profile.id, &title, &scope, &schema, &sql)
                    .map_err(|e| e.to_string())?;
                Ok(Some(format!("已保存《{}》", saved.title)))
            },
            cx,
        );
    }
    pub(super) fn load_saved(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(item) = self.saved_items.iter().find(|item| item.id == id).cloned() else {
            return;
        };
        let id = if item.profile_id.is_empty() {
            self.profile
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_default()
        } else {
            item.profile_id.clone()
        };
        let profiles = self.profiles.clone();
        self.stop_query(cx);
        let generation = self.request_generation;
        let task = cx
            .background_executor()
            .spawn(async move { profiles.get(&id) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.request_generation != generation {
                    return;
                }
                match outcome {
                    Ok(profile) => {
                        this.select_profile(profile, cx);
                        this.scope = item.scope;
                        this.schema = item.schema;
                        this.editor
                            .update(cx, |editor, cx| editor.set_text(item.text, cx));
                        this.status = format!("已载入《{}》", item.title).into();
                        this.saved_open = false;
                        this.saved_note = None;
                    }
                    Err(error) => this.saved_note = Some(error.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn delete_saved(&mut self, id: String, cx: &mut Context<Self>) {
        self.saved_job(
            move |service| {
                service.delete(&id).map_err(|e| e.to_string())?;
                Ok(Some("已删除".into()))
            },
            cx,
        );
    }
    pub(super) fn commit_saved_rename(&mut self, cx: &mut Context<Self>) {
        let Some((id, editor)) = self.saved_renaming.clone() else {
            return;
        };
        let title = Self::editor_text(&editor, cx).trim().to_string();
        if title.is_empty() {
            self.saved_note = Some("名称不能为空".into());
            cx.notify();
            return;
        }
        self.saved_renaming = None;
        self.saved_job(
            move |service| {
                let saved = service.rename(&id, &title).map_err(|e| e.to_string())?;
                Ok(Some(format!("已重命名为《{}》", saved.title)))
            },
            cx,
        );
    }
}
