use std::sync::Arc;

use gpui::{
    App, Application, Bounds, Context, Entity, FontWeight, IntoElement, SharedString, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use superlink_app::{DraftService, HostService, NotebookService, ProfileService, QueryService};
use superlink_domain::Profile;

use crate::note_page::NotePage;
use crate::query_page::QueryPage;
use crate::terminal_page::TerminalPage;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Workspace {
    Sql,
    Shell,
    Note,
}

impl Workspace {
    fn label(self) -> &'static str {
        match self {
            Workspace::Sql => "SQL",
            Workspace::Shell => "Shell",
            Workspace::Note => "Note",
        }
    }

}

pub struct SuperLinkApp {
    service: Arc<ProfileService>,
    query_page: Entity<QueryPage>,
    note_page: Entity<NotePage>,
    terminal_page: Entity<TerminalPage>,
    profiles: Vec<Profile>,
    selected: usize,
    workspace: Workspace,
    theme: Theme,
    error: Option<SharedString>,
}

impl SuperLinkApp {
    pub fn new(
        service: Arc<ProfileService>,
        queries: Arc<QueryService>,
        drafts: Arc<DraftService>,
        notebook: Arc<NotebookService>,
        hosts: Arc<HostService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = if window.appearance() == gpui::WindowAppearance::Dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        let (profiles, error) = match service.list() {
            Ok(profiles) => (profiles, None),
            Err(err) => (Vec::new(), Some(SharedString::from(err.to_string()))),
        };
        let query_page = cx.new(|cx| QueryPage::new(service.clone(), queries, drafts, theme, cx));
        let note_page = cx.new(|_cx| NotePage::new(notebook, theme));
        let terminal_page = cx.new(|cx| TerminalPage::new(hosts, theme, cx));
        let mut app = Self {
            service,
            query_page,
            note_page,
            terminal_page,
            profiles,
            selected: 0,
            workspace: Workspace::Sql,
            theme,
            error,
        };
        app.sync_selection(cx);
        app
    }

    fn sync_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(profile) = self.profiles.get(self.selected).cloned() {
            self.query_page
                .update(cx, |page, cx| page.select_profile(profile, cx));
        }
    }

    fn refresh(&mut self) {
        match self.service.list() {
            Ok(profiles) => {
                self.profiles = profiles;
                self.error = None;
                if self.selected >= self.profiles.len() {
                    self.selected = self.profiles.len().saturating_sub(1);
                }
            }
            Err(err) => self.error = Some(SharedString::from(err.to_string())),
        }
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut column = div()
            .flex()
            .flex_col()
            .w(px(248.0))
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border);

        column = column.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .h(px(44.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.muted)
                        .child("连接"),
                )
                .child(
                    div()
                        .id("refresh-connections")
                        .px_1()
                        .rounded_md()
                        .cursor_pointer()
                        .text_xs()
                        .text_color(theme.muted)
                        .hover(move |style| style.bg(theme.accent_soft))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.refresh();
                            cx.notify();
                        }))
                        .child(format!("{}", self.profiles.len())),
                ),
        );

        if self.profiles.is_empty() {
            column = column.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(theme.muted)
                    .child("还没有连接。数据库层接入后，这里会列出本机工作区。"),
            );
        }

        for (index, profile) in self.profiles.iter().enumerate() {
            let active = index == self.selected;
            let name = SharedString::from(profile.name.clone());
            let kind = SharedString::from(profile.sql_dialect().to_uppercase());
            let row = div()
                .id(("profile", index))
                .flex()
                .flex_col()
                .gap_1()
                .mx_2()
                .px_2()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .bg(if active {
                    theme.accent_soft
                } else {
                    theme.sidebar
                })
                .hover(move |style| style.bg(theme.accent_soft))
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.selected = index;
                    this.workspace = Workspace::Sql;
                    this.sync_selection(cx);
                    cx.notify();
                }))
                .child(
                    div()
                        .text_sm()
                        .text_color(if active { theme.accent } else { theme.text })
                        .child(name),
                )
                .child(div().text_xs().text_color(theme.muted).child(kind));
            column = column.child(row);
        }

        column
    }

    // The returned element owns only 'static data; opt out of the edition-2024
    // lifetime capture so several tabs can be built from one context.
    fn workspace_tab(
        &self,
        workspace: Workspace,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.workspace == workspace;
        let theme = self.theme;
        div()
            .id(("workspace", workspace as usize))
            .flex()
            .items_center()
            .h_full()
            .px_3()
            .cursor_pointer()
            .border_b_2()
            .border_color(if active { theme.accent } else { theme.surface })
            .text_sm()
            .text_color(if active { theme.text } else { theme.muted })
            .child(workspace.label())
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.workspace = workspace;
                cx.notify();
            }))
    }

}

impl Render for SuperLinkApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut tabs = Vec::with_capacity(3);
        for workspace in [Workspace::Sql, Workspace::Shell, Workspace::Note] {
            tabs.push(self.workspace_tab(workspace, cx));
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .font_family("PingFang SC")
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(48.0))
                    .px_4()
                    .gap_2()
                    .bg(theme.surface)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("SuperLink"),
                    )
                    .child(div().w(px(12.0)))
                    .children(tabs)
                    .child(div().flex_1())
                    .child(
                        div().text_xs().text_color(theme.muted).child(
                            self.error
                                .clone()
                                .unwrap_or_else(|| SharedString::from("原生 Rust 工作台")),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .bg(theme.canvas)
                            .child(match self.workspace {
                                Workspace::Sql => self.query_page.clone().into_any_element(),
                                Workspace::Note => self.note_page.clone().into_any_element(),
                                Workspace::Shell => self.terminal_page.clone().into_any_element(),
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(26.0))
                    .px_3()
                    .bg(theme.surface)
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted)
                    .child(format!("本地工作区 · {} 个连接", self.profiles.len())),
            )
    }
}

pub fn run(
    service: Arc<ProfileService>,
    queries: Arc<QueryService>,
    drafts: Arc<DraftService>,
    notebook: Arc<NotebookService>,
    hosts: Arc<HostService>,
) {
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("SuperLink".into()),
                        ..Default::default()
                    }),
                    focus: true,
                    ..Default::default()
                },
                |window, cx| {
                    let service = service.clone();
                    let queries = queries.clone();
                    let drafts = drafts.clone();
                    let notebook = notebook.clone();
                    let hosts = hosts.clone();
                    cx.new(|cx| {
                        SuperLinkApp::new(service, queries, drafts, notebook, hosts, window, cx)
                    })
                },
            )
            .expect("failed to open the SuperLink window");
        let _ = handle;
        cx.activate(true);
    });
}
