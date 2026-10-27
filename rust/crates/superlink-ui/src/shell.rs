use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use gpui::{
    App, Application, Bounds, Context, Entity, FontWeight, IntoElement, SharedString, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use superlink_app::{
    DraftService, HistoryService, HostService, NotebookService, ProfileService, QueryService,
    SettingsService,
};
use superlink_domain::Profile;

use crate::editor::SqlEditor;
use crate::icons;
use crate::note_page::NotePage;
use crate::query_page::QueryPage;
use crate::terminal_page::TerminalPage;
use crate::theme::{Theme, ThemeMode};

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
    collapsed_groups: HashSet<String>,
    settings: Arc<SettingsService>,
    mode: ThemeMode,
    settings_open: bool,
    sql_filter: String,
    sql_search: Entity<SqlEditor>,
    workspace: Workspace,
    theme: Theme,
    error: Option<SharedString>,
}

impl SuperLinkApp {
    pub fn new(
        service: Arc<ProfileService>,
        queries: Arc<QueryService>,
        drafts: Arc<DraftService>,
        history: Arc<HistoryService>,
        notebook: Arc<NotebookService>,
        hosts: Arc<HostService>,
        settings: Arc<SettingsService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mode = ThemeMode::from_key(&settings.theme_mode());
        let theme = mode.resolve(window.appearance() == gpui::WindowAppearance::Dark);
        let (profiles, error) = match service.list() {
            Ok(profiles) => (profiles, None),
            Err(err) => (Vec::new(), Some(SharedString::from(err.to_string()))),
        };
        let query_page = cx.new(|cx| {
            QueryPage::new(
                service.clone(),
                queries,
                drafts,
                history,
                settings.clone(),
                theme,
                cx,
            )
        });
        let weak_shell = cx.entity().downgrade();
        query_page.update(cx, |page, cx| {
            page.set_on_saved(
                move |id, app| {
                    let weak_shell = weak_shell.clone();
                    let _ = weak_shell.update(app, |shell, cx| {
                        shell.select_profile_by_id(id, cx)
                    });
                },
                cx,
            );
        });
        let weak_app = cx.entity().downgrade();
        let sql_search = cx.new(SqlEditor::new);
        sql_search.update(cx, move |editor, cx| {
            editor.set_placeholder("搜索连接…", cx);
            let weak_app = weak_app.clone();
            editor.set_on_change(
                move |text, app| {
                    let weak_app = weak_app.clone();
                    let _ = weak_app.update(app, |page, cx| {
                        page.sql_filter = text.to_string();
                        cx.notify();
                    });
                },
                cx,
            );
        });
        let note_page = cx.new(|_cx| NotePage::new(notebook, theme));
        let terminal_page = cx.new(|cx| TerminalPage::new(hosts, theme, cx));
        let mut app = Self {
            service,
            query_page,
            note_page,
            terminal_page,
            profiles,
            selected: 0,
            collapsed_groups: HashSet::new(),
            settings,
            mode,
            settings_open: false,
            sql_filter: String::new(),
            sql_search: sql_search.clone(),
            workspace: Workspace::Sql,
            theme,
            error,
        };
        app.sync_selection(cx);
        app
    }

    /// Cycles 自动 -> 浅色 -> 深色 and persists the choice.
    fn cycle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_mode(self.mode.next(), window, cx);
    }

    fn set_mode(&mut self, mode: ThemeMode, window: &mut Window, cx: &mut Context<Self>) {
        let theme = mode.resolve(window.appearance() == gpui::WindowAppearance::Dark);
        self.mode = mode;
        self.theme = theme;
        self.query_page
            .update(cx, |page, cx| page.set_theme(theme, cx));
        self.terminal_page
            .update(cx, |page, cx| page.set_theme(theme, cx));
        self.note_page
            .update(cx, |page, cx| page.set_theme(theme, cx));
        self.settings.set_theme_mode(mode.as_key());
        cx.notify();
    }

    fn settings_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let modes = [
            (ThemeMode::System, "自动"),
            (ThemeMode::Light, "浅色"),
            (ThemeMode::Dark, "深色"),
        ];
        let limits = [1_000u32, 5_000, 20_000];

        let mut mode_row = div().flex().items_center().gap_2();
        for (mode, label) in modes {
            let active = self.mode == mode;
            mode_row =
                mode_row.child(
                    div()
                        .id(SharedString::from(format!("theme-mode-{}", mode.as_key())))
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(if active { theme.accent } else { theme.canvas })
                        .border_1()
                        .border_color(if active { theme.accent } else { theme.border })
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.set_mode(mode, window, cx)
                        }))
                        .child(
                            div()
                                .text_xs()
                                .text_color(if active { theme.surface } else { theme.text })
                                .child(label),
                        ),
                );
        }

        let current_limit = self.settings.row_limit();
        let mut limit_row = div().flex().items_center().gap_2();
        for limit in limits {
            let active = current_limit == limit;
            limit_row = limit_row.child(
                div()
                    .id(SharedString::from(format!("row-limit-{limit}")))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.accent } else { theme.canvas })
                    .border_1()
                    .border_color(if active { theme.accent } else { theme.border })
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.settings.set_row_limit(limit);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_xs()
                            .text_color(if active { theme.surface } else { theme.text })
                            .child(format!("{limit} 行")),
                    ),
            );
        }

        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .bg(theme.canvas)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_6()
                    .w(px(520.0))
                    .rounded_md()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(icons::icon("superlink/settings.svg", 16.0, theme.muted))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.text)
                                    .child("设置"),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id("close-settings")
                                    .px_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.accent_soft))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.settings_open = false;
                                        cx.notify();
                                    }))
                                    .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_xs().text_color(theme.muted).child("外观"))
                            .child(mode_row),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted)
                                    .child("结果行数上限"),
                            )
                            .child(limit_row),
                    ),
            )
    }

    /// A connection was saved in the form: refresh, select it, open its query.
    fn select_profile_by_id(&mut self, id: &str, cx: &mut Context<Self>) {
        self.refresh();
        if let Some(index) = self.profiles.iter().position(|profile| profile.id == id) {
            self.selected = index;
        }
        self.workspace = Workspace::Sql;
        self.sync_selection(cx);
        cx.notify();
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
                        .flex()
                        .items_center()
                        .gap_1()
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
                        .child(icons::icon("superlink/refresh.svg", 13.0, theme.muted))
                        .child(format!("{}", self.profiles.len())),
                ),
        );

        column = column.child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .pb_2()
                .child(
                    div()
                        .id("new-query-side")
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.accent_soft))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.query_page.update(cx, |page, cx| page.add_tab(cx));
                            this.workspace = Workspace::Sql;
                            cx.notify();
                        }))
                        .child(icons::icon("superlink/new-query.svg", 14.0, theme.muted))
                        .child(div().text_xs().text_color(theme.muted).child("新建查询")),
                )
                .child(
                    div()
                        .id("new-connection-side")
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.accent_soft))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.query_page
                                .update(cx, |page, cx| page.open_connection_form(cx));
                            this.workspace = Workspace::Sql;
                            cx.notify();
                        }))
                        .child(icons::icon(
                            "superlink/new-connection.svg",
                            14.0,
                            theme.muted,
                        ))
                        .child(div().text_xs().text_color(theme.muted).child("新建连接")),
                )
                .child(
                    div()
                        .id("history-side")
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.accent_soft))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.workspace = Workspace::Sql;
                            this.query_page
                                .update(cx, |page, cx| page.toggle_history(cx));
                            cx.notify();
                        }))
                        .child(icons::icon("superlink/history.svg", 14.0, theme.muted))
                        .child(div().text_xs().text_color(theme.muted).child("执行历史")),
                ),
        );

        column = column.child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .mx_2()
                .mb_2()
                .px_2()
                .h(px(28.0))
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .child(icons::icon("shell/search.svg", 13.0, theme.muted))
                .child(self.sql_search.clone()),
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

        let needle = self.sql_filter.to_lowercase();
        let mut groups: BTreeMap<String, Vec<(usize, &Profile)>> = BTreeMap::new();
        for (index, profile) in self.profiles.iter().enumerate() {
            if !needle.is_empty()
                && !profile.name.to_lowercase().contains(&needle)
                && !profile.sql_dialect().to_lowercase().contains(&needle)
                && !profile.group.to_lowercase().contains(&needle)
            {
                continue;
            }
            let key = if profile.group.trim().is_empty() {
                "未分组".to_string()
            } else {
                profile.group.clone()
            };
            groups.entry(key).or_default().push((index, profile));
        }

        for (group, entries) in groups {
            let collapsed = self.collapsed_groups.contains(&group);
            let group_key = group.clone();
            column = column.child(
                div()
                    .id(SharedString::from(format!("group-{group}")))
                    .flex()
                    .items_center()
                    .gap_1()
                    .mx_2()
                    .mt_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if !this.collapsed_groups.remove(&group_key) {
                            this.collapsed_groups.insert(group_key.clone());
                        }
                        cx.notify();
                    }))
                    .child(div().text_xs().text_color(theme.muted).child(if collapsed {
                        "▸"
                    } else {
                        "▾"
                    }))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.muted)
                            .child(SharedString::from(group.clone())),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted)
                            .child(format!("{}", entries.len())),
                    ),
            );
            if collapsed {
                continue;
            }
            for (index, profile) in entries {
                let active = index == self.selected;
                let name = SharedString::from(profile.name.clone());
                let kind = SharedString::from(profile.sql_dialect().to_uppercase());
                let row = div()
                    .id(("profile", index))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .mx_2()
                    .ml_5()
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
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(icons::icon(
                                icons::datasource_icon(profile.sql_dialect()),
                                14.0,
                                if active { theme.accent } else { theme.muted },
                            ))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if active { theme.accent } else { theme.text })
                                    .child(name),
                            ),
                    )
                    .child(
                        div()
                            .pl(px(22.0))
                            .text_xs()
                            .text_color(theme.muted)
                            .child(kind),
                    );
                column = column.child(row);
            }
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
                        div()
                            .id("theme-toggle")
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(
                                cx.listener(|this, _event, window, cx| {
                                    this.cycle_theme(window, cx)
                                }),
                            )
                            .child(icons::icon("superlink/sliders.svg", 14.0, theme.muted))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted)
                                    .child(self.mode.label()),
                            ),
                    )
                    .child(
                        div()
                            .id("open-settings")
                            .flex()
                            .items_center()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.settings_open = true;
                                cx.notify();
                            }))
                            .child(icons::icon("superlink/settings.svg", 15.0, theme.muted)),
                    )
                    .child(
                        div().text_xs().text_color(theme.muted).child(
                            self.error
                                .clone()
                                .unwrap_or_else(|| SharedString::from("原生 Rust 工作台")),
                        ),
                    ),
            )
            .child(if self.settings_open {
                self.settings_view(cx).into_any_element()
            } else if self.workspace == Workspace::Sql {
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
                    )
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(match self.workspace {
                        Workspace::Shell => self.terminal_page.clone().into_any_element(),
                        Workspace::Note => self.note_page.clone().into_any_element(),
                        Workspace::Sql => div().into_any_element(),
                    })
                    .into_any_element()
            })
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
    history: Arc<HistoryService>,
    notebook: Arc<NotebookService>,
    hosts: Arc<HostService>,
    settings: Arc<SettingsService>,
) {
    Application::new()
        .with_assets(icons::Assets::discover())
        .run(move |cx: &mut App| {
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
                        let history = history.clone();
                        let notebook = notebook.clone();
                        let hosts = hosts.clone();
                        let settings = settings.clone();
                        cx.new(|cx| {
                            SuperLinkApp::new(
                                service, queries, drafts, history, notebook, hosts, settings,
                                window, cx,
                            )
                        })
                    },
                )
                .expect("failed to open the SuperLink window");
            let _ = handle;
            cx.activate(true);
        });
}
