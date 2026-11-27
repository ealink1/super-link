//! 数据库 / Namespace 选择器：工具栏输入框即搜索框，候选列表是下方弹层。
//!
//! 参考 GoNavi 的库选择弹层：输入即过滤、当前库浅绿加粗、`↑`/`↓` 移动、
//! 回车应用、Esc 或点击外部关闭。弹层用 `deferred` 绘制在页面内容之上，
//! 避免被下方的编辑器与结果区覆盖。

use super::*;

/// 选中行沿用对象树的绿色底，保证两处选中语义一致。
const SELECTED_GREEN: u32 = 0x17823b;
const SELECTED_ALPHA: f32 = 0.18;
const LIST_MAX_HEIGHT: f32 = 280.0;
const ROW_HEIGHT: f32 = 30.0;
/// 弹层与输入框之间的间距。
const POPUP_GAP: f32 = 4.0;

/// 大小写不敏感地过滤库列表；空查询返回全部。
fn filter_scope_options(options: &[String], needle: &str) -> Vec<String> {
    let needle = needle.trim().to_lowercase();
    options
        .iter()
        .filter(|name| needle.is_empty() || name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

/// 光标默认停在当前已选库上；不在列表里时停在首项。
fn cursor_for(options: &[String], current: &str) -> usize {
    options.iter().position(|name| name == current).unwrap_or(0)
}

/// 回车采用的库名：优先光标所在候选，没有候选时用输入文本。
fn confirm_value(matches: &[String], cursor: usize, typed: &str) -> String {
    if matches.is_empty() {
        return typed.trim().to_string();
    }
    matches
        .get(cursor.min(matches.len().saturating_sub(1)))
        .cloned()
        .unwrap_or_else(|| typed.trim().to_string())
}

impl QueryPage {
    /// 工具栏输入框同时充当搜索框；输入变化重置候选光标。
    pub(super) fn new_scope_search(cx: &mut Context<Self>) -> Entity<SqlEditor> {
        let editor = cx.new(|cx| {
            let mut editor = SqlEditor::new(cx);
            editor.set_placeholder("数据库 / Namespace", cx);
            editor
        });
        let weak = cx.entity().downgrade();
        editor.update(cx, |editor, cx| {
            editor.set_on_change(
                move |_text, app| {
                    let weak = weak.clone();
                    // 回调发生在编辑器自身更新中，排队再改页面状态。
                    app.defer(move |app| {
                        let _ = weak.update(app, |this, cx| {
                            this.scope_cursor = 0;
                            cx.notify();
                        });
                    });
                },
                cx,
            );
        });
        editor
    }

    /// 工具栏输入框：已展开则收起，否则清空为搜索词、读取库列表并聚焦。
    pub(super) fn toggle_scope(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.scope_open {
            self.close_scope(cx);
            return;
        }
        self.profiles_open = false;
        self.scope_open = true;
        self.scope_cursor = cursor_for(&self.scope_options, &self.scope);
        self.scope_search.update(cx, |editor, cx| {
            editor.set_placeholder("搜索数据库…", cx);
            editor.set_text("", cx);
        });
        use gpui::Focusable as _;
        let handle = self.scope_search.read(cx).focus_handle(cx);
        window.focus(&handle);
        self.load_scope_options(cx);
        cx.notify();
    }

    /// 点击输入框：收起状态下先展开候选列表，然后聚焦当前库名全选以便直接改写。
    pub(super) fn focus_scope_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.scope_open {
            self.toggle_scope(window, cx);
            return;
        }
        use gpui::Focusable as _;
        let handle = self.scope_search.read(cx).focus_handle(cx);
        window.focus(&handle);
        self.scope_search
            .update(cx, |editor, cx| editor.select_all_text(cx));
        cx.notify();
    }

    /// 收起弹层，并把输入框恢复成当前库名。
    pub(super) fn close_scope(&mut self, cx: &mut Context<Self>) {
        if !self.scope_open {
            return;
        }
        self.scope_open = false;
        self.sync_scope_input(cx);
        cx.notify();
    }

    /// 输入框空闲时显示当前库；弹层打开或正在输入时不覆盖用户输入。
    ///
    /// 关闭弹层时若用户在搜索框里输入过文字但没有应用，丢弃这段临时搜索词，
    /// 避免把半截过滤词当成库名。
    pub(super) fn sync_scope_input(&mut self, cx: &mut Context<Self>) {
        if self.scope_open {
            return;
        }
        let scope = self.scope.clone();
        self.scope_search.update(cx, |editor, cx| {
            let placeholder = if scope.is_empty() {
                "数据库 / Namespace".to_string()
            } else {
                scope.clone()
            };
            editor.set_placeholder(placeholder.clone(), cx);
            if editor.text() != scope && editor.text() != placeholder {
                editor.set_text(scope.clone(), cx);
            }
        });
    }

    /// 页面捕获阶段按下鼠标：落在输入框和弹层之外就收起弹层。
    ///
    /// 捕获阶段先于输入框自身的焦点/选词处理，判断用的屏幕区域来自上一帧。
    pub(super) fn scope_mouse_down_out(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        popup_bounds: Option<gpui::Bounds<gpui::Pixels>>,
        cx: &mut Context<Self>,
    ) {
        if !self.scope_open {
            return;
        }
        let in_input = self
            .scope_input_bounds
            .borrow()
            .is_some_and(|bounds| bounds.contains(&position));
        let in_popup = popup_bounds.is_some_and(|bounds| bounds.contains(&position));
        if !in_input && !in_popup {
            // 收起后再让事件继续派发，点击目标（编辑器等）照常获得焦点。
            self.close_scope(cx);
        }
    }

    /// 输入框里按 Esc / Enter / 上下键时驱动候选列表；其余按键照常编辑。
    ///
    /// 页面级监听会收到所有子树冒泡上来的按键，这里先确认焦点仍在搜索框，
    /// 避免在 SQL 编辑器里按回车/方向键时被弹层吞掉。
    pub(super) fn scope_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.scope_open {
            return false;
        }
        use gpui::Focusable as _;
        let handle = self.scope_search.read(cx).focus_handle(cx);
        if !handle.is_focused(window) {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_scope(cx);
                true
            }
            "enter" => {
                self.confirm_scope(cx);
                true
            }
            "up" => {
                self.move_scope_cursor(-1, cx);
                true
            }
            "down" => {
                self.move_scope_cursor(1, cx);
                true
            }
            _ => false,
        }
    }

    /// 后台读取当前连接的库列表；连接或修订变化后丢弃旧结果。
    fn load_scope_options(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.clone() else {
            return;
        };
        let queries = self.queries.clone();
        let profiles = self.profiles.clone();
        let id = profile.id.clone();
        let revision = profile.revision;
        let task = cx.background_executor().spawn(async move {
            let credentials = profiles
                .credentials(&profile)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            queries
                .databases(&profile, &credentials)
                .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.profile.as_ref().map(|p| (&p.id, p.revision)) != Some((&id, revision)) {
                    return;
                }
                match result {
                    Ok(items) => {
                        this.scope_options = items;
                        let needle = this.scope_search.read(cx).text().to_string();
                        let matches = filter_scope_options(&this.scope_options, &needle);
                        this.scope_cursor = cursor_for(&matches, &this.scope);
                    }
                    Err(error) => this.status = format!("读取数据库失败：{error}").into(),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `↑`/`↓` 移动候选光标，循环。
    pub(super) fn move_scope_cursor(&mut self, delta: isize, cx: &mut Context<Self>) {
        let needle = self.scope_search.read(cx).text().to_string();
        let matches = filter_scope_options(&self.scope_options, &needle);
        if matches.is_empty() {
            self.scope_cursor = 0;
        } else {
            let len = matches.len() as isize;
            self.scope_cursor = (self.scope_cursor as isize + delta).rem_euclid(len) as usize;
        }
        cx.notify();
    }

    /// 回车：按候选库切换当前标签的库，Schema 保持不变。
    pub(super) fn confirm_scope(&mut self, cx: &mut Context<Self>) {
        let needle = self.scope_search.read(cx).text().to_string();
        let matches = filter_scope_options(&self.scope_options, &needle);
        let value = confirm_value(&matches, self.scope_cursor, &needle);
        self.apply_scope(value, cx);
    }

    fn apply_scope(&mut self, value: String, cx: &mut Context<Self>) {
        self.stop_query(cx);
        self.scope = value;
        self.scope_open = false;
        self.sync_scope_input(cx);
        self.save_active(cx);
        cx.notify();
    }

    /// 弹层主体：候选列表，宽度与上方输入框一致。
    pub(super) fn scope_picker(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.scope_open {
            return None;
        }
        let theme = self.theme;
        let needle = self.scope_search.read(cx).text().to_string();
        let matches = filter_scope_options(&self.scope_options, &needle);
        let cursor = self.scope_cursor.min(matches.len().saturating_sub(1));
        let selected_bg = gpui::Hsla::from(gpui::rgb(SELECTED_GREEN)).opacity(SELECTED_ALPHA);

        let mut list = div()
            .id("query-scope-list")
            .flex()
            .flex_col()
            .gap(px(1.0))
            .p_1()
            .max_h(px(LIST_MAX_HEIGHT))
            .overflow_y_scroll()
            .scrollbar_width(px(8.0));
        if matches.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child("没有匹配的数据库，回车按输入值应用"),
            );
        }
        for (index, name) in matches.iter().enumerate() {
            let selected = *name == self.scope;
            let hovered = index == cursor;
            let value = name.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("query-scope-option-{index}")))
                    .h(px(ROW_HEIGHT))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .when(selected, |row| {
                        row.bg(selected_bg).font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(hovered && !selected, |row| row.bg(theme.hover))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_scope(value.clone(), cx);
                    }))
                    .child(SharedString::from(name.clone())),
            );
        }

        let panel = div()
            .id("query-scope-picker")
            .w_full()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .shadow_md()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_scope(cx)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        cx.stop_propagation();
                        this.close_scope(cx);
                    }
                    "enter" => {
                        cx.stop_propagation();
                        this.confirm_scope(cx);
                    }
                    "up" => {
                        cx.stop_propagation();
                        this.move_scope_cursor(-1, cx);
                    }
                    "down" => {
                        cx.stop_propagation();
                        this.move_scope_cursor(1, cx);
                    }
                    _ => {}
                }
            }))
            .child(list);

        // 绝对定位在输入框容器内、贴输入框下缘；deferred 让它在整页之后绘制，
        // 这样位置由布局算出、不会随点击坐标漂移，也不会被编辑器盖住。
        let popup_slot = self.scope_popup_bounds.clone();
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .top(px(crate::theme::CONTROL_MIN + POPUP_GAP))
                    .left(px(0.0))
                    .right(px(0.0))
                    .on_children_prepainted(move |bounds, _, _| {
                        if let Some(bounds) = bounds.first() {
                            *popup_slot.borrow_mut() = Some(*bounds);
                        }
                    })
                    .child(panel),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{confirm_value, cursor_for, filter_scope_options};

    fn options() -> Vec<String> {
        ["ott_init", "ott.member", "sys", "information_schema"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn empty_query_keeps_every_option() {
        assert_eq!(filter_scope_options(&options(), "").len(), 4);
        assert_eq!(filter_scope_options(&options(), "   ").len(), 4);
    }

    #[test]
    fn filter_is_case_insensitive_and_substring_based() {
        assert_eq!(
            filter_scope_options(&options(), "OTT_"),
            vec!["ott_init".to_string()]
        );
        assert_eq!(
            filter_scope_options(&options(), "SCHEMA"),
            vec!["information_schema".to_string()]
        );
        assert!(filter_scope_options(&options(), "zzz").is_empty());
    }

    #[test]
    fn cursor_lands_on_the_current_database_or_first_option() {
        let all = options();
        assert_eq!(cursor_for(&all, "sys"), 2);
        assert_eq!(cursor_for(&all, "missing"), 0);
        assert_eq!(cursor_for(&[], "sys"), 0);
    }

    #[test]
    fn confirm_prefers_the_highlighted_match_then_the_typed_text() {
        let all = options();
        assert_eq!(confirm_value(&all, 1, "ott"), "ott.member");
        assert_eq!(confirm_value(&all, 99, "ott"), "information_schema");
        assert_eq!(confirm_value(&[], 0, "  custom  "), "custom");
    }
}
