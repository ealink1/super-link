use super::*;

impl NotePage {
    pub(super) fn import_markdown(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("导入 Markdown 笔记".into()),
        });
        cx.spawn(async move |this, cx| {
            let paths = match picker.await {
                Ok(Ok(Some(paths))) => paths,
                _ => return,
            };
            let group = this
                .update(cx, |this, _| this.active_group.clone())
                .unwrap_or_default();
            let task = cx
                .background_executor()
                .spawn(async move { superlink_app::documents::read_notes(&paths, &group) });
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(notes) => {
                        let mut book = this.notebook.clone();
                        book.notes.extend(notes);
                        match book.validate() {
                            Ok(()) => {
                                this.notebook = book;
                                this.changed(cx);
                            }
                            Err(e) => this.error = Some(e.to_string().into()),
                        }
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn export_markdown(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.current().cloned() else {
            return;
        };
        let picker = cx.prompt_for_new_path(&superlink_app::data_root(), Some("笔记.md"));
        cx.spawn(async move |this, cx| {
            let path = match picker.await {
                Ok(Ok(Some(path))) => path,
                _ => return,
            };
            let task = cx
                .background_executor()
                .spawn(async move { superlink_app::documents::write_document(&path, &note.body) });
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => this.notice = Some("已导出 Markdown".into()),
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
