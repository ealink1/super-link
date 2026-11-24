//! Each terminal owns its own transport, screen and reconnection generation.
//! Returning to the host list leaves all session entities alive.
use crate::{terminal_page::TerminalPage, theme::Theme};
mod tabs;
mod welcome;
use gpui::{App, Context, Entity, Render, SharedString, Window, div, prelude::*, px};
use std::sync::Arc;
use superlink_app::HostService;
use superlink_domain::ShellHost;

struct Tab {
    id: String,
    title: String,
    page: Entity<TerminalPage>,
}
pub struct ShellWorkspace {
    settings: Arc<superlink_app::SettingsService>,
    hosts: Arc<HostService>,
    picker: Entity<TerminalPage>,
    tabs: Vec<Tab>,
    active: Option<usize>,
    home: bool,
    rail_expanded: bool,
    theme: Theme,
    tab_menu: Option<String>,
    menu_left: gpui::Pixels,
    last_active: Option<usize>,
}
impl ShellWorkspace {
    pub(crate) fn new_terminal(&mut self, cx: &mut Context<Self>) {
        self.open(None, cx);
    }
    pub(crate) fn manage_hosts(&mut self, cx: &mut Context<Self>) {
        if self.active.is_some() {
            self.last_active = self.active;
        }
        self.home = false;
        self.active = None;
        self.tab_menu = None;
        self.picker.update(cx, |page, cx| page.open_hosts(cx));
        cx.notify();
    }
    fn show_terminals(&mut self, cx: &mut Context<Self>) {
        self.active = if self.tabs.is_empty() {
            None
        } else {
            Some(self.last_active.unwrap_or(0).min(self.tabs.len() - 1))
        };
        self.home = self.active.is_none();
        if let Some(tab) = self.active.and_then(|i| self.tabs.get(i)) {
            tab.page.update(cx, |p, cx| p.request_focus(cx));
        }
        cx.notify();
    }
    pub fn new(
        hosts: Arc<HostService>,
        settings: Arc<superlink_app::SettingsService>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = crate::shell_style::palette(theme);
        let picker = cx.new(|cx| TerminalPage::new(hosts.clone(), settings.clone(), theme, cx));
        let weak = cx.entity().downgrade();
        picker.update(cx, |page, _| {
            page.set_open_handler(move |host, app: &mut App| {
                let weak = weak.clone();
                app.defer(move |app| {
                    let _ = weak.update(app, |this, cx| this.open(host, cx));
                });
            })
        });
        let weak = cx.entity().downgrade();
        picker.update(cx, |page, _| {
            page.set_terminal_handler(move |app| {
                let weak = weak.clone();
                app.defer(move |app| {
                    let _ = weak.update(app, |this, cx| this.show_terminals(cx));
                });
            })
        });
        Self {
            settings,
            hosts,
            picker,
            tabs: Vec::new(),
            active: None,
            home: false,
            rail_expanded: false,
            theme,
            tab_menu: None,
            menu_left: px(64.),
            last_active: None,
        }
    }
    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        let theme = crate::shell_style::palette(theme);
        self.theme = theme;
        self.picker.update(cx, |p, cx| p.set_theme(theme, cx));
        for tab in &self.tabs {
            tab.page.update(cx, |p, cx| p.set_theme(theme, cx));
        }
        cx.notify();
    }
    fn open(&mut self, host: Option<ShellHost>, cx: &mut Context<Self>) {
        self.open_program(host, None, cx);
    }
    fn open_program(
        &mut self,
        host: Option<ShellHost>,
        program: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.home = false;
        self.tab_menu = None;
        if self.tabs.len() >= 32 {
            return;
        }
        let page = cx
            .new(|cx| TerminalPage::new(self.hosts.clone(), self.settings.clone(), self.theme, cx));
        let title = host
            .as_ref()
            .map(|h| format!("SSH · {}", h.name))
            .unwrap_or_else(|| "本地终端".into());
        let weak = cx.entity().downgrade();
        page.update(cx, |page, cx| {
            page.set_back_handler(move |app| {
                let weak = weak.clone();
                app.defer(move |app| {
                    let _ = weak.update(app, |this, cx| {
                        this.manage_hosts(cx);
                    });
                });
            });
            if let Some(program) = program {
                page.start_local_program(program, cx);
            } else {
                page.start_session(host, cx);
            }
        });
        self.tabs.push(Tab {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            page,
        });
        self.active = Some(self.tabs.len() - 1);
        self.last_active = self.active;
        cx.notify();
    }
    pub(crate) fn close_all(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<superlink_terminal::SessionHandle> {
        let mut handles = Vec::new();
        for tab in &self.tabs {
            if let Some(handle) = tab
                .page
                .update(cx, |page, cx| page.take_session_for_quit(cx))
            {
                handles.push(handle);
            }
        }
        self.tabs.clear();
        self.tab_menu = None;
        self.active = None;
        handles
    }
    fn close(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs[index]
            .page
            .update(cx, |page, cx| page.close_session(cx));
        self.tabs.remove(index);
        self.tab_menu = None;
        if self.tabs.is_empty() {
            self.home = true;
        }
        self.active = match self.active {
            Some(active) if active == index => {
                if self.tabs.is_empty() {
                    None
                } else {
                    Some(index.min(self.tabs.len() - 1))
                }
            }
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
        if let Some(tab) = self.active.and_then(|i| self.tabs.get(i)) {
            tab.page.update(cx, |p, cx| p.request_focus(cx));
        }
        cx.notify();
    }
}
impl Render for ShellWorkspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .on_key_down(
                cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| this.tab_key(event, cx)),
            )
            .when(self.active.is_some() || self.home, |root| root.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .h(px(34.0))
                    .text_size(px(12.))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .w(px(if self.home && self.rail_expanded {
                                144.
                            } else {
                                64.
                            }))
                            .when(self.home, |row| {
                                row.child(
                                    div()
                                        .id("welcome-toggle-navigation")
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .size(px(32.))
                                        .child(crate::icons::icon(
                                            "shell/chevron-right.svg",
                                            16.,
                                            theme.muted,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.rail_expanded = !this.rail_expanded;
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id("shell-tabs-scroll")
                            .flex()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_x_scroll()
                            .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                                let active = self.active == Some(index);
                                div()
                                    .id(SharedString::from(tab.id.clone()))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .mx_1()
                                    .h(px(30.))
                                    .rounded_md()
                                    .flex_shrink_0()
                                    .bg(if active { theme.surface } else { theme.canvas })
                                    .border_1()
                                    .border_color(theme.border)
                                    .text_size(px(12.0))
                                    .text_color(theme.text)
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                        this.menu_left = px(f32::from(event.position().x).clamp(64., f32::from(window.viewport_size().width) - 200.));
                                        this.show_tab_menu(index, cx);
                                    }))
                                    .on_mouse_down(
                                        gpui::MouseButton::Right,
                                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                            this.menu_left = px(f32::from(event.position.x).clamp(64., f32::from(window.viewport_size().width) - 200.));
                                            this.show_tab_menu(index, cx);
                                        }),
                                    )
                                    .child(div().size(px(7.)).rounded_full().bg(tab.page.read(cx).indicator_color()))
                                    .child(SharedString::from(tab.title.clone()))
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("close-{}", tab.id)))
                                            .px_1()
                                            .child("×")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.close(index, cx);
                                            })),
                                    )
                            })),
                    )
                    .child(
                        div()
                            .id("shell-new-local")
                            .cursor_pointer()
                            .px_3()
                            .text_color(theme.text)
                            .child("+")
                            .on_click(cx.listener(|this, _, _, cx| this.open(None, cx))),
                    )
                    .child(
                        div()
                            .id("shell-host-list")
                            .cursor_pointer()
                            .px_3()
                            .py_1()
                            .text_color(theme.text)
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(crate::icons::icon("shell/server.svg", 16., theme.muted))
                            .child("主机")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.manage_hosts(cx);
                            })),
                    ),
            ))
            .child(div().flex().flex_1().min_h(px(0.0)).child(
                if let Some(tab) = self.active.and_then(|i| self.tabs.get(i)) {
                    tab.page.clone().into_any_element()
                } else if self.home {
                    self.welcome(cx)
                } else {
                    self.picker.clone().into_any_element()
                },
            ))
            .children(
                self.tab_menu
                    .as_ref()
                    .and_then(|id| self.tab_menu_panel(id, cx)),
            )
    }
}
