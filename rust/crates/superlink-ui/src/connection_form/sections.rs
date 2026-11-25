//! 数据库、SSL、SSH、代理、外观与高级分区。

use super::view::hex_color;
use super::*;
use crate::theme::TEXT_SIZE;
use widgets::{Choice, Toggle};

impl ConnectionForm {
    pub(super) fn database_section(&self, editors: &Editors, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let mut block = div().flex().flex_col().gap_3();
        block = block
            .child(self.form_row("数据库", self.input_box(&editors.database)))
            .child(self.form_row("精确库名", self.input_box(&editors.allow)))
            .child(self.form_row("通配包含", self.input_box(&editors.include)))
            .child(self.form_row("通配排除", self.input_box(&editors.exclude)))
            .child(
                div()
                    .ml(px(92.0))
                    .text_size(px(11.0))
                    .text_color(theme.muted)
                    .child("精确库名与通配规则均为逗号分隔；留空表示显示全部可见库。"),
            );
        if !matches!(self.kind(), "sqlite" | "duckdb") {
            block = block
                .child(self.form_row(
                    "模式",
                    self.segment(
                        "topology",
                        &TOPOLOGY_OPTIONS,
                        self.topology(),
                        Choice::Topology,
                        cx,
                    ),
                ))
                .child(self.form_row("节点地址", self.input_box(&editors.hosts)));
        }
        block
            .child(
                self.form_row(
                    "分组",
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(self.input_box(&editors.group).w(px(180.0)))
                        .child(self.segment(
                            "environment",
                            &ENVIRONMENT_OPTIONS,
                            self.environment(),
                            Choice::Environment,
                            cx,
                        )),
                ),
            )
            .child(self.protection_block(cx))
            .into_any_element()
    }

    pub(super) fn ssl_section(&self, editors: &Editors, cx: &mut Context<Self>) -> AnyElement {
        let flags = self.flags();
        self.section_card(
            "ssl",
            "SSL / TLS",
            "加密与证书校验",
            flags.use_ssl,
            Toggle::UseSsl,
            vec![
                self.form_row(
                    "验证模式",
                    self.segment(
                        "ssl-mode",
                        &SSL_MODE_OPTIONS,
                        self.ssl_mode(),
                        Choice::Ssl,
                        cx,
                    ),
                )
                .into_any_element(),
                self.form_row(
                    "严格校验",
                    self.segment(
                        "ssl-verify",
                        &SSL_VERIFY_OPTIONS,
                        self.ssl_mode(),
                        Choice::Ssl,
                        cx,
                    ),
                )
                .into_any_element(),
                self.input_row("CA 证书", &editors.ssl_ca, cx)
                    .into_any_element(),
                self.input_row("客户端证书", &editors.ssl_cert, cx)
                    .into_any_element(),
                self.input_row("客户端私钥", &editors.ssl_key, cx)
                    .into_any_element(),
            ],
            cx,
        )
    }

    pub(super) fn ssh_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let network = self.network();
        self.section_card(
            "ssh",
            "SSH 隧道",
            "跳板机 / 堡垒机转发",
            network.ssh,
            Toggle::UseSsh,
            self.network_rows(network::SSH_FIELDS, cx),
            cx,
        )
    }

    pub(super) fn proxy_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let network = self.network();
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.section_card(
                "proxy",
                "代理",
                "本地代理或网关转发（仅填写 raw host，不含协议前缀）",
                network.proxy,
                Toggle::UseProxy,
                self.network_rows(network::PROXY_FIELDS, cx),
                cx,
            ))
            .child(self.section_card(
                "tunnel",
                "HTTP 隧道",
                "经 HTTP 网关转发（自定义部署）",
                network.tunnel,
                Toggle::UseTunnel,
                self.network_rows(network::TUNNEL_FIELDS, cx),
                cx,
            ))
            .children((network.tunnel || network.proxy).then(|| {
                div()
                    .text_size(px(11.0))
                    .text_color(theme.muted)
                    .child("代理与 HTTP 隧道互不影响，可同时启用；凭据只保存在本机密钥库。")
                    .into_any_element()
            }))
            .into_any_element()
    }

    pub(super) fn appearance_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let icon_type = self.icon_type().to_string();
        let icon_color = self.icon_color().to_string();
        let mut icons_grid = div().flex().flex_wrap().gap_1().max_w(px(480.0));
        for descriptor in Self::catalog_entries() {
            let key = descriptor.key;
            let on = key == icon_type;
            icons_grid = icons_grid.child(
                div()
                    .id(SharedString::from(format!("icon-{key}")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(32.0))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if on { theme.accent_soft } else { theme.canvas })
                    .border_1()
                    .border_color(if on { theme.accent } else { theme.border })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let color = this.icon_color().to_string();
                        this.set_icon(key.to_string(), color, cx);
                    }))
                    .child(icons::icon(icons::datasource_icon(key), 20.0, theme.text)),
            );
        }
        let mut swatches = div().flex().flex_wrap().gap_2().max_w(px(480.0));
        for value in ICON_COLOR_OPTIONS {
            let on = value.eq_ignore_ascii_case(&icon_color);
            swatches = swatches.child(
                div()
                    .id(SharedString::from(format!("color-{value}")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(22.0))
                    .rounded_full()
                    .cursor_pointer()
                    .bg(hex_color(value))
                    .border_1()
                    .border_color(if on { theme.accent } else { theme.border })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let icon = this.icon_type().to_string();
                        this.set_icon(icon, value.to_string(), cx);
                    }))
                    .children(on.then(|| {
                        div()
                            .text_size(px(11.0))
                            .text_color(gpui::white())
                            .child("✓")
                    })),
            );
        }
        let preview_fill = if icon_color.is_empty() {
            theme.accent
        } else {
            hex_color(&icon_color)
        };
        let preview_name = {
            let name = self.name_text(cx);
            if name.is_empty() {
                "连接名称".to_string()
            } else {
                name
            }
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(44.0))
                            .rounded_md()
                            .bg(preview_fill)
                            .child(icons::icon(
                                icons::datasource_icon(&icon_type),
                                24.0,
                                gpui::white(),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(preview_name),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.muted)
                                    .child("预览"),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(TEXT_SIZE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("图标"),
            )
            .child(icons_grid)
            .child(
                div()
                    .text_size(px(TEXT_SIZE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("颜色"),
            )
            .child(swatches)
            .into_any_element()
    }

    pub(super) fn advanced_section(&self, editors: &Editors, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.form_row("连接串", self.input_box(&editors.uri)))
            .child(
                div()
                    .ml(px(92.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .children(
                        ["解析并填充", "从字段生成", "复制"]
                            .into_iter()
                            .enumerate()
                            .map(|(index, label)| {
                                div()
                                    .id(SharedString::from(format!("uri-action-{index}")))
                                    .px_2()
                                    .py(px(3.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_size(px(12.0))
                                    .text_color(theme.accent)
                                    .hover(move |style| style.bg(theme.accent_soft))
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, _, cx| match index {
                                        0 => this.parse_uri_action(cx),
                                        1 => this.generate_uri_action(cx),
                                        _ => this.copy_uri_action(cx),
                                    }))
                            }),
                    ),
            )
            .child(self.form_row("额外参数", self.input_box(&editors.params)))
            .child(
                div()
                    .ml(px(92.0))
                    .text_size(px(11.0))
                    .text_color(theme.muted)
                    .child("URI / DSN query 参数，例如 charset=utf8mb4；认证密码请使用常规页的密码字段。"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.form_row_w(
                        "连接超时",
                        self.input_box(&editors.timeout),
                        100.0,
                    ))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child("秒，0 表示驱动默认"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.form_row_w(
                        "查询超时",
                        self.input_box(&editors.query_timeout),
                        100.0,
                    ))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child("秒，0 关闭自动停止"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .ml(px(92.0))
                            .text_size(px(TEXT_SIZE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("协议扩展配置"),
                    )
                    .child(
                        div()
                            .ml(px(92.0))
                            .flex()
                            .h(px(150.0))
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.canvas)
                            .child(editors.extra.clone()),
                    )
                    .child(
                        div()
                            .ml(px(92.0))
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child("JSON 对象；未知字段原样保留，供其它工具读取。"),
                    ),
            )
            .into_any_element()
    }

    fn protection_block(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let flags = self.flags();
        let checks: [(&'static str, &'static str, bool, Toggle); 5] = [
            (
                "readonly-connection",
                "只读保护（服务端也应使用只读账号）",
                flags.read_only,
                Toggle::ReadOnly,
            ),
            (
                "protect-data-edit",
                "限制数据编辑",
                flags.protect_edit,
                Toggle::ProtectEdit,
            ),
            (
                "protect-structure",
                "限制结构编辑",
                flags.protect_structure,
                Toggle::ProtectStructure,
            ),
            (
                "protect-script",
                "限制写入脚本执行",
                flags.protect_script,
                Toggle::ProtectScript,
            ),
            (
                "protect-import",
                "限制数据导入",
                flags.protect_import,
                Toggle::ProtectImport,
            ),
        ];
        div()
            .flex()
            .flex_col()
            .gap_1()
            .ml(px(92.0))
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.canvas)
            .child(
                div()
                    .text_size(px(TEXT_SIZE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("生产连接保护"),
            )
            .children(
                checks
                    .into_iter()
                    .map(|(id, label, on, toggle)| self.check_row(id, label, on, toggle, cx)),
            )
            .into_any_element()
    }
}
