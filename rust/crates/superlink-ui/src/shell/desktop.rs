use super::*;
gpui::actions!(
    superlink,
    [
        Quit,
        NewQuery,
        NewConnection,
        Settings,
        SaveDocument,
        CheckUpdates,
        NewTerminal,
        ManageHosts
    ]
);
fn dispatch(
    window: gpui::WindowHandle<SuperLinkApp>,
    cx: &mut App,
    action: impl FnOnce(&mut SuperLinkApp, &mut Window, &mut Context<SuperLinkApp>) + 'static,
) {
    // GPUI temporarily removes the active Window while dispatching global actions.
    cx.defer(move |cx| {
        let _ = window.update(cx, action);
    });
}
pub(super) fn register(window: gpui::WindowHandle<SuperLinkApp>, cx: &mut App) {
    cx.on_action(move |_: &NewTerminal, cx| {
        dispatch(window, cx, |this, _, cx| {
            this.workspace = Workspace::Shell;
            this.terminal_page
                .update(cx, |page, cx| page.new_terminal(cx));
            cx.notify();
        })
    });
    cx.on_action(move |_: &ManageHosts, cx| {
        dispatch(window, cx, |this, _, cx| {
            this.workspace = Workspace::Shell;
            this.terminal_page
                .update(cx, |page, cx| page.manage_hosts(cx));
            cx.notify();
        })
    });
    cx.on_action(move |_: &CheckUpdates, cx| {
        dispatch(window, cx, |this, _, cx| {
            this.settings_open = true;
            this.check_updates(cx);
        })
    });
    cx.on_action(move |_: &Quit, cx| dispatch(window, cx, |this, _, cx| this.request_quit(cx)));
    cx.on_action(move |_: &NewQuery, cx| {
        dispatch(window, cx, |this, window, cx| {
            this.new_query_action(window, cx)
        })
    });
    cx.on_action(move |_: &NewConnection, cx| {
        dispatch(window, cx, |this, window, cx| {
            this.new_connection_action(window, cx)
        })
    });
    cx.on_action(move |_: &Settings, cx| {
        dispatch(window, cx, |this, window, cx| {
            this.open_settings(window, cx)
        })
    });
    cx.on_action(move |_: &SaveDocument, cx| {
        dispatch(window, cx, |this, _, cx| match this.workspace {
            Workspace::Note => this.note_page.update(cx, |page, cx| page.save_document(cx)),
            Workspace::Sql => this
                .query_page
                .update(cx, |page, cx| page.save_document(cx)),
            Workspace::Shell => {}
        })
    });
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.bind_keys([
        gpui::KeyBinding::new(&format!("{modifier}-q"), Quit, None),
        gpui::KeyBinding::new(&format!("{modifier}-n"), NewQuery, None),
        gpui::KeyBinding::new(&format!("{modifier}-shift-n"), NewConnection, None),
        gpui::KeyBinding::new(&format!("{modifier}-s"), SaveDocument, None),
        gpui::KeyBinding::new(&format!("{modifier}-,"), Settings, None),
        gpui::KeyBinding::new(&format!("{modifier}-alt-t"), NewTerminal, None),
        gpui::KeyBinding::new(&format!("{modifier}-alt-h"), ManageHosts, None),
    ]);
    cx.set_menus(vec![
        gpui::Menu {
            name: "SuperLink".into(),
            items: vec![
                gpui::MenuItem::action("设置…", Settings),
                gpui::MenuItem::action("检测更新…", CheckUpdates),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("退出 SuperLink", Quit),
            ],
        },
        gpui::Menu {
            name: "Shell".into(),
            items: vec![
                gpui::MenuItem::action("新建本地终端", NewTerminal),
                gpui::MenuItem::action("主机管理", ManageHosts),
            ],
        },
        gpui::Menu {
            name: "文件".into(),
            items: vec![
                gpui::MenuItem::action("新建查询", NewQuery),
                gpui::MenuItem::action("新建连接", NewConnection),
                gpui::MenuItem::action("保存", SaveDocument),
            ],
        },
    ]);
}
impl SuperLinkApp {
    pub(super) fn request_quit(&mut self, cx: &mut Context<Self>) {
        if self.quitting {
            return;
        }
        if self
            .workbench
            .iter()
            .chain(self.parked_workbenches.iter())
            .any(|b| {
                !b.edits.is_empty()
                    || b.designer
                        .as_ref()
                        .is_some_and(|d| d.read(cx).is_dirty() || d.read(cx).is_busy())
            })
        {
            self.error = Some("请先保存或放弃所有表标签中的数据与结构草稿，再退出".into());
            cx.activate(true);
            cx.notify();
            return;
        }
        if let Some(bench) = self.workbench.as_mut()
            && !bench.edits.is_empty()
        {
            bench.error = Some("请先提交或撤销表数据变更，再退出".into());
            cx.activate(true);
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut()
            && !edit.pending.is_empty()
        {
            edit.error = Some("请先提交或撤销结构变更，再退出".into());
            cx.activate(true);
            cx.notify();
            return;
        }
        self.quitting = true;
        let mut cancels = self.query_page.update(cx, |page, cx| page.prepare_quit(cx));
        cancels.extend(
            [
                self.drivers.cancel.take(),
                self.updates.cancel.take(),
                self.workbench.as_mut().and_then(|b| b.operation.take()),
            ]
            .into_iter()
            .flatten(),
        );
        let terminals = self.terminal_page.update(cx, |page, cx| page.close_all(cx));
        let notes = self
            .note_page
            .update(cx, |page, cx| page.flush_for_quit(cx));
        let settings = self.settings.clone();
        // The draft service flushes its ordered queue when its last Arc drops;
        // hold it here explicitly so Quit waits for the queue before termination.
        let drafts = self.query_page.read(cx).draft_service();
        let files = cx.background_executor().spawn(async move {
            for cancel in cancels {
                cancel.cancel();
                if !cancel.wait_finished(std::time::Duration::from_secs(15)) {
                    return Err("后台请求尚未释放，请稍后再退出".into());
                }
            }
            for terminal in terminals {
                terminal.close().map_err(|e| e.to_string())?;
            }
            settings.flush()?;
            drafts.flush()
        });
        cx.spawn(async move |this, cx| {
            let result = match notes.await {
                Ok(()) => files.await,
                Err(e) => Err(e),
            };
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => {
                    if let Some(request) = this.updates.prepared.as_ref()
                        && let Err(error) = superlink_app::updates::launch_helper(
                            request,
                            &superlink_app::data_root(),
                        )
                    {
                        this.quitting = false;
                        this.updates.message = error.into();
                        cx.notify();
                        return;
                    }
                    cx.quit();
                }
                Err(error) => {
                    this.quitting = false;
                    this.note_page
                        .update(cx, |page, cx| page.resume_after_quit_error(error, cx));
                    this.workspace = Workspace::Note;
                    cx.activate(true);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
