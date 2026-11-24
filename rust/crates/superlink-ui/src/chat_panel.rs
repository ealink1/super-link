//! Text-only AI conversation. Database results and editor contents are never
//! attached automatically; only the message the user sends leaves the app.
use crate::{editor::SqlEditor, multiline_editor::SqlTextEditor, theme::Theme};
use gpui::{AnyElement, Context, Entity, Render, SharedString, Window, div, prelude::*, px};
use std::sync::{Arc, Mutex};
use superlink_app::{
    QueryCancel,
    chat::{ChatConfig, ChatMessage, ChatService},
};

pub struct ChatPanel {
    service: Arc<ChatService>,
    theme: Theme,
    config: ChatConfig,
    messages: Vec<ChatMessage>,
    input: Entity<SqlTextEditor>,
    base: Entity<SqlEditor>,
    model: Entity<SqlEditor>,
    key: Entity<SqlEditor>,
    settings_open: bool,
    cancel: Option<QueryCancel>,
    generation: u64,
    partial: String,
    pending: Option<String>,
    error: Option<String>,
    notice: Option<String>,
}
impl ChatPanel {
    pub fn new(service: Arc<ChatService>, theme: Theme, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut e = SqlTextEditor::new(cx);
            e.set_plain(true, cx);
            e.set_theme(theme, cx);
            e.set_placeholder("输入消息（最多 8 KiB）", cx);
            e
        });
        let base = cx.new(SqlEditor::new);
        base.update(cx, |e, cx| {
            e.set_placeholder("https://api.example.com/v1", cx)
        });
        let model = cx.new(SqlEditor::new);
        model.update(cx, |e, cx| e.set_placeholder("模型名称", cx));
        let key = cx.new(SqlEditor::new);
        key.update(cx, |e, cx| {
            e.set_masked(true, cx);
            e.set_placeholder("API Key", cx);
        });
        let loader = service.clone();
        let task = cx.background_executor().spawn(async move { loader.load() });
        cx.spawn(async move |this, cx| {
            let config = task.await;
            let _ = this.update(cx, |this, cx| {
                match config {
                    Ok(config) => {
                        this.settings_open = config.base_url.is_empty();
                        this.base
                            .update(cx, |e, cx| e.set_text(config.base_url.clone(), cx));
                        this.model
                            .update(cx, |e, cx| e.set_text(config.model.clone(), cx));
                        this.key
                            .update(cx, |e, cx| e.set_text(config.api_key.clone(), cx));
                        this.config = config;
                    }
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
        Self {
            service,
            theme,
            config: ChatConfig {
                stream: true,
                ..Default::default()
            },
            messages: Vec::new(),
            input,
            base,
            model,
            key,
            settings_open: true,
            cancel: None,
            generation: 0,
            partial: String::new(),
            pending: None,
            error: None,
            notice: None,
        }
    }
    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.input.update(cx, |e, cx| e.set_theme(theme, cx));
        cx.notify();
    }
    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        handler: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        div()
            .id(id)
            .cursor_pointer()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(theme.button)
            .text_color(theme.text)
            .text_size(px(12.0))
            .hover(move |s| s.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _, _, cx| handler(this, cx)))
            .child(label)
            .into_any_element()
    }
    fn save_config(&mut self, cx: &mut Context<Self>) {
        let config = ChatConfig {
            base_url: self.base.read(cx).text().trim().into(),
            model: self.model.read(cx).text().trim().into(),
            api_key: self.key.read(cx).text().trim().into(),
            stream: self.config.stream,
        };
        if let Err(e) = config.validate() {
            self.error = Some(e);
            cx.notify();
            return;
        }
        let service = self.service.clone();
        let saved = config.clone();
        let task = cx
            .background_executor()
            .spawn(async move { service.save(&saved) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.config = config;
                        this.settings_open = false;
                        this.error = None;
                        this.notice = Some("AI 配置已加密保存".into());
                    }
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn stop(&mut self, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancel) = self.cancel.take() {
            cx.background_executor()
                .spawn(async move {
                    cancel.cancel();
                })
                .detach();
        }
        self.notice = Some("已停止；未完成回复不会加入后续上下文".into());
        cx.notify();
    }
    fn send(&mut self, cx: &mut Context<Self>) {
        if self.cancel.is_some() {
            return;
        }
        let text = self.input.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        if text.len() > 8 << 10 {
            self.error = Some("单条消息不能超过 8 KiB".into());
            cx.notify();
            return;
        }
        if let Err(e) = self.config.validate() {
            self.error = Some(e);
            self.settings_open = true;
            cx.notify();
            return;
        }
        let mut messages = self.messages.clone();
        messages.push(ChatMessage {
            role: "user".into(),
            content: text.clone(),
        });
        self.pending = Some(text);
        self.partial.clear();
        self.error = None;
        self.notice = None;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let cancel = QueryCancel::new();
        self.cancel = Some(cancel.clone());
        let service = self.service.clone();
        let config = self.config.clone();
        let latest = Arc::new(Mutex::new(None));
        let output = latest.clone();
        let task = cx.background_executor().spawn(async move {
            service.reply(&config, &messages, &cancel, |text| {
                *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_string());
            })
        });
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(32))
                    .await;
                let text = latest.lock().unwrap_or_else(|e| e.into_inner()).take();
                let alive = this
                    .update(cx, |this, cx| {
                        if this.generation != generation || this.cancel.is_none() {
                            return false;
                        }
                        if let Some(text) = text {
                            this.partial = text;
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.cancel = None;
                match result {
                    Ok(reply) => {
                        if let Some(user) = this.pending.take() {
                            this.messages.push(ChatMessage {
                                role: "user".into(),
                                content: user,
                            });
                            this.messages.push(ChatMessage {
                                role: "assistant".into(),
                                content: reply,
                            });
                        }
                        this.partial.clear();
                        this.input.update(cx, |e, cx| e.set_text("", cx));
                    }
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl Drop for ChatPanel {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            std::thread::spawn(move || cancel.cancel());
        }
    }
}
impl Render for ChatPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .w(px(400.0))
            .min_w(px(300.0))
            .h_full()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .text_color(theme.text)
            .text_size(px(13.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(px(42.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .child("AI 助手")
                    .child(div().flex_1())
                    .child(self.button(
                        "chat-new",
                        "新对话",
                        |this, cx| {
                            this.stop(cx);
                            this.messages.clear();
                            this.pending = None;
                            this.partial.clear();
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.button(
                        "chat-settings",
                        "设置",
                        |this, cx| {
                            this.settings_open = !this.settings_open;
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .children(self.settings_open.then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .bg(theme.canvas)
                    .child("API 基础地址")
                    .child(self.base.clone())
                    .child("模型")
                    .child(self.model.clone())
                    .child("API Key")
                    .child(self.key.clone())
                    .child(self.button(
                        "chat-stream",
                        if self.config.stream {
                            "流式回复：开"
                        } else {
                            "流式回复：关"
                        },
                        |this, cx| {
                            this.config.stream = !this.config.stream;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.button(
                        "chat-config-save",
                        "保存配置",
                        |this, cx| this.save_config(cx),
                        cx,
                    ))
            }))
            .child(
                div()
                    .id("chat-messages")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .gap_3()
                    .p_3()
                    .children(self.messages.iter().map(|m| {
                        div()
                            .p_3()
                            .rounded_md()
                            .bg(if m.role == "user" {
                                theme.accent_soft
                            } else {
                                theme.canvas
                            })
                            .child(SharedString::from(m.content.clone()))
                    }))
                    .children(self.pending.as_ref().map(|text| {
                        div()
                            .p_3()
                            .bg(theme.accent_soft)
                            .child(SharedString::from(text.clone()))
                    }))
                    .children((!self.partial.is_empty()).then(|| {
                        div()
                            .p_3()
                            .bg(theme.canvas)
                            .child(SharedString::from(self.partial.clone()))
                    }))
                    .children(self.error.as_ref().map(|e| {
                        div()
                            .text_color(theme.danger)
                            .child(SharedString::from(e.clone()))
                    }))
                    .children(self.notice.as_ref().map(|n| {
                        div()
                            .text_color(theme.muted)
                            .child(SharedString::from(n.clone()))
                    })),
            )
            .child(
                div()
                    .h(px(120.0))
                    .p_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(self.input.clone()),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .p_2()
                    .child(self.button("chat-stop", "停止", |this, cx| this.stop(cx), cx))
                    .child(self.button("chat-send", "发送", |this, cx| this.send(cx), cx)),
            )
    }
}
