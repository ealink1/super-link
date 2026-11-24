use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TabId {
    Query(String),
    Table(String),
}

pub(super) struct TabMenu {
    anchor: TabId,
    position: gpui::Point<gpui::Pixels>,
}

#[derive(Clone, Copy)]
enum CloseScope {
    Current,
    Others,
    Right,
    All,
}

fn targets(tabs: &[TabId], anchor: &TabId, scope: CloseScope) -> Vec<TabId> {
    let Some(index) = tabs.iter().position(|tab| tab == anchor) else {
        return Vec::new();
    };
    tabs.iter()
        .enumerate()
        .filter(|(i, _)| match scope {
            CloseScope::Current => *i == index,
            CloseScope::Others => *i != index,
            CloseScope::Right => *i > index,
            CloseScope::All => true,
        })
        .map(|(_, tab)| tab.clone())
        .collect()
}

impl SuperLinkApp {
    fn workspace_tab_ids(&self, cx: &App) -> Vec<TabId> {
        self.query_page
            .read(cx)
            .workspace_tabs()
            .into_iter()
            .map(|(id, _, _)| TabId::Query(id))
            .chain(self.table_tab_order.iter().cloned().map(TabId::Table))
            .collect()
    }

    pub(super) fn show_sql_tab_menu(
        &mut self,
        anchor: TabId,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.tab_overflow_position = None;
        let bounds = window.viewport_size();
        self.sql_tab_menu = Some(TabMenu {
            anchor,
            position: gpui::point(
                position.x.min((bounds.width - px(190.)).max(px(0.))),
                position.y.min((bounds.height - px(128.)).max(px(0.))),
            ),
        });
        cx.notify();
    }

    pub(super) fn sql_tab_menu_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.sql_tab_menu.as_ref()?;
        if self.workspace != Workspace::Sql {
            return None;
        }
        let theme = self.theme;
        let tabs = self.workspace_tab_ids(cx);
        let mut panel = div()
            .absolute()
            .left(menu.position.x)
            .top(menu.position.y)
            .w(px(188.))
            .p_1()
            .rounded_md()
            .shadow_md()
            .occlude()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.sql_tab_menu = None;
                cx.notify();
            }));
        for (index, (scope, label)) in [
            (CloseScope::Current, "关闭"),
            (CloseScope::Others, "关闭其他选项卡"),
            (CloseScope::Right, "关闭右侧选项卡"),
            (CloseScope::All, "全部关闭"),
        ]
        .into_iter()
        .enumerate()
        {
            let enabled = !targets(&tabs, &menu.anchor, scope).is_empty();
            let anchor = menu.anchor.clone();
            panel = panel.child(
                div()
                    .id(("sql-tab-menu-action", index))
                    .h(px(28.))
                    .px_3()
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .text_color(if enabled { theme.text } else { theme.muted })
                    .when(enabled, |row| {
                        row.cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if enabled {
                            this.close_workspace_tabs(&anchor, scope, cx);
                        }
                        this.sql_tab_menu = None;
                        cx.notify();
                    })),
            );
        }
        Some(panel.into_any_element())
    }

    fn close_workspace_tabs(&mut self, anchor: &TabId, scope: CloseScope, cx: &mut Context<Self>) {
        let selected = targets(&self.workspace_tab_ids(cx), anchor, scope);
        let blocked = self
            .workbench
            .iter()
            .chain(self.parked_workbenches.iter())
            .any(|bench| {
                selected.contains(&TabId::Table(bench.id.clone()))
                    && (!bench.edits.is_empty()
                        || bench.loading
                        || bench.ddl_loading
                        || bench
                            .designer
                            .as_ref()
                            .is_some_and(|d| d.read(cx).is_dirty() || d.read(cx).is_busy()))
            })
            || (self
                .structure_edit
                .as_ref()
                .is_some_and(|edit| edit.busy || !edit.pending.is_empty())
                && selected.iter().any(|tab| matches!(tab, TabId::Table(_))));
        if blocked {
            self.error = Some("待关闭标签有暂存变更或正在执行操作，请完成或撤销后再关闭".into());
            return;
        }
        for tab in selected {
            match tab {
                TabId::Query(id) => self
                    .query_page
                    .update(cx, |page, cx| page.close_workspace_tab(&id, cx)),
                TabId::Table(id) => self.close_table_tab(&id, cx),
            }
        }
        if self.query_page.read(cx).workspace_tabs().is_empty() && self.workbench.is_none() {
            self.show_object_tab(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_scopes_follow_mixed_visible_order() {
        let q = TabId::Query("q".into());
        let t = TabId::Table("t".into());
        let r = TabId::Table("r".into());
        let tabs = vec![q.clone(), t.clone(), r.clone()];
        assert_eq!(targets(&tabs, &t, CloseScope::Current), vec![t.clone()]);
        assert_eq!(
            targets(&tabs, &t, CloseScope::Others),
            vec![q.clone(), r.clone()]
        );
        assert_eq!(targets(&tabs, &t, CloseScope::Right), vec![r.clone()]);
        assert_eq!(targets(&tabs, &r, CloseScope::Right), Vec::<TabId>::new());
        assert_eq!(targets(&tabs, &t, CloseScope::All), tabs);
        assert!(targets(&tabs, &TabId::Query("gone".into()), CloseScope::All).is_empty());
    }
}
