use super::*;
pub(super) fn editor_placeholder(kind: &str) -> &'static str {
    use superlink_domain::Family;
    match superlink_domain::resolve(kind).map(|d| d.family) {
        Ok(Family::Cache) => "输入 Redis 命令，例如 GET 'key'",
        Ok(Family::Search) => "输入 REST 请求，例如 GET /_search，或输入 JSON DSL",
        Ok(Family::Document | Family::Vector | Family::Message | Family::Configuration) => {
            "使用上方表单生成命令，或输入 JSON 命令"
        }
        _ => "输入 SQL，例如 SELECT 1;",
    }
}
pub(super) struct ProtocolControls {
    pub(super) name: Entity<SqlEditor>,
    pub(super) value: Entity<SqlEditor>,
    pub(super) group: Entity<SqlEditor>,
    operation: String,
    limit: u32,
}
impl ProtocolControls {
    pub fn new(cx: &mut Context<QueryPage>) -> Self {
        let make = |hint, cx: &mut Context<QueryPage>| {
            cx.new(|cx| {
                let mut editor = SqlEditor::new(cx);
                editor.set_placeholder(hint, cx);
                editor
            })
        };
        Self {
            name: make("Key / Collection / Topic / Data ID", cx),
            value: make("Filter JSON / 查询向量 / 消息内容", cx),
            group: make("Group（默认 DEFAULT_GROUP）", cx),
            operation: "GET".into(),
            limit: 10,
        }
    }
}
impl QueryPage {
    pub(crate) fn set_protocol_target(&mut self, name: &str, cx: &mut Context<Self>) {
        self.protocol
            .name
            .update(cx, |e, cx| e.set_text(name.to_string(), cx));
    }
    fn protocol_field(&self, editor: &Entity<SqlEditor>) -> AnyElement {
        div()
            .flex()
            .flex_1()
            .min_w(px(100.))
            .h(px(30.))
            .px_2()
            .items_center()
            .border_1()
            .border_color(self.theme.border)
            .rounded_md()
            .bg(self.theme.surface)
            .child(editor.clone())
            .into_any_element()
    }
    fn protocol_button(
        &self,
        operation: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("protocol-{operation}")))
            .cursor_pointer()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(self.theme.button)
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| this.build_protocol(operation, cx)))
            .into_any_element()
    }
    fn build_protocol(&mut self, operation: &str, cx: &mut Context<Self>) {
        let Some(profile) = &self.profile else {
            return;
        };
        let operation = if profile.config.kind == "redis" {
            self.protocol.operation.as_str()
        } else {
            operation
        };
        let publishing = operation == "publish";
        let result = superlink_domain::protocol::command(
            &profile.config.kind,
            operation,
            self.protocol.name.read(cx).text(),
            self.protocol.value.read(cx).text(),
            self.protocol.group.read(cx).text(),
            &self.scope,
            self.protocol.limit,
        );
        match result {
            Ok(command) => {
                self.load_sql(command, cx);
                self.status = if publishing {
                    "发布命令已生成；执行前需确认写入"
                } else {
                    "命令已生成，点击执行后生效"
                }
                .into();
            }
            Err(error) => self.error = Some(error.into()),
        }
        cx.notify();
    }
    pub(super) fn protocol_controls(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use superlink_domain::Family;
        let profile = self.profile.as_ref()?;
        let descriptor = superlink_domain::resolve(&profile.config.kind).ok()?;
        if descriptor.family == Family::Sql {
            return None;
        }
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .text_size(px(12.))
            .text_color(self.theme.text)
            .border_b_1()
            .border_color(self.theme.border);
        if descriptor.family == Family::Search {
            return Some(
                panel
                    .child("Elasticsearch REST Console · JSON / NDJSON · 索引读取和写入分开确认")
                    .into_any_element(),
            );
        }
        let fields = div()
            .flex()
            .gap_2()
            .child(self.protocol_field(&self.protocol.name));
        panel = panel.child(if descriptor.family == Family::Cache {
            fields
        } else {
            fields.child(self.protocol_field(&self.protocol.value))
        });
        let mut actions = div().flex().flex_wrap().gap_2().items_center();
        match descriptor.family {
            Family::Cache => {
                panel = panel.child("Redis · 分批 SCAN 浏览；可生成不同类型的值读取命令");
                for operation in [
                    "GET", "HGETALL", "LRANGE", "SMEMBERS", "ZRANGE", "XRANGE", "TYPE", "TTL",
                ] {
                    actions = actions.child(
                        div()
                            .id(SharedString::from(format!("redis-{operation}")))
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(if self.protocol.operation == operation {
                                self.theme.accent_soft
                            } else {
                                self.theme.button
                            })
                            .child(operation)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.protocol.operation = operation.into();
                                cx.notify();
                            })),
                    );
                }
                actions = actions.child(self.protocol_button("read", "生成值读取命令", cx));
            }
            Family::Document => {
                panel = panel.child("MongoDB · Collection 与 Filter JSON · limit=100");
                actions = actions.child(self.protocol_button("read", "生成文档查询", cx));
            }
            Family::Vector => {
                panel = panel.child("Collection 浏览 / 原生向量检索；向量维度需与集合一致");
                actions = actions
                    .child(self.protocol_button("list", "列出 Collection", cx))
                    .child(self.protocol_button("preview", "生成条目预览", cx))
                    .child(self.protocol_button("search", "生成向量检索", cx));
            }
            Family::Message => {
                panel = panel.child(match profile.config.kind.as_str() { "rabbitmq" => "Management API 读取后 requeue，可能改变顺序和 redelivered 状态；执行前确认副作用。", "mqtt" => "仅接收订阅期间的消息，不回放离线历史；发布需写入确认。", _ => "限量预览不提交业务 Consumer Group 位点；发布需写入确认。" });
                for limit in [10, 50, 100] {
                    actions = actions.child(
                        div()
                            .id(("protocol-limit", limit as usize))
                            .cursor_pointer()
                            .px_2()
                            .bg(if self.protocol.limit == limit {
                                self.theme.accent_soft
                            } else {
                                self.theme.button
                            })
                            .child(limit.to_string())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.protocol.limit = limit;
                                cx.notify();
                            })),
                    );
                }
                actions = actions
                    .child(self.protocol_button("read", "生成限量读取", cx))
                    .child(self.protocol_button("publish", "生成发布命令", cx));
            }
            Family::Configuration => {
                panel = panel
                    .child("Nacos · Namespace 使用数据库范围，可指定配置分组")
                    .child(self.protocol_field(&self.protocol.group));
                for (op, label) in [
                    ("configs", "配置列表"),
                    ("get", "读取内容"),
                    ("services", "服务列表"),
                    ("instances", "服务实例"),
                    ("publish", "生成发布"),
                ] {
                    actions = actions.child(self.protocol_button(op, label, cx));
                }
            }
            _ => {}
        }
        Some(panel.child(actions).into_any_element())
    }
}
