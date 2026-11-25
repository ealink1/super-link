//! 表单里复用的行、开关、分段选择与按钮。

use super::*;
use gpui::Stateful;

#[derive(Debug, Clone, Copy)]
pub(super) enum Toggle {
    Remember,
    ConnectAfter,
    ReadOnly,
    ProtectEdit,
    ProtectStructure,
    ProtectScript,
    ProtectImport,
    UseSsl,
    UseSsh,
    UseProxy,
    UseTunnel,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Choice {
    Environment,
    Topology,
    Ssl,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ButtonKind {
    Primary,
    Ghost,
}

impl ConnectionForm {
    /// 左侧标签 + 输入框的一行；标签右对齐、输入框定宽，与旧版表单风格一致。
    pub(super) fn input_row(
        &self,
        label: &str,
        editor: &Entity<SqlEditor>,
        _cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.form_row_w(label, self.input_box(editor), 360.0)
    }

    /// 标签右对齐、字段定宽的一行，用于居中的对话框式表单。
    pub(super) fn form_row(&self, label: &str, control: impl IntoElement) -> gpui::Div {
        self.form_row_w(label, control, 360.0)
    }

    /// 同上，但指定输入框宽度（端口等短字段）。
    pub(super) fn form_row_w(
        &self,
        label: &str,
        control: impl IntoElement,
        width: f32,
    ) -> gpui::Div {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .w(px(80.0))
                    .flex_shrink_0()
                    .text_right()
                    .text_size(px(TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .w(px(width))
                    .flex_shrink_0()
                    .child(control.into_any_element()),
            )
    }

    pub(super) fn input_row_w(
        &self,
        label: &str,
        editor: &Entity<SqlEditor>,
        width: f32,
        _cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.form_row_w(label, self.input_box(editor), width)
    }

    pub(super) fn input_box(&self, editor: &Entity<SqlEditor>) -> gpui::Div {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .h(px(30.0))
            .px_2()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.canvas)
            .text_color(theme.text)
            .child(editor.clone())
    }

    /// 勾选行；点击切换对应开关。
    pub(super) fn check_row(
        &self,
        id: &'static str,
        label: &str,
        on: bool,
        toggle: Toggle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .text_size(px(TEXT_SIZE))
            .text_color(theme.text)
            .on_click(cx.listener(move |this, _, _, cx| this.apply_toggle(toggle, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(16.0))
                    .flex_shrink_0()
                    .rounded_sm()
                    .border_1()
                    .border_color(if on { theme.accent } else { theme.border })
                    .bg(if on { theme.accent } else { theme.canvas })
                    .children(on.then(|| {
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.surface)
                            .child("✓")
                    })),
            )
            .child(label.to_string())
            .into_any_element()
    }

    /// 一组互斥选项。
    pub(super) fn segment(
        &self,
        id: &'static str,
        options: &[(&'static str, &'static str)],
        current: &str,
        choice: Choice,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let mut row = div().flex().items_center().gap_1();
        for (key, label) in options {
            let key = *key;
            let active = key == current;
            row = row.child(
                div()
                    .id(SharedString::from(format!("{id}-{key}")))
                    .px_2()
                    .py(px(4.0))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.accent } else { theme.canvas })
                    .border_1()
                    .border_color(if active { theme.accent } else { theme.border })
                    .on_click(cx.listener(move |this, _, _, cx| this.apply_choice(choice, key, cx)))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if active { theme.surface } else { theme.text })
                            .child(*label),
                    ),
            );
        }
        row.into_any_element()
    }

    /// 网络页的一个分组卡：开关 + 标题 + 说明 + 展开后的明细。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn section_card(
        &self,
        id: &'static str,
        title: &str,
        description: &str,
        on: bool,
        toggle: Toggle,
        extras: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let mut card = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        // 复选框本体：点击切换，标签文字另起一列显示标题与说明。
                        {
                            let boxed =
                                div()
                                    .id(SharedString::from(format!("card-{id}")))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(16.0))
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(if on { theme.accent } else { theme.border })
                                    .bg(if on { theme.accent } else { theme.canvas })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.apply_toggle(toggle, cx)
                                    }));
                            if on {
                                boxed.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme.surface)
                                        .child("✓"),
                                )
                            } else {
                                boxed
                            }
                        },
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("card-label-{id}")))
                            .flex()
                            .flex_col()
                            .cursor_pointer()
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.apply_toggle(toggle, cx)),
                            )
                            .child(
                                div()
                                    .text_size(px(TEXT_SIZE))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(title.to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.muted)
                                    .child(description.to_string()),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child(if on { "已启用" } else { "未启用" }),
                    ),
            );
        if on {
            for extra in extras {
                card = card.child(extra);
            }
        }
        card.into_any_element()
    }

    pub(super) fn button(&self, label: &str, kind: ButtonKind) -> Stateful<gpui::Div> {
        let theme = self.theme;
        let (bg, fg, border) = match kind {
            ButtonKind::Primary => (theme.accent, theme.surface, theme.accent),
            ButtonKind::Ghost => (theme.canvas, theme.text, theme.border),
        };
        div()
            .id(SharedString::from(format!("form-{label}")))
            .px_3()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .bg(bg)
            .border_1()
            .border_color(border)
            .hover(move |style| style.bg(theme.accent_soft))
            .child(
                div()
                    .text_size(px(TEXT_SIZE))
                    .font_weight(match kind {
                        ButtonKind::Primary => FontWeight::SEMIBOLD,
                        ButtonKind::Ghost => FontWeight::NORMAL,
                    })
                    .text_color(fg)
                    .child(label.to_string()),
            )
    }

    pub(super) fn name_text(&self, cx: &Context<Self>) -> String {
        let editor = self.editors().name;
        editor.read(cx).text().to_string()
    }

    /// 网络分组里的字段行，按 `network::*_FIELDS` 的顺序生成。
    pub(super) fn network_rows(&self, keys: &[&str], _cx: &mut Context<Self>) -> Vec<AnyElement> {
        keys.iter()
            .map(|key| {
                let editor = self.network().editor(key);
                self.form_row_w(
                    network::NetworkFields::label(key),
                    self.input_box(&editor),
                    300.0,
                )
                .into_any_element()
            })
            .collect()
    }

    pub(super) fn apply_toggle(&mut self, toggle: Toggle, cx: &mut Context<Self>) {
        match toggle {
            Toggle::Remember => self.remember_password = !self.remember_password,
            Toggle::ConnectAfter => self.connect_after_save = !self.connect_after_save,
            Toggle::ReadOnly => self.read_only = !self.read_only,
            Toggle::ProtectEdit => self.protect_edit = !self.protect_edit,
            Toggle::ProtectStructure => self.protect_structure = !self.protect_structure,
            Toggle::ProtectScript => self.protect_script = !self.protect_script,
            Toggle::ProtectImport => self.protect_import = !self.protect_import,
            Toggle::UseSsl => self.use_ssl = !self.use_ssl,
            Toggle::UseSsh => self.network.ssh = !self.network.ssh,
            Toggle::UseProxy => self.network.proxy = !self.network.proxy,
            Toggle::UseTunnel => self.network.tunnel = !self.network.tunnel,
        }
        cx.notify();
    }

    pub(super) fn apply_choice(&mut self, choice: Choice, value: &str, cx: &mut Context<Self>) {
        match choice {
            Choice::Environment => self.environment = value.to_string(),
            Choice::Topology => self.topology = value.to_string(),
            Choice::Ssl => self.ssl_mode = value.to_string(),
        }
        cx.notify();
    }
}
