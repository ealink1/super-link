use super::*;

#[derive(Clone, Copy)]
enum CloseScope {
    Left,
    Right,
    Others,
    All,
}

fn targets(ids: &[String], anchor: &str, scope: CloseScope) -> Vec<String> {
    let Some(index) = ids.iter().position(|id| id == anchor) else {
        return Vec::new();
    };
    ids.iter()
        .enumerate()
        .filter(|(i, _)| match scope {
            CloseScope::Left => *i < index,
            CloseScope::Right => *i > index,
            CloseScope::Others => *i != index,
            CloseScope::All => true,
        })
        .map(|(_, id)| id.clone())
        .collect()
}

impl ShellWorkspace {
    /// Left click: switch to the tab only, never open the context menu.
    pub(super) fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = Some(index);
        self.home = false;
        self.tab_menu = None;
        self.tabs[index]
            .page
            .update(cx, |p, cx| p.request_focus(cx));
        cx.notify();
    }

    pub(super) fn show_tab_menu(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        self.tab_menu = Some(tab.id.clone());
        self.active = Some(index);
        self.tabs[index]
            .page
            .update(cx, |p, cx| p.request_focus(cx));
        self.home = false;
        cx.notify();
    }
    pub(super) fn tab_menu_panel(
        &self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.tabs.iter().any(|t| t.id == id) {
            return None;
        }
        let ids = self.tabs.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        let theme = self.theme;
        Some(
            div()
                .absolute()
                .top(px(34.))
                .left(self.menu_left)
                .w(px(190.))
                .p_1()
                .rounded_md()
                .flex()
                .flex_col()
                .occlude()
                .bg(theme.surface)
                .border_1()
                .border_color(theme.border)
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.tab_menu = None;
                    cx.notify();
                }))
                .children(
                    [
                        (CloseScope::Left, "关闭左边标签"),
                        (CloseScope::Right, "关闭右边标签"),
                        (CloseScope::Others, "关闭其他标签"),
                        (CloseScope::All, "关闭所有标签"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (scope, label))| {
                        let enabled = !targets(&ids, id, scope).is_empty();
                        let anchor = id.to_string();
                        div()
                            .id(("shell-tab-menu-action", i))
                            .px_3()
                            .py_2()
                            .text_size(px(12.))
                            .text_color(if enabled { theme.text } else { theme.muted })
                            .child(label)
                            .when(enabled, |row| {
                                row.cursor_pointer().hover(move |s| s.bg(theme.accent_soft))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                let ids =
                                    this.tabs.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
                                let selected = targets(&ids, &anchor, scope);
                                for index in (0..this.tabs.len()).rev() {
                                    if selected.contains(&this.tabs[index].id) {
                                        this.close(index, cx);
                                    }
                                }
                                this.tab_menu = None;
                                cx.notify();
                            }))
                    }),
                )
                .child(
                    div()
                        .id("shell-tab-menu-dismiss")
                        .px_3()
                        .py_2()
                        .cursor_pointer()
                        .text_size(px(12.))
                        .text_color(theme.muted)
                        .child("取消")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tab_menu = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }
    pub(super) fn tab_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform && key == "w" {
            if let Some(index) = self.active {
                self.close(index, cx);
                cx.stop_propagation();
            }
        } else if modifiers.control && key == "tab" && !self.tabs.is_empty() {
            let count = self.tabs.len();
            let active = self.active.unwrap_or(count - 1);
            self.active = Some(if modifiers.shift {
                (active + count - 1) % count
            } else {
                (active + 1) % count
            });
            self.home = false;
            self.tab_menu = None;
            if let Some(index) = self.active {
                self.tabs[index]
                    .page
                    .update(cx, |p, cx| p.request_focus(cx));
            }
            cx.stop_propagation();
            cx.notify();
        } else if modifiers.platform
            && let Ok(index) = key.parse::<usize>()
            && index > 0
            && index <= self.tabs.len()
        {
            self.active = Some(index - 1);
            self.tabs[index - 1]
                .page
                .update(cx, |p, cx| p.request_focus(cx));
            self.home = false;
            self.tab_menu = None;
            cx.stop_propagation();
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_close_scopes_keep_order_and_reject_stale_anchors() {
        let ids: Vec<String> = ["a", "b", "c", "d"].map(str::to_string).into();
        assert_eq!(targets(&ids, "b", CloseScope::Left), ["a"]);
        assert_eq!(targets(&ids, "b", CloseScope::Right), ["c", "d"]);
        assert_eq!(targets(&ids, "b", CloseScope::Others), ["a", "c", "d"]);
        assert_eq!(targets(&ids, "b", CloseScope::All), ids);
        assert!(targets(&ids, "removed", CloseScope::All).is_empty());
        assert!(targets(&ids, "a", CloseScope::Left).is_empty());
        assert!(targets(&ids, "d", CloseScope::Right).is_empty());
    }
}
