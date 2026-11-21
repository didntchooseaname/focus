use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use focus_core::NoteId;
use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Forced, Mode, Type, size, typography::SANS};
use gpui::{
    AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    MouseMoveEvent, ParentElement, Pixels, Render, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};

use crate::actions::*;
use crate::editor::{EditorEvent, NoteEditor, ViewMode};
use crate::kit::Gallery;
use crate::library::Library;
use crate::palette::{Palette, PaletteEvent};
use crate::profile;
use crate::settings::schema::{Prefs, PrefsEvent, Section, Value};
use crate::settings::{Facts, SettingsEvent, SettingsView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeftTab {
    Files,
    Search,
    Tags,
    Bookmarks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightTab {
    Outline,
    Links,
    Info,
    Ai,
    Ops,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BottomTab {
    Journal,
    Tasks,
    Broken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Bottom,
    Split,
}

pub struct Layout {
    pub left_tab: LeftTab,
    pub left_open: bool,
    pub left_w: Pixels,
    pub right_tab: RightTab,
    pub right_open: bool,
    pub right_w: Pixels,
    pub bottom_tab: BottomTab,
    pub bottom_open: bool,
    pub bottom_h: Pixels,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            left_tab: LeftTab::Files,
            left_open: true,
            left_w: size::LEFT,
            right_tab: RightTab::Outline,
            right_open: true,
            right_w: size::RIGHT,
            bottom_tab: BottomTab::Tasks,
            bottom_open: false,
            bottom_h: size::BOTTOM,
        }
    }
}

pub enum Overlay {
    None,
    Zoom(String),
    Palette {
        view: Entity<Palette>,
        _events: Subscription,
    },
    Settings {
        view: Entity<SettingsView>,
        _events: Subscription,
    },
    OpsDialog(Entity<focus_view_ops::NewEngagementDialog>),
    Transfer {
        view: gpui::AnyView,
        _events: Subscription,
    },
    Versions {
        view: Entity<focus_view_history::VersionsDialog>,
        _events: Subscription,
    },
    OpenAddress(String),
    Crumbs(crate::crumbs::CrumbMenu),
    #[cfg_attr(
        not(unix),
        expect(dead_code, reason = "no second launch reaches a window")
    )]
    SwitchVault(crate::instance::VaultSwitch),
    TabMenu(crate::groups::TabMenu),
    ObsidianWrite(crate::obsidian::Confirmation),
    Capture(crate::capture::CaptureDialog),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Graph,
    Ops(crate::ops::OpsScreen),
    Base(u64),
    File(u64),
}

pub enum Screen {
    Workspace,
    Kit(Entity<Gallery>),
    Welcome { selected: usize },
    Updates { offered: bool },
    Vaults,
}

pub struct Drag {
    pub edge: Edge,
    pub origin: Pixels,
    pub start: Pixels,
}

pub struct Shell {
    pub focus: FocusHandle,
    pub scripted: bool,
    pub library: Option<Arc<Library>>,
    pub notices: crate::notices::Notices,
    pub right_scroll: gpui::ScrollHandle,
    pub outline_scroll: gpui::UniformListScrollHandle,
    pub vault_root: Option<PathBuf>,
    pub vault_error: Option<String>,
    pub pending: usize,
    pub vault_views: Option<crate::vault_views::VaultViews>,
    pub tabs: Vec<Entity<NoteEditor>>,
    pub active: Option<usize>,
    pub(crate) history: Vec<NoteId>,
    pub(crate) history_pos: usize,
    pub split: Option<crate::groups::Split>,
    pub layout: Layout,
    pub overlay: Overlay,
    pub modal: crate::modal::ModalFocus,
    pub screen: Screen,
    pub prefs: Entity<Prefs>,
    pub search: Entity<focus_view_search::SearchPanel>,
    pub drag: Option<Drag>,
    pub forced: HashMap<String, Forced>,
    pub facts: Facts,
    first_frame_seen: bool,
    pub pending_line: Option<(NoteId, u32)>,
    pub slash: Option<crate::editing::Slash>,
    pub slash_catalog: crate::editing::SlashCatalog,
    pub selbar: crate::selbar::SelectionBar,
    pub editor_bounds: crate::editing::EditorBounds,
    pub store: crate::session::Store,
    save_task: Option<gpui::Task<()>>,
    saved_session: crate::session::Session,
    restore: Option<crate::session::Session>,
    pub ai: Option<Entity<focus_view_ai::AiPanel>>,
    pub ai_guard: Option<PathBuf>,
    pub ai_note: crate::ai::ActiveCell,
    rebuilding: bool,
    pub search_assist: Option<std::sync::Arc<dyn focus_view_search::assist::AssistBackend>>,
    pub find: Option<crate::find::FindBar>,
    pub code_decoration: Option<std::sync::Arc<crate::ops::OpsVariables>>,
    pub link_preview: Option<Entity<focus_view_reading::LinkPreview>>,
    pub preview: Option<(NoteId, gpui::Point<Pixels>)>,
    pub graph: Option<Entity<focus_view_graph::GraphView>>,
    pub local_graph: Option<Entity<focus_view_graph::LocalGraph>>,
    pub graph_echo: bool,
    pub ops: Option<crate::ops::OpsViews>,
    pub panes: Vec<Pane>,
    pub active_pane: Option<usize>,
    pub(crate) agent_server: Option<focus_mcp::ipc::ReferenceServer>,
    pub agent: crate::ipc::AgentPresence,
    pub obsidian: crate::obsidian::Pending,
    pub loose_watch: Option<crate::loose::watch::Watch>,
    pub open_paths: Vec<PathBuf>,
    pub bases: Vec<crate::bases::BasePage>,
    pub next_base: u64,
    pub files: crate::media::Files,
    pub import: Option<focus_view_vault::ImportStatus>,
    pub history_error: Option<String>,
    pub updates: crate::update::Updates,
    applied_history: Option<focus_core::HistoryConfig>,
    pub closed: Vec<crate::tab_keys::Closed>,
    pub right_strip: gpui::FocusHandle,
    pub bottom_strip: gpui::FocusHandle,
    pub wheel_zoom: std::rc::Rc<std::cell::Cell<f32>>,
    pub _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(
        vault: Option<PathBuf>,
        open: Vec<PathBuf>,
        scripted: bool,
        store: crate::session::Store,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let values = store.values();
        let prefs = cx.new(|_| {
            let mut prefs = Prefs::new();
            prefs.load(values);
            prefs
        });
        focus_kit::fade::set_animate(
            !scripted && !prefs.read(cx).bool("appearance.reduce_motion"),
            cx,
        );
        focus_kit::size::set_compact(prefs.read(cx).choice("appearance.density") == 1);
        let first_launch = store.first_launch();
        let reopen = !first_launch && vault.is_none();
        let vault = vault.or_else(|| {
            if reopen {
                store.settings.last_vault.clone().filter(|p| p.is_dir())
            } else {
                None
            }
        });
        let restore = (reopen && vault.is_some()).then(|| store.session());
        let search = cx.new(|cx| focus_view_search::SearchPanel::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(
                &search,
                window,
                |this, _, event: &focus_view_search::SearchEvent, window, cx| match event {
                    focus_view_search::SearchEvent::OpenNote {
                        note,
                        line,
                        new_tab,
                    } => {
                        this.open_note(*note, *new_tab, window, cx);
                        if let Some(line) = line {
                            this.pending_line = Some((*note, *line));
                        }
                    }
                    focus_view_search::SearchEvent::FocusReleased => this.focus_content(window, cx),
                    focus_view_search::SearchEvent::NotesWritten(notes) => {
                        this.reload_notes(notes, window, cx)
                    }
                },
            ),
            cx.subscribe_in(&prefs, window, |this, _, event: &PrefsEvent, window, cx| {
                let PrefsEvent::Changed(key) = event;
                this.apply_pref(key, window, cx);
            }),
        ];
        let focus = cx.focus_handle();
        window.focus(&focus);
        let weak = cx.entity().downgrade();
        let interceptor = cx.intercept_keystrokes(move |event, window, cx| {
            let consumed = weak
                .update(cx, |shell, cx| shell.intercept(event, window, cx))
                .unwrap_or(false);
            if consumed {
                cx.stop_propagation();
            }
        });
        let mut shell = Shell {
            focus,
            scripted,
            library: None,
            notices: Default::default(),
            right_scroll: gpui::ScrollHandle::new(),
            outline_scroll: gpui::UniformListScrollHandle::new(),
            vault_root: vault.clone(),
            vault_error: None,
            pending: 0,
            vault_views: None,
            tabs: Vec::new(),
            active: None,
            history: Vec::new(),
            history_pos: 0,
            split: None,
            layout: Layout::default(),
            overlay: Overlay::None,
            modal: crate::modal::ModalFocus::new(cx),
            screen: Screen::Workspace,
            prefs,
            search,
            drag: None,
            forced: HashMap::new(),
            facts: Facts {
                window_ms: Some(profile::elapsed_ms()),
                ..Facts::default()
            },
            first_frame_seen: false,
            pending_line: None,
            slash: None,
            slash_catalog: crate::editing::SlashCatalog::new(),
            selbar: Default::default(),
            editor_bounds: crate::editing::EditorBounds::default(),
            store,
            save_task: None,
            saved_session: crate::session::Session::default(),
            restore,
            ai: None,
            ai_guard: None,
            ai_note: Default::default(),
            search_assist: None,
            rebuilding: false,
            find: None,
            code_decoration: None,
            link_preview: None,
            preview: None,
            graph: None,
            local_graph: None,
            graph_echo: false,
            ops: None,
            panes: Vec::new(),
            active_pane: None,
            agent_server: None,
            agent: Default::default(),
            loose_watch: None,
            open_paths: open,
            obsidian: Default::default(),
            bases: Vec::new(),
            next_base: 0,
            files: Default::default(),
            import: None,
            history_error: None,
            updates: Default::default(),
            applied_history: None,
            closed: Vec::new(),
            right_strip: cx.focus_handle(),
            bottom_strip: cx.focus_handle(),
            wheel_zoom: Default::default(),
            _subscriptions: subscriptions,
        };
        shell._subscriptions.push(interceptor);
        shell._subscriptions.push(cx.on_app_quit(|this, cx| {
            this.flush_all(cx);
            async {}
        }));
        let closing = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = closing.update(cx, |shell, cx| shell.flush_all(cx));
            true
        });
        if let Some(session) = &shell.restore {
            if let Some((open, width)) = session.left {
                shell.layout.left_open = open;
                shell.layout.left_w = px(width);
            }
            if let Some((open, width)) = session.right {
                shell.layout.right_open = open;
                shell.layout.right_w = px(width);
            }
            if let Some((open, height)) = session.bottom {
                shell.layout.bottom_open = open;
                shell.layout.bottom_h = px(height);
            }
        }
        crate::actions::set_overrides(&shell.prefs.read(cx).text("shortcuts.overrides"));
        shell.migrate_mcp_secrets(cx);
        cx.background_spawn(async { crate::fonts::warm() }).detach();
        shell.saved_session = shell.session_now(cx);
        shell.apply_pref("appearance.theme", window, cx);
        shell.apply_pref("appearance.ui_scale", window, cx);
        shell.apply_pref("appearance.remote_images", window, cx);
        if first_launch {
            shell.screen = Screen::Welcome {
                selected: crate::welcome::preselected(),
            };
        } else if vault.is_none() {
            shell.screen = Screen::Vaults;
        }
        if let Some(root) = vault {
            shell.load_vault(root, window, cx);
        }
        shell.watch_updates(crate::update::AT_LAUNCH, cx);
        shell
    }

    pub fn apply_language(&mut self, code: &str, window: &mut Window, cx: &mut Context<Self>) {
        let code = if code == "system" {
            focus_i18n::detect_system()
        } else {
            code.to_string()
        };
        if let Err(error) = focus_i18n::set_language(&code) {
            eprintln!("focus: language: {error}");
            return;
        }
        gpui_component::set_locale(&code);
        self.slash_catalog = crate::editing::SlashCatalog::new();
        self.slash = None;
        window.set_window_title(&focus_i18n::t("ui-app-name"));
        if let Overlay::Palette { .. } = self.overlay {
            self.overlay = Overlay::None;
        }
        for tab in self.all_tabs() {
            if tab.read(cx).loose.is_some() && !tab.read(cx).parked {
                tab.update(cx, |editor, cx| {
                    editor.schedule_parse(std::time::Duration::ZERO, cx)
                });
            }
        }
        cx.refresh_windows();
    }

    fn refresh_store(&mut self, cx: &mut Context<Self>) {
        let values: Vec<(&'static str, Value)> = {
            let prefs = self.prefs.read(cx);
            crate::settings::schema::SETTINGS
                .iter()
                .filter_map(|s| prefs.get(s.key).map(|v| (s.key, v.clone())))
                .collect()
        };
        self.store.set_values(values.iter().map(|(k, v)| (*k, v)));
        let session = self.session_now(cx);
        self.store.set_session(&session);
        self.saved_session = session;
    }

    pub fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.refresh_store(cx);
        let settings = self.store.settings.clone();
        let writable = self.store.writable();
        if !writable {
            return;
        }
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(300))
                .await;
            let result = cx
                .background_spawn(async move { crate::session::write(&settings) })
                .await;
            if let Err(error) = result {
                eprintln!("focus: writing the settings: {error:#}");
                let message = focus_i18n::t_args(
                    "errors-settings-write",
                    &[("detail", format!("{error:#}").as_str().into())],
                );
                let _ = this.update(cx, |this, cx| {
                    this.notify_user(focus_kit::Tone::Danger, message, cx)
                });
            }
        }));
    }

    pub fn session_now(&self, cx: &gpui::App) -> crate::session::Session {
        let group = |tabs: &[Entity<NoteEditor>], active: Option<usize>| {
            let kept: Vec<usize> = (0..tabs.len())
                .filter(|&i| tabs[i].read(cx).loose.is_none())
                .collect();
            crate::session::GroupSession {
                tabs: kept
                    .iter()
                    .map(|&i| tabs[i].read(cx).meta.path.to_path_buf())
                    .collect(),
                views: kept.iter().map(|&i| Some(tabs[i].read(cx).view)).collect(),
                active: active.and_then(|active| kept.iter().position(|&i| i == active)),
            }
        };
        let focused = group(&self.tabs, self.active);
        let (first, second) = match &self.split {
            None => (focused, None),
            Some(split) => {
                let other = group(&split.other.tabs, split.other.active);
                if split.focused_second {
                    (other, Some(focused))
                } else {
                    (focused, Some(other))
                }
            }
        };
        crate::session::Session {
            tabs: first.tabs,
            views: first.views,
            active: first.active,
            share: self.split.as_ref().map(|split| split.share),
            focus_second: self.focused_second(),
            second,
            left: Some((self.layout.left_open, f32::from(self.layout.left_w))),
            right: Some((self.layout.right_open, f32::from(self.layout.right_w))),
            bottom: Some((self.layout.bottom_open, f32::from(self.layout.bottom_h))),
            expanded: self.expanded_folders(cx),
        }
    }

    fn restore_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.restore.take() else {
            return;
        };
        let Some(library) = self.library.clone() else {
            return;
        };
        if !self.prefs.read(cx).bool("general.reopen_tabs") {
            return;
        }
        let first = crate::session::GroupSession {
            tabs: session.tabs.clone(),
            views: session.views.clone(),
            active: session.active,
        };
        self.restore_group(&first, &library, window, cx);
        if let Some(second) = session.second.as_ref().filter(|g| !g.tabs.is_empty())
            && !self.tabs.is_empty()
        {
            self.split = Some(crate::groups::Split {
                other: Default::default(),
                focused_second: false,
                share: crate::groups::clamp_share(session.share.unwrap_or(crate::groups::SHARE)),
            });
            self.focus_group(true, window, cx);
            self.restore_group(second, &library, window, cx);
            self.drop_empty_group(window, cx);
            if !session.focus_second {
                self.focus_group(false, window, cx);
            }
        }
    }

    fn restore_group(
        &mut self,
        group: &crate::session::GroupSession,
        library: &Library,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (index, path) in group.tabs.iter().enumerate() {
            let Some(id) = library.note_by_path(path) else {
                continue;
            };
            self.open_note(id, true, window, cx);
            if let (Some(Some(view)), Some(editor)) =
                (group.views.get(index), self.active_editor().cloned())
            {
                editor.update(cx, |editor, cx| editor.set_view(*view, window, cx));
            }
        }
        if let Some(active) = group.active.filter(|a| *a < self.tabs.len()) {
            self.activate(active, window, cx);
        }
    }

    pub fn load_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.pending += 1;
        self.vault_root = Some(root.clone());
        self.vault_error = None;
        let ignore: Vec<String> = match self.prefs.read(cx).get("files.exclude") {
            Some(Value::Text(text)) => text
                .split([',', '\n'])
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        };
        let external = if self.prefs.read(cx).bool("files.follow_external_links") {
            focus_core::ExternalLinks::Follow
        } else {
            focus_core::ExternalLinks::Skip
        };
        let history = self.history_config();
        self.applied_history = history;
        let task =
            cx.background_spawn(async move { Library::open(&root, ignore, external, history) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.pending -= 1;
                match result {
                    Ok(library) => this.vault_loaded(library, window, cx),
                    Err(error) => {
                        eprintln!("focus: opening the vault: {error:#}");
                        this.vault_error = Some(focus_i18n::t_args(
                            "errors-vault-open",
                            &[("detail", format!("{error:#}").as_str().into())],
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn switch_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.library.is_some() {
            self.flush_all(cx);
        }
        self.split = None;
        self.tabs.clear();
        self.active = None;
        self.history.clear();
        self.history_pos = 0;
        self.panes.clear();
        self.active_pane = None;
        self.bases.clear();
        self.forget_files(cx);
        self.graph = None;
        self.ops = None;
        self.code_decoration = None;
        self.find = None;
        self.slash = None;
        self.preview = None;
        self.import = None;
        self.history_error = None;
        self.restore = None;
        self.library = None;
        focus_ops::detach_history();
        self.vault_views = None;
        self.agent_server = None;
        self.overlay = Overlay::None;
        self.screen = Screen::Workspace;
        self.load_vault(root, window, cx);
    }

    pub fn rebuild_index(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.vault_root.clone() else {
            return;
        };
        self.flush_all(cx);
        let session = self.session_now(cx);
        let cache = focus_core::settings::cache_dir_for(&root);
        if let Err(error) = std::fs::remove_dir_all(&cache)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            self.notify_user(
                focus_kit::Tone::Danger,
                crate::errors::vault(&focus_core::VaultError::Io {
                    path: cache,
                    source: error,
                }),
                cx,
            );
            return;
        }
        self.rebuilding = true;
        self.restore = Some(session);
        self.split = None;
        self.tabs.clear();
        self.active = None;
        self.library = None;
        self.vault_views = None;
        self.agent_server = None;
        self.load_vault(root, window, cx);
    }

    fn history_config(&self) -> Option<focus_core::HistoryConfig> {
        let capture_without_data_dir =
            self.scripted && std::env::var_os("FOCUS_DATA_DIR").is_none_or(|v| v.is_empty());
        (!capture_without_data_dir).then_some(self.store.settings.history)
    }

    fn apply_history_settings(&mut self, cx: &mut Context<Self>) {
        let config = self.history_config();
        if config == self.applied_history {
            return;
        }
        let (Some(config), Some(library)) = (config, self.library.clone()) else {
            return;
        };
        self.applied_history = Some(config);
        let vault = library.vault().clone();
        let task = cx.background_spawn(async move {
            vault.set_history(&focus_core::settings::history_dir_for(vault.root()), config)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Err(error) => this.show_history_error(&error, cx),
                Ok(()) => {
                    let open = this.library.as_ref();
                    if open.is_some_and(|open| Arc::ptr_eq(open, &library)) {
                        crate::library::attach_ops_history(library.vault());
                    }
                }
            });
        })
        .detach();
    }

    fn show_history_error(&mut self, error: &focus_core::VaultError, cx: &mut Context<Self>) {
        eprintln!("focus: note history: {error}");
        self.history_error = Some(crate::errors::vault(error));
        cx.notify();
    }

    pub fn poll_history_error(&mut self, cx: &mut Context<Self>) {
        if let Some(error) = self
            .library
            .as_ref()
            .and_then(|library| library.vault().take_history_error())
        {
            self.show_history_error(&error, cx);
        }
    }

    fn vault_loaded(&mut self, mut library: Library, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(error) = library.history_error.take() {
            self.show_history_error(&error, cx);
        }
        if std::mem::take(&mut self.rebuilding) {
            let text = focus_i18n::t_args(
                "ui-toast-indexed",
                &[
                    ("count", library.note_count().into()),
                    (
                        "duration",
                        focus_i18n::format_duration(std::time::Duration::from_millis(
                            library.open_ms as u64,
                        ))
                        .as_str()
                        .into(),
                    ),
                ],
            );
            self.notify_user(focus_kit::Tone::Ok, text, cx);
        }
        let notes = library.note_count();
        profile::mark(&format!(
            "vault open ({notes} notes, vault and index {:.0} ms)",
            library.open_ms
        ));
        self.facts.vault_open_ms = Some(library.open_ms);
        self.facts.notes = notes;
        self.facts.vault = Some(library.root().display().to_string());
        let library = Arc::new(library);
        let events = library.events();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if this
                    .update_in(cx, |this, window, cx| this.vault_event(event, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let persisted = library.clone();
        cx.background_spawn(async move {
            if let Err(error) = persisted.persist_index() {
                eprintln!("focus: writing the index: {error:#}");
            }
        })
        .detach();
        self.library = Some(library);
        self.refresh_search_services(window, cx);
        let expanded = self
            .restore
            .as_ref()
            .map(|session| session.expanded.clone())
            .unwrap_or_default();
        self.mount_vault_views(expanded, window, cx);
        if let Some(root) = self.vault_root.clone() {
            self.store.opened_vault(&root);
        }
        self.restore_tabs(window, cx);
        self.open_given_paths(window, cx);
        self.follow_pending_addresses(window, cx);
        self.mount_ai(window, cx);
        self.mount_local_graph(window, cx);
        self.sync_ops(window, cx);
        self.sync_agent_socket(window, cx);
        self.schedule_save(cx);
    }

    fn vault_event(
        &mut self,
        event: focus_core::VaultEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use focus_core::VaultEvent;
        let Some(library) = self.library.clone() else {
            return;
        };
        library.apply(&event);
        match &event {
            VaultEvent::NoteChanged(id) | VaultEvent::NoteMoved { id, .. } => {
                for tab in self.all_tabs() {
                    if tab.read(cx).id == *id {
                        tab.update(cx, |editor, cx| {
                            if let Some(task) = editor.external_change(window, cx) {
                                task.detach();
                            }
                            if matches!(event, VaultEvent::NoteMoved { .. }) && !editor.parked {
                                editor.schedule_parse(std::time::Duration::ZERO, cx);
                            }
                        });
                    }
                }
            }
            VaultEvent::NoteRemoved { id, .. } => {
                for _ in 0..2 {
                    while let Some(index) = self.tabs.iter().position(|tab| tab.read(cx).id == *id)
                    {
                        self.tabs[index].update(cx, |editor, _| editor.discard());
                        self.close_tab(index, window, cx);
                    }
                    if self.in_other_group(*id, cx).is_some() {
                        let other = !self.focused_second();
                        self.focus_group(other, window, cx);
                    }
                }
            }
            VaultEvent::NoteAdded(_) | VaultEvent::TreeChanged => {}
        }
        self.rescan_copies(&event, &library, cx);
        self.poll_history_error(cx);
        self.search
            .update(cx, |panel, cx| panel.refresh(window, cx));
        self.ai_vault_event(&event, window, cx);
        self.graph_vault_event(&event, cx);
        self.vault_views_changed(&event, cx);
        if !matches!(event, VaultEvent::NoteChanged(_)) {
            self.facts.notes = library.note_count();
        }
        cx.notify();
    }

    fn rescan_copies(
        &mut self,
        event: &focus_core::VaultEvent,
        library: &crate::library::Library,
        cx: &mut Context<Self>,
    ) {
        use focus_core::VaultEvent;
        let folder = |path: &std::path::Path| {
            path.parent()
                .unwrap_or(std::path::Path::new(""))
                .to_path_buf()
        };
        let (folders, moved) = match event {
            VaultEvent::NoteAdded(id) => (
                library
                    .note(*id)
                    .map(|m| folder(&m.path))
                    .into_iter()
                    .collect(),
                None,
            ),
            VaultEvent::NoteRemoved { path, .. } => (vec![folder(path)], None),
            VaultEvent::NoteMoved { id, from, to } => (vec![folder(from), folder(to)], Some(*id)),
            VaultEvent::NoteChanged(_) | VaultEvent::TreeChanged => return,
        };
        for tab in self.all_tabs() {
            let concerned = {
                let editor = tab.read(cx);
                Some(editor.id) == moved || folders.iter().any(|f| editor.in_folder(f))
            };
            if concerned {
                tab.update(cx, |editor, cx| editor.scan_copies(cx));
            }
        }
    }

    pub fn settled(&self, cx: &gpui::App) -> bool {
        self.pending == 0
            && !self.files_busy(cx)
            && self.all_tabs().iter().all(|t| !t.read(cx).busy())
            && focus_view_reading::media::loading(cx) == 0
    }

    pub fn typesetting(&self, cx: &gpui::App) -> focus_view_reading::view::Typesetting {
        let prefs = self.prefs.read(cx);
        focus_view_reading::view::Typesetting {
            base: prefs.number("appearance.reading_size") as f32,
            measure: prefs.number("appearance.measure") as f32,
            leading: prefs.number("appearance.reading_leading") as f32,
            tinted: prefs.choice("appearance.code_colors") == 0,
            text_font: match prefs.choice("appearance.reading_font") {
                0 => "Inter".into(),
                1 => "Sora".into(),
                _ => crate::fonts::system_family(crate::fonts::Generic::Sans),
            },
            code_font: match prefs.choice("appearance.code_font") {
                0 => "JetBrains Mono".into(),
                _ => crate::fonts::system_family(crate::fonts::Generic::Mono),
            },
        }
    }

    pub fn follow_link(
        &mut self,
        goal: focus_view_reading::model::LinkGoal,
        new_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use focus_view_reading::model::LinkGoal;
        match goal {
            LinkGoal::Note { id, anchor } => {
                self.open_note(id, new_tab, window, cx);
                if let (Some(anchor), Some(editor)) = (anchor, self.active_editor().cloned()) {
                    let reading = editor.read(cx).reading.clone();
                    reading.update(cx, |reading, _| {
                        let line = reading.built().and_then(|b| {
                            b.headings
                                .iter()
                                .find(|h| h.text.eq_ignore_ascii_case(&anchor))
                                .map(|h| h.line)
                        });
                        if let Some(line) = line {
                            reading.scroll_to_line(line);
                        }
                    });
                }
            }
            LinkGoal::External(url) => self.open_address(&url, window, cx),
            LinkGoal::Attachment(path) if crate::bases::is_base(&path) => {
                self.open_base(&path, window, cx)
            }
            LinkGoal::Attachment(path) if crate::media::opens(&path) => {
                if let Some(absolute) = self.library.as_ref().map(|library| library.absolute(&path))
                {
                    let page = self.files.link_page.take();
                    self.open_media(&absolute, page, window, cx);
                }
            }
            LinkGoal::Attachment(path) => {
                if let Some(absolute) = self.library.as_ref().map(|library| library.absolute(&path))
                {
                    self.open_file(&absolute, cx);
                }
            }
            LinkGoal::Tag(tag) => {
                self.layout.left_open = true;
                self.layout.left_tab = LeftTab::Search;
                let query = format!("tag:{tag}");
                self.search
                    .update(cx, |panel, cx| panel.set_query(&query, window, cx));
                cx.notify();
            }
            LinkGoal::Footnote(_) => {}
            LinkGoal::Unresolved(target) => self.notify_user(
                focus_kit::Tone::Warn,
                focus_i18n::t_args(
                    "reading-link-unresolved",
                    &[("target", target.as_str().into())],
                ),
                cx,
            ),
        }
    }

    pub fn all_tabs(&self) -> Vec<Entity<NoteEditor>> {
        let mut tabs = self.tabs.clone();
        if let Some(split) = &self.split {
            tabs.extend(split.other.tabs.iter().cloned());
        }
        tabs
    }

    pub fn active_editor(&self) -> Option<&Entity<NoteEditor>> {
        if self.active_pane.is_some() {
            return None;
        }
        self.active.and_then(|i| self.tabs.get(i))
    }

    pub fn last_note(&self, cx: &gpui::App) -> Option<NoteId> {
        self.active
            .and_then(|i| self.tabs.get(i))
            .map(|e| e.read(cx).id)
    }

    pub fn show_pane(&mut self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .split
            .as_ref()
            .is_some_and(|split| split.other.panes.contains(&pane))
        {
            let other = !self.focused_second();
            self.focus_group(other, window, cx);
        }
        let index = match self.panes.iter().position(|p| *p == pane) {
            Some(index) => index,
            None => {
                self.panes.push(pane);
                self.panes.len() - 1
            }
        };
        self.active_pane = Some(index);
        self.slash = None;
        window.focus(&self.focus);
        self.focus_file(pane, window, cx);
        cx.notify();
    }

    pub fn close_pane(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.panes.len() {
            return;
        }
        if let Some(closed) = self.closed_pane(index) {
            crate::tab_keys::remember(&mut self.closed, closed);
        }
        match self.panes.remove(index) {
            Pane::Base(id) => self.forget_base(id),
            Pane::File(id) => self.forget_file(id, cx),
            _ => {}
        }
        self.active_pane = match self.active_pane {
            Some(active) if active == index => None,
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
        self.focus_content(window, cx);
        self.drop_empty_group(window, cx);
        cx.notify();
    }

    pub fn flush_all(&mut self, cx: &mut Context<Self>) {
        for tab in self.all_tabs() {
            if tab.read(cx).follower {
                continue;
            }
            if let Some(pending) = tab.update(cx, |editor, cx| editor.pending_write(cx))
                && let Err(error) = pending.write()
            {
                eprintln!("focus: writing a note at exit: {error}");
            }
        }
        self.refresh_store(cx);
        self.save_task = None;
        if self.store.writable()
            && let Err(error) = crate::session::write(&self.store.settings)
        {
            eprintln!("focus: writing the settings at exit: {error:#}");
        }
    }

    pub fn reload_notes(&mut self, notes: &[NoteId], window: &mut Window, cx: &mut Context<Self>) {
        for tab in self.all_tabs() {
            if !notes.contains(&tab.read(cx).id) {
                continue;
            }
            tab.update(cx, |editor, cx| {
                if let Some(task) = editor.disk_changed(window, cx) {
                    task.detach();
                }
            });
        }
    }

    pub fn show_search(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.layout.left_open = true;
        self.layout.left_tab = LeftTab::Search;
        let query = query.to_string();
        self.search
            .update(cx, |panel, cx| panel.set_query(&query, window, cx));
        cx.notify();
    }

    pub fn default_view(&self, cx: &gpui::App) -> ViewMode {
        match self.prefs.read(cx).choice("general.default_view") {
            1 => ViewMode::Live,
            2 => ViewMode::Reading,
            _ => ViewMode::Source,
        }
    }

    pub fn open_note(
        &mut self,
        id: NoteId,
        new_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(meta) = self.library.as_ref().and_then(|library| library.note(id)) {
            self.note_opened(&meta.path, cx);
        }
        if let Some(index) = self.tabs.iter().position(|tab| tab.read(cx).id == id) {
            self.activate(index, window, cx);
            self.push_history(id);
            return;
        }
        let Some(meta) = self.library.as_ref().and_then(|library| library.note(id)) else {
            if let Some(path) = crate::loose::path_for(id) {
                self.open_loose(path, window, cx);
            }
            return;
        };
        self.open_meta(meta, None, new_tab, window, cx);
    }

    pub fn open_meta(
        &mut self,
        meta: std::sync::Arc<focus_core::NoteMeta>,
        loose: Option<PathBuf>,
        new_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = meta.id;
        let Some(library) = self.library.clone() else {
            return;
        };
        let options = {
            let prefs = self.prefs.read(cx);
            crate::editor::EditorOptions {
                view: self.default_view(cx),
                line_numbers: prefs.bool("editor.line_numbers"),
                typesetting: self.typesetting(cx),
                autosave: std::time::Duration::from_millis(prefs.number("editor.autosave") as u64),
                wrap: prefs.bool("editor.wrap"),
                tab_width: prefs.number("editor.tab_width") as usize,
                font: crate::editing::EditorFont::from_choice(
                    prefs.choice("appearance.editor_font"),
                ),
                bounds: self.editor_bounds.clone(),
                edit_in_place: prefs.bool("editor.edit_in_place"),
            }
        };
        let figure_width = self.figure_width(cx);
        let handles = self.prefs.read(cx).bool("editor.block_handles");
        let editor = cx.new(|cx| {
            let mut editor = NoteEditor::new(meta, library.clone(), options, window, cx);
            editor.loose = loose;
            editor.set_figure_width(figure_width, cx);
            editor.set_block_handles(handles, cx);
            editor
        });
        let decoration = self
            .code_decoration
            .clone()
            .map(|d| d as std::sync::Arc<dyn focus_view_reading::CodeDecoration>);
        if decoration.is_some() {
            let variables = self.code_variables(cx);
            editor.update(cx, |editor, cx| {
                editor.set_code_decoration(decoration, cx);
                editor.set_code_variables(variables, cx);
            });
        }
        self._subscriptions.push(cx.subscribe_in(
            &editor,
            window,
            |this, editor, event: &EditorEvent, window, cx| match event {
                EditorEvent::Changed => {
                    if let Some((id, line)) = this.pending_line
                        && editor.read(cx).id == id
                        && matches!(editor.read(cx).load, crate::editor::Load::Ready)
                    {
                        this.pending_line = None;
                        editor.update(cx, |editor, cx| {
                            editor
                                .reading
                                .update(cx, |reading, _| reading.scroll_to_line(line));
                            if editor.view != ViewMode::Reading {
                                editor.go_to_line(line, window, cx);
                            }
                        });
                    }
                    this.sync_twin(editor, window, cx);
                    this.poll_history_error(cx);
                    cx.notify()
                }
                EditorEvent::OpenLink { goal, new_tab } => {
                    this.follow_link(goal.clone(), *new_tab, window, cx)
                }
                EditorEvent::LinkPage(page) => this.files.link_page = Some(*page),
                EditorEvent::Zoom(source) => {
                    this.overlay = Overlay::Zoom(source.clone());
                    cx.notify();
                }
                EditorEvent::Hover { goal, position } => {
                    let note = match goal {
                        Some(focus_view_reading::model::LinkGoal::Note { id, anchor }) => {
                            Some((*id, anchor.clone()))
                        }
                        _ => None,
                    };
                    this.show_link_preview(note, *position, cx);
                }
                EditorEvent::Failed(message) => {
                    this.notify_user(focus_kit::Tone::Danger, message.clone(), cx)
                }
                EditorEvent::Notice(message) => {
                    this.notify_user(focus_kit::Tone::Ok, message.clone(), cx)
                }
                EditorEvent::Edited => {
                    this.on_edited(editor, cx);
                    this.sync_twin(editor, window, cx);
                }
                EditorEvent::Crumb { folder, at } => {
                    this.open_crumb_menu_at(folder.clone(), at.x, at.y + px(12.), cx)
                }
                EditorEvent::Variable(menu) => this.on_variable_menu(menu, window, cx),
                EditorEvent::OpenAt {
                    note,
                    line,
                    new_tab,
                } => {
                    let (note, line) = (*note, *line);
                    this.open_note(note, *new_tab, window, cx);
                    if let Some(line) = line {
                        this.pending_line = Some((note, line));
                    }
                }
            },
        ));
        let replace = !new_tab && self.active_editor().is_some_and(|e| !e.read(cx).dirty);
        let index = match (replace, self.active) {
            (true, Some(active)) => {
                let replaced = self.tabs[active].clone();
                self.hand_lead(&replaced, cx);
                self.tabs[active] = editor.clone();
                active
            }
            (_, Some(active)) => {
                self.tabs.insert(active + 1, editor.clone());
                active + 1
            }
            (_, None) => {
                self.tabs.push(editor.clone());
                self.tabs.len() - 1
            }
        };
        let shared = self
            .in_other_group(id, cx)
            .and_then(|twin| twin.read(cx).shared(cx));
        if let Some(shared) = shared {
            editor.update(cx, |editor, cx| {
                editor.follower = true;
                editor.follow(&shared, window, cx);
            });
            self.activate(index, window, cx);
            self.push_history(id);
            return;
        }
        if self.in_other_group(id, cx).is_some() {
            editor.update(cx, |editor, _| editor.follower = true);
        }
        self.pending += 1;
        let load = editor.update(cx, |editor, cx| editor.load(window, cx));
        cx.spawn(async move |this, cx| {
            load.await;
            let _ = this.update(cx, |this, cx| {
                this.pending -= 1;
                cx.notify();
            });
        })
        .detach();
        self.activate(index, window, cx);
        self.push_history(id);
    }

    fn push_history(&mut self, id: NoteId) {
        if self.history.get(self.history_pos.wrapping_sub(1)) == Some(&id) {
            return;
        }
        self.history.truncate(self.history_pos);
        self.history.push(id);
        self.history_pos = self.history.len();
    }

    pub fn can_go_back(&self) -> bool {
        self.history_pos > 1
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_pos < self.history.len()
    }

    fn go_history(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.history_pos as isize + delta;
        if target < 1 || target as usize > self.history.len() {
            return;
        }
        self.history_pos = target as usize;
        let id = self.history[self.history_pos - 1];
        let saved = self.history_pos;
        self.open_note(id, false, window, cx);
        self.history_pos = saved;
    }

    pub fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.tabs.get(index).cloned() else {
            return;
        };
        self.active = Some(index);
        self.active_pane = None;
        self.preview = None;
        for (i, tab) in self.tabs.clone().into_iter().enumerate() {
            if i != index {
                tab.update(cx, |editor, cx| editor.park(window, cx));
            }
        }
        if let Some(load) = editor.update(cx, |editor, cx| editor.unpark(window, cx)) {
            self.pending += 1;
            cx.spawn(async move |this, cx| {
                load.await;
                let _ = this.update(cx, |this, cx| {
                    this.pending -= 1;
                    cx.notify();
                });
            })
            .detach();
        }
        let path = editor.read(cx).meta.path.clone();
        self.reveal_in_tree(Some(path), cx);
        self.follow_local_graph(cx);
        *self.ai_note.borrow_mut() = Some(editor.clone());
        if let Some(panel) = &self.ai {
            panel.update(cx, |panel, cx| panel.refresh_context(cx));
        }
        if matches!(self.overlay, Overlay::None) {
            self.focus_content(window, cx);
        }
        cx.notify();
    }

    pub fn focus_content(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.active_editor().cloned() {
            Some(editor)
                if editor.read(cx).view == ViewMode::Source || editor.read(cx).live.is_some() =>
            {
                let input = editor.read(cx).typing_input();
                input.update(cx, |input, cx| input.focus(window, cx));
            }
            Some(editor) => {
                let handle = gpui::Focusable::focus_handle(editor.read(cx).reading.read(cx), cx);
                window.focus(&handle);
            }
            _ => window.focus(&self.focus),
        }
    }

    pub fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        if let Some(closed) = self.closed_tab(index, cx) {
            crate::tab_keys::remember(&mut self.closed, closed);
        }
        let closing = self.tabs[index].clone();
        let twin = self.twin_of(&closing, cx);
        self.hand_lead(&closing, cx);
        let pending = match twin {
            Some(_) => None,
            None => closing.update(cx, |editor, cx| editor.pending_write(cx)),
        };
        if let Some(pending) = pending {
            let task = cx.background_spawn(async move { pending.write() });
            cx.spawn(async move |this, cx| {
                if let Err(error) = task.await {
                    let _ = this.update(cx, |this, cx| {
                        this.notify_user(
                            focus_kit::Tone::Danger,
                            focus_i18n::t_args(
                                "reading-save-failed",
                                &[("error", crate::errors::vault(&error).as_str().into())],
                            ),
                            cx,
                        );
                    });
                }
            })
            .detach();
        }
        self.tabs.remove(index);
        self.active = match self.active {
            _ if self.tabs.is_empty() => None,
            Some(active) if active > index => Some(active - 1),
            Some(active) => Some(active.min(self.tabs.len() - 1)),
            None => None,
        };
        match self.active {
            Some(active) => self.activate(active, window, cx),
            None => {
                self.reveal_in_tree(None, cx);
                window.focus(&self.focus);
            }
        }
        self.drop_empty_group(window, cx);
        cx.notify();
    }

    pub fn figure_width(&self, cx: &gpui::App) -> f32 {
        self.prefs.read(cx).number("appearance.measure") as f32 + 2. * f32::from(size::SHEET_PAD)
    }

    pub fn set_view(&mut self, view: ViewMode, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor().cloned() {
            editor.update(cx, |editor, cx| editor.set_view(view, window, cx));
        }
        if matches!(self.overlay, Overlay::None) {
            self.focus_content(window, cx);
        }
        cx.notify();
    }

    pub fn open_palette(&mut self, initial: &str, window: &mut Window, cx: &mut Context<Self>) {
        let note = self.active_editor().map(|e| e.read(cx).id);
        let recent: Vec<NoteId> = self.history.iter().rev().copied().collect();
        let library = self.library.clone();
        let ops = self.prefs.read(cx).bool("ops.enabled");
        let ai = self.prefs.read(cx).bool("ai.enabled");
        let palette =
            cx.new(|cx| Palette::new(library, note, recent, ops, ai, initial, window, cx));
        if let Some(pages) = self.front_pdf().map(|pdf| pdf.read(cx).page_count()) {
            palette.update(cx, |palette, cx| palette.set_pages(pages, cx));
        }
        let subscription = cx.subscribe_in(
            &palette,
            window,
            |this, _, event: &PaletteEvent, window, cx| match event {
                PaletteEvent::OpenNote { id, new_tab } => {
                    let (id, new_tab) = (*id, *new_tab);
                    this.close_overlay(window, cx);
                    this.open_note(id, new_tab, window, cx);
                }
                PaletteEvent::Run(action) => {
                    let action = action.boxed_clone();
                    this.close_overlay(window, cx);
                    window.dispatch_action(action, cx);
                }
                PaletteEvent::OpenAtLine { id, line } => {
                    let (id, line) = (*id, *line);
                    this.close_overlay(window, cx);
                    this.open_note(id, false, window, cx);
                    this.pending_line = Some((id, line));
                }
                PaletteEvent::SearchTag(tag) => {
                    let query = format!("tag:#{tag}");
                    this.close_overlay(window, cx);
                    this.layout.left_open = true;
                    this.layout.left_tab = LeftTab::Search;
                    this.search
                        .update(cx, |panel, cx| panel.set_query(&query, window, cx));
                }
                PaletteEvent::FromTemplate(template) => {
                    let template = *template;
                    this.close_overlay(window, cx);
                    this.note_from_template(template, window, cx);
                }
                PaletteEvent::FromHostTemplate(index) => {
                    let index = *index;
                    this.close_overlay(window, cx);
                    this.note_from_host_template(index, window, cx);
                }
                PaletteEvent::GoToLine(line) => {
                    let line = *line;
                    this.close_overlay(window, cx);
                    if let Some(editor) = this.active_editor().cloned() {
                        editor.update(cx, |editor, cx| editor.go_to_line(line, window, cx));
                    }
                }
                PaletteEvent::OpenFile(path) => {
                    let absolute = this.library.as_ref().map(|l| l.absolute(path));
                    this.close_overlay(window, cx);
                    if let Some(absolute) = absolute {
                        this.open_media(&absolute, None, window, cx);
                    }
                }
                PaletteEvent::GoToPage(page) => {
                    let page = *page;
                    this.close_overlay(window, cx);
                    if let Some(pdf) = this.front_pdf() {
                        pdf.update(cx, |pdf, cx| pdf.go_to(page, None, cx));
                    }
                }
                PaletteEvent::Close => this.close_overlay(window, cx),
            },
        );
        let input = palette.read(cx).input.clone();
        input.update(cx, |input, cx| input.focus(window, cx));
        self.overlay = Overlay::Palette {
            view: palette,
            _events: subscription,
        };
        cx.notify();
    }

    pub fn open_settings(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.facts.rss = crate::chrome::resident_memory();
        let mut facts = self.facts.clone();
        facts.recent = self.store.settings.recent_vaults.clone();
        facts.index_ms = self
            .library
            .as_ref()
            .map(|library| library.index().stats().build_ms);
        facts.history = self
            .library
            .as_ref()
            .and_then(|library| library.vault().history());
        facts.engagement = self.ops.as_ref().and_then(|ops| {
            ops.state
                .read(cx)
                .active
                .as_ref()
                .map(|active| active.engagement.name.clone())
        });
        let prefs = self.prefs.clone();
        let view = cx.new(|cx| SettingsView::new(prefs, section, facts, window, cx));
        let subscription = cx.subscribe_in(
            &view,
            window,
            |this, _, event: &SettingsEvent, window, cx| match event {
                SettingsEvent::Close => this.close_overlay(window, cx),
                SettingsEvent::OpenVault => this.prompt_vault(window, cx),
                SettingsEvent::SwitchVault(path) => {
                    let path = path.clone();
                    this.close_overlay(window, cx);
                    this.switch_vault(path, window, cx);
                }
                SettingsEvent::Action("transfer.import") => {
                    this.open_transfer(crate::transfer::Transfer::Import, window, cx)
                }
                SettingsEvent::Action("transfer.export_vault") => this.open_transfer(
                    crate::transfer::Transfer::Export { vault: true },
                    window,
                    cx,
                ),
                SettingsEvent::Action("search.rebuild") => {
                    this.close_overlay(window, cx);
                    this.rebuild_index(window, cx);
                }
                SettingsEvent::Action(crate::update::CHECK_NOW) => {
                    this.check_updates(true, cx).detach()
                }
                SettingsEvent::Action(_) => {}
            },
        );
        let search = view.read(cx).search_input().clone();
        search.update(cx, |input, cx| input.focus(window, cx));
        self.overlay = Overlay::Settings {
            view,
            _events: subscription,
        };
        cx.notify();
    }

    pub fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::None;
        self.give_focus_back(window, cx);
        cx.notify();
    }

    pub fn prompt_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let chosen = match paths.await {
                Ok(Ok(Some(mut paths))) => paths.pop(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    eprintln!("focus: folder picker: {error:#}");
                    None
                }
                Err(_) => None,
            };
            if let Some(root) = chosen {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.close_overlay(window, cx);
                    this.switch_vault(root, window, cx);
                });
            }
        })
        .detach();
    }

    fn apply_pref(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let prefs = self.prefs.read(cx);
        match key {
            "appearance.theme" => {
                let mode = match prefs.choice(key) {
                    1 => Mode::Light,
                    2 => match window.appearance() {
                        gpui::WindowAppearance::Light | gpui::WindowAppearance::VibrantLight => {
                            Mode::Light
                        }
                        _ => Mode::Dark,
                    },
                    _ => Mode::Dark,
                };
                if let Err(error) = focus_kit::theme::apply(mode, Some(window), cx) {
                    eprintln!("focus: theme: {error:#}");
                }
            }
            "ops.enabled" | "ops.folder" => {
                if !prefs.bool("ops.enabled") && self.layout.right_tab == RightTab::Ops {
                    self.layout.right_tab = RightTab::Outline;
                }
                if key == "ops.folder" {
                    self.ops = None;
                    self.guard_ai_engagements(window, cx);
                }
                self.sync_ops(window, cx);
            }
            "appearance.remote_images" => focus_view_reading::set_remote_images(
                focus_view_reading::RemoteImages::from_index(prefs.choice(key)),
                cx,
            ),
            "editor.wrap" => {
                let wrap = prefs.bool(key);
                for tab in self.all_tabs() {
                    tab.update(cx, |editor, cx| {
                        editor
                            .input
                            .update(cx, |input, cx| input.set_soft_wrap(wrap, window, cx))
                    });
                }
            }
            crate::update::OFFERED => self.watch_updates(std::time::Duration::ZERO, cx),
            "ai.key" => {
                if let Some(Value::Text(key)) = prefs.get(key).cloned() {
                    self.store_ai_key(key.to_string(), window, cx);
                }
            }
            key if key.starts_with("graph.") => self.graph_prefs_changed(cx),
            "editor.line_numbers" => {
                let on = prefs.bool(key);
                for tab in self.all_tabs() {
                    tab.update(cx, |editor, cx| editor.set_line_numbers(on, window, cx));
                }
            }
            "editor.edit_in_place" => {
                let on = prefs.bool(key);
                for tab in self.all_tabs() {
                    tab.update(cx, |editor, cx| editor.set_edit_in_place(on, window, cx));
                }
            }
            "editor.block_handles" => {
                let on = prefs.bool(key);
                for tab in self.all_tabs() {
                    tab.update(cx, |editor, cx| editor.set_block_handles(on, cx));
                }
            }
            "files.show_other" => {
                self.vault_view_settings_changed(cx);
                self.graph_prefs_changed(cx);
            }
            "files.update_links" | "general.confirm_delete" | "general.new_note_folder" => {
                self.vault_view_settings_changed(cx)
            }
            "files.trash" => {
                self.vault_view_settings_changed(cx);
                self.apply_engagement_trash(cx);
            }
            "files.hide_templates" | "general.templates" => self.apply_excluded_folders(cx),
            "files.follow_external_links" => self.rebuild_index(window, cx),
            "ai.agent_socket" => self.sync_agent_socket(window, cx),
            "appearance.ui_scale" => {
                let zoom = prefs.number(key) as f32 / 100.;
                window.set_zoom(zoom, cx);
            }
            "appearance.reading_size"
            | "appearance.measure"
            | "appearance.reading_leading"
            | "appearance.code_colors"
            | "appearance.reading_font"
            | "appearance.code_font" => {
                let typesetting = self.typesetting(cx);
                let figure_width = self.figure_width(cx);
                for tab in self.all_tabs() {
                    let reading = tab.read(cx).reading.clone();
                    reading.update(cx, |reading, cx| {
                        reading.set_typesetting(typesetting.clone(), cx)
                    });
                    tab.update(cx, |editor, cx| editor.set_figure_width(figure_width, cx));
                }
            }
            "appearance.editor_font" => {
                let font = crate::editing::EditorFont::from_choice(prefs.choice(key));
                for tab in self.all_tabs() {
                    tab.update(cx, |editor, _| editor.block_font = font);
                }
            }
            "shortcuts.overrides" => crate::actions::set_overrides(&prefs.text(key)),
            "general.language" => {
                let code = crate::settings::schema::LANGUAGE_CODES
                    .get(prefs.choice(key))
                    .copied()
                    .flatten()
                    .unwrap_or("system");
                self.apply_language(code, window, cx);
            }
            _ => {}
        }
        if !matches!(self.screen, Screen::Welcome { .. }) {
            self.schedule_save(cx);
        }
        if key == "appearance.reduce_motion" {
            let reduce = self.prefs.read(cx).bool(key);
            focus_kit::fade::set_animate(!self.scripted && !reduce, cx);
        }
        if key == "appearance.density" {
            focus_kit::size::set_compact(self.prefs.read(cx).choice(key) == 1);
            window.refresh();
        }
        if key == "appearance.reduce_motion"
            && let Some(graph) = self.graph.clone()
        {
            let reduce = self.prefs.read(cx).bool(key);
            graph.update(cx, |graph, cx| graph.set_reduce_motion(reduce, cx));
        }
        if key.starts_with("ai.") || key == "ops.enabled" || key == "appearance.reduce_motion" {
            self.ai_settings_changed(cx);
        }
        if matches!(key, "search.accents" | "search.limit") {
            self.apply_search_settings(window, cx);
        }
        if key.starts_with("ops.") {
            self.apply_ops_settings(cx);
        }
        if matches!(key, "search.ai" | "ai.redact" | "ai.redact_ips") {
            self.refresh_search_services(window, cx);
        }
        self.apply_history_settings(cx);
        if key == "ai.enabled" {
            if !prefs_ai_enabled(&self.prefs, cx) && self.layout.right_tab == RightTab::Ai {
                self.layout.right_tab = RightTab::Outline;
            }
            self.connect_ai(window, cx);
        }
        if matches!(key, "ai.base_url" | "ai.model" | "ai.provider")
            || (key.starts_with("ai.") && (key.ends_with(".base_url") || key.ends_with(".model")))
        {
            self.connect_ai(window, cx);
        }
        cx.notify();
    }

    pub fn set_pref(&mut self, key: &'static str, value: Value, cx: &mut Context<Self>) {
        self.prefs.update(cx, |prefs, cx| prefs.set(key, value, cx));
    }

    pub fn forced(&self, name: &str) -> Forced {
        self.forced.get(name).copied().unwrap_or_default()
    }

    fn on_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = &self.drag else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            cx.notify();
            return;
        }
        let viewport = window.viewport_size();
        match drag.edge {
            Edge::Left => {
                let width = drag.start
                    + focus_kit::Dir::current().toward_end(event.position.x - drag.origin);
                self.layout.left_w =
                    width.clamp(size::LEFT_MIN, size::LEFT_MAX.min(viewport.width * 0.5));
            }
            Edge::Right => {
                let width = drag.start
                    - focus_kit::Dir::current().toward_end(event.position.x - drag.origin);
                self.layout.right_w =
                    width.clamp(size::RIGHT_MIN, size::RIGHT_MAX.min(viewport.width * 0.5));
            }
            Edge::Split => self.drag_divider(event.position.x, window),
            Edge::Bottom => {
                let height = drag.start - (event.position.y - drag.origin);
                let max =
                    viewport.height - size::TITLE_BAR - size::STATUS_BAR - size::TAB_BAR - px(120.);
                self.layout.bottom_h = height.clamp(size::BOTTOM_MIN, max.max(size::BOTTOM_MIN));
            }
        }
        cx.notify();
    }

    pub fn start_drag(&mut self, edge: Edge, origin: Pixels, cx: &mut Context<Self>) {
        let start = match edge {
            Edge::Left => self.layout.left_w,
            Edge::Right => self.layout.right_w,
            Edge::Bottom => self.layout.bottom_h,
            Edge::Split => origin,
        };
        self.drag = Some(Drag {
            edge,
            origin,
            start,
        });
        cx.notify();
    }

    fn bind_actions(
        &self,
        root: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let root = self.bind_media_actions(root, cx);
        self.bind_window_keys(root, cx)
            .on_action(
                cx.listener(|this, _: &CheckUpdates, _, cx| this.check_updates(true, cx).detach()),
            )
            .on_action(cx.listener(|this, _: &ToggleLeft, _, cx| {
                this.layout.left_open = !this.layout.left_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleRight, _, cx| {
                this.layout.right_open = !this.layout.right_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleBottom, _, cx| {
                this.layout.bottom_open = !this.layout.bottom_open;
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, _: &OpenPalette, window, cx| this.open_palette("", window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenCommands, window, cx| {
                    this.open_palette(">", window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.open_settings(Section::General, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &CloseTab, window, cx| this.close_front_tab(window, cx)),
            )
            .on_action(cx.listener(|this, _: &NextTab, window, cx| this.step_tab(1, window, cx)))
            .on_action(cx.listener(|this, _: &PrevTab, window, cx| this.step_tab(-1, window, cx)))
            .on_action(
                cx.listener(|this, _: &SplitRight, window, cx| this.split_right(None, window, cx)),
            )
            .on_action(cx.listener(|this, _: &FocusOtherGroup, window, cx| {
                let other = !this.focused_second();
                this.focus_group(other, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Back, window, cx| this.go_history(-1, window, cx)))
            .on_action(cx.listener(|this, _: &Forward, window, cx| this.go_history(1, window, cx)))
            .on_action(cx.listener(|this, _: &ToggleReading, window, cx| {
                let next = match this.active_editor().map(|e| e.read(cx)) {
                    Some(editor) if editor.view == ViewMode::Reading => editor.editing_view,
                    Some(_) => ViewMode::Reading,
                    None => return,
                };
                this.set_view(next, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ViewSource, window, cx| {
                this.set_view(ViewMode::Source, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ViewLive, window, cx| {
                this.set_view(ViewMode::Live, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ViewReading, window, cx| {
                this.set_view(ViewMode::Reading, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SearchVault, window, cx| {
                this.layout.left_open = true;
                this.layout.left_tab = LeftTab::Search;
                this.search
                    .update(cx, |panel, cx| panel.focus_field(window, cx));
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, _: &OpenGraph, window, cx| this.open_graph(None, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenVaultGraph, window, cx| {
                    this.open_vault_graph(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &AskAi, window, cx| {
                if this.prefs.read(cx).bool("ai.enabled") {
                    this.show_ai(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpsDashboard, window, cx| {
                this.show_ops(crate::ops::OpsScreen::Dashboard, window, cx)
            }))
            .on_action(cx.listener(|this, _: &OpsHosts, window, cx| {
                this.show_ops(crate::ops::OpsScreen::Hosts, window, cx)
            }))
            .on_action(cx.listener(|this, _: &OpsFindings, window, cx| {
                this.show_ops(crate::ops::OpsScreen::Findings, window, cx)
            }))
            .on_action(cx.listener(|this, _: &OpsCredentials, window, cx| {
                this.show_ops(crate::ops::OpsScreen::Credentials, window, cx)
            }))
            .on_action(cx.listener(|this, _: &OpsCommands, window, cx| {
                this.show_ops(crate::ops::OpsScreen::Commands, window, cx)
            }))
            .on_action(cx.listener(|this, _: &OpsReport, window, cx| {
                this.show_ops(crate::ops::OpsScreen::Report, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &OpsNewEngagement, window, cx| {
                    this.new_engagement(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &ExportNote, window, cx| {
                this.open_transfer(
                    crate::transfer::Transfer::Export { vault: false },
                    window,
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &ExportVault, window, cx| {
                this.open_transfer(
                    crate::transfer::Transfer::Export { vault: true },
                    window,
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &ImportNotes, window, cx| {
                this.open_transfer(crate::transfer::Transfer::Import, window, cx)
            }))
            .on_action(cx.listener(|this, _: &TidyAttachments, window, cx| {
                this.open_transfer(crate::transfer::Transfer::Tidy, window, cx)
            }))
            .on_action(cx.listener(|this, _: &OrphanAttachments, window, cx| {
                this.open_transfer(crate::transfer::Transfer::Orphans, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowVersions, window, cx| {
                this.show_active_versions(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &MergeNote, window, cx| this.open_merge(None, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ExtractSelection, window, cx| this.open_extract(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &MoveBlockUp, window, cx| this.move_block(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &MoveBlockDown, window, cx| {
                    this.move_block(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &OpenBlockSource, window, cx| {
                if let Some(editor) = this.active_editor().cloned() {
                    editor.update(cx, |editor, cx| editor.open_caret_source(window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &CopyHtml, _, cx| this.copy_note(true, cx)))
            .on_action(
                cx.listener(|this, _: &DailyNote, window, cx| this.open_daily_note(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &QuickCapture, window, cx| this.open_capture(window, cx)),
            )
            .on_action(cx.listener(|this, _: &JournalEntry, window, cx| {
                this.quick_journal_entry(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &NewFromTemplate, window, cx| {
                    this.choose_template(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &CopyMarkdown, _, cx| this.copy_note(false, cx)))
            .on_action(cx.listener(|this, _: &ShowFiles, window, cx| this.show_files(window, cx)))
            .on_action(
                cx.listener(|this, _: &Confirm, window, cx| match this.overlay {
                    Overlay::SwitchVault(_) => this.confirm_vault_switch(window, cx),
                    Overlay::OpenAddress(_) => this.confirm_address(window, cx),
                    Overlay::ObsidianWrite(_) => this.confirm_obsidian_write(window, cx),
                    _ => cx.propagate(),
                }),
            )
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                if !matches!(this.overlay, Overlay::None) {
                    this.close_overlay(window, cx);
                } else if matches!(this.screen, Screen::Kit(_)) {
                    this.screen = Screen::Workspace;
                    cx.notify();
                } else if this.layout.bottom_open
                    && this.region_now(window, cx) == crate::window_keys::Region::Bottom
                {
                    this.layout.bottom_open = false;
                    this.focus_content(window, cx);
                    cx.notify();
                }
            }))
    }
}

impl Shell {
    fn render_zoom(
        &mut self,
        source: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use focus_view_reading::media::{Slot, media};
        let t = *cx.tokens();
        let scale = window.scale_factor() * 2.;
        let slot = media(cx).update(cx, |media, cx| media.mermaid(&source, &t, scale, cx));
        let viewport = window.viewport_size();
        let body: gpui::AnyElement = match slot {
            Slot::Ready {
                image,
                width,
                height,
                ..
            } => {
                let fit = ((viewport.width * 0.9) / px(width))
                    .min((viewport.height * 0.9 - px(56.)) / px(height));
                gpui::img(image)
                    .w(px(width) * fit)
                    .h(px(height) * fit)
                    .into_any_element()
            }
            Slot::Loading => div()
                .ui()
                .text_color(t.ink_3)
                .child(focus_i18n::t("reading-diagram-loading"))
                .into_any_element(),
            Slot::Failed(message) => div()
                .ui()
                .text_color(t.danger)
                .child(message)
                .into_any_element(),
        };
        focus_kit::fade_in(
            "fade-zoom",
            div()
                .id("zoom-veil")
                .absolute()
                .inset_0()
                .flow_row()
                .items_center()
                .justify_center()
                .bg(t.veil)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
                )
                .child(
                    focus_kit::floating(&t)
                        .flex()
                        .flex_col()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flow_row()
                                .items_center()
                                .h(px(28.))
                                .px(size::S3)
                                .border_b_1()
                                .border_color(t.line)
                                .ui_sm()
                                .text_color(t.ink_3)
                                .child(focus_i18n::t("reading-diagram"))
                                .child(div().flex_1())
                                .child(focus_kit::Kbd::new(focus_i18n::keys_text("escape"))),
                        )
                        .child(div().p(size::S6).bg(t.bg).child(body)),
                ),
            cx,
        )
    }
}

impl Shell {
    pub fn show_link_preview(
        &mut self,
        note: Option<(NoteId, Option<String>)>,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.preview = None;
        if let (Some((id, anchor)), Some(library)) = (note, self.library.clone()) {
            let decoration = self
                .code_decoration
                .clone()
                .map(|d| d as std::sync::Arc<dyn focus_view_reading::CodeDecoration>);
            let preview = self
                .link_preview
                .get_or_insert_with(|| {
                    cx.new(|cx| {
                        let mut preview = focus_view_reading::LinkPreview::new(cx);
                        preview.set_code_decoration(decoration, cx);
                        preview
                    })
                })
                .clone();
            let resolver: std::sync::Arc<dyn focus_view_reading::Resolver> =
                match self.active_editor() {
                    Some(editor) => editor.read(cx).resolver(),
                    None => library.vault().clone(),
                };
            preview.update(cx, |preview, cx| preview.show(id, anchor, resolver, cx));
            self.preview = Some((id, position));
        }
        cx.notify();
    }

    fn render_preview(&self, _: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let (_, position) = self.preview?;
        let preview = self.link_preview.clone()?;
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(position + gpui::point(px(0.), px(18.)))
                    .snap_to_window()
                    .child(preview),
            )
            .into_any_element(),
        )
    }

    fn render_slash(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let menu = self.slash.as_mut()?;
        let anchor = menu.anchor;
        let drawn = menu
            .editor
            .update(cx, |editor, cx| editor.anchor_bounds(anchor, window, cx));
        menu.drawn_at = drawn.or(menu.drawn_at);
        let at = menu.drawn_at?;
        let t = *cx.tokens();
        let weak = cx.entity().downgrade();
        let menu = self.slash.as_ref()?;
        Some(crate::slash::render(
            &t,
            &menu.entries,
            menu.kind == crate::editing::MenuKind::Commands,
            &menu.hits,
            menu.selected,
            &menu.query,
            at,
            window.viewport_size(),
            &menu.scroll,
            move |index, window, cx| {
                let _ = weak.update(cx, |shell, cx| shell.pick_slash(index, window, cx));
            },
            {
                let weak = cx.entity().downgrade();
                move |_, cx| {
                    let _ = weak.update(cx, |shell, cx| {
                        shell.slash = None;
                        cx.notify();
                    });
                }
            },
            cx,
        ))
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.remember_focus_giver(window, cx);
        if !self.first_frame_seen {
            self.first_frame_seen = true;
            let this = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                let ms = profile::mark("first frame");
                let _ = this.update(cx, |this, _| this.facts.first_frame_ms = Some(ms));
            });
        }
        let t = *cx.tokens();
        if let Some(editor) = self.active_editor().cloned()
            && let Some(phases) = editor.update(cx, |editor, _| editor.open_phases.take())
        {
            let (opened, path, size) = {
                let editor = editor.read(cx);
                (
                    editor.opened(),
                    editor.meta.path.display().to_string(),
                    editor.meta.size,
                )
            };
            window.on_next_frame(move |_, _| {
                crate::profile::mark(&format!(
                    "note shown: {path} ({size} bytes): read {:.0} ms, editor filled {:.0} ms, model built {:.0} ms, first frame {:.0} ms",
                    phases[0],
                    phases[1],
                    phases[2],
                    opened.elapsed().as_secs_f32() * 1000.
                ));
            });
        }
        self.close_unfocused_block(window, cx);
        let front = self.active_editor().cloned();
        if *self.ai_note.borrow() != front {
            *self.ai_note.borrow_mut() = front;
        }
        if self.library.is_some() && self.session_now(cx) != self.saved_session {
            self.schedule_save(cx);
        }
        let body = match &self.screen {
            Screen::Kit(gallery) => div()
                .flex_1()
                .min_h_0()
                .child(gallery.clone())
                .into_any_element(),
            Screen::Workspace => self.render_workspace(window, cx).into_any_element(),
            Screen::Welcome { selected } => {
                let selected = *selected;
                self.render_welcome(selected, cx)
            }
            Screen::Updates { offered } => {
                let offered = *offered;
                self.render_updates(offered, cx)
            }
            Screen::Vaults => self.render_vaults(cx),
        };
        let overlay = match &self.overlay {
            Overlay::None => None,
            Overlay::Zoom(source) => Some(self.render_zoom(source.clone(), window, cx)),
            Overlay::OpenAddress(target) => Some(self.render_open_address(&target.clone(), cx)),
            Overlay::Crumbs(menu) => Some(self.render_crumb_menu(&menu.clone(), cx)),
            Overlay::SwitchVault(switch) => Some(self.render_vault_switch(&switch.clone(), cx)),
            Overlay::TabMenu(menu) => Some(self.render_tab_menu(*menu, cx)),
            Overlay::ObsidianWrite(confirmation) => {
                Some(self.render_obsidian_write(&confirmation.clone(), cx))
            }
            Overlay::Capture(dialog) => Some(self.render_capture(dialog.input.clone(), cx)),
            Overlay::Palette { view, .. } => Some(
                div()
                    .absolute()
                    .inset_0()
                    .child(view.clone())
                    .into_any_element(),
            ),
            Overlay::Transfer { view, .. } => Some(
                div()
                    .absolute()
                    .inset_0()
                    .child(view.clone())
                    .into_any_element(),
            ),
            Overlay::Versions { view, .. } => Some(
                div()
                    .absolute()
                    .inset_0()
                    .child(view.clone())
                    .into_any_element(),
            ),
            Overlay::OpsDialog(dialog) => Some(focus_kit::fade_in(
                "fade-ops-dialog",
                div()
                    .id("ops-dialog-veil")
                    .absolute()
                    .inset_0()
                    .flow_row()
                    .items_center()
                    .justify_center()
                    .bg(t.veil)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
                    )
                    .child(
                        div()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(dialog.clone()),
                    ),
                cx,
            )),
            Overlay::Settings { view, .. } => Some(focus_kit::fade_in(
                "fade-settings",
                div()
                    .id("settings-veil")
                    .absolute()
                    .inset_0()
                    .flow_row()
                    .items_center()
                    .justify_center()
                    .bg(t.veil)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
                    )
                    .child(
                        div()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(view.clone()),
                    ),
                cx,
            )),
        };
        let overlay = overlay.map(|overlay| self.scope_overlay(overlay));
        let root = div()
            .id("focus")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.ink)
            .font_family(SANS)
            .text_start()
            .on_mouse_move(cx.listener(Self::on_drag_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.drag.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .child(self.render_titlebar(window, cx))
            .children(self.render_update(cx))
            .child(body)
            .child(self.render_statusbar(cx))
            .children(overlay)
            .children(self.render_preview(cx))
            .children(self.render_slash(window, cx))
            .children(self.render_notices(cx));
        let root = self.bind_actions(root, cx);
        root.when(self.drag.is_some(), |root| {
            root.cursor(
                if self.drag.as_ref().is_some_and(|d| d.edge == Edge::Bottom) {
                    gpui::CursorStyle::ResizeUpDown
                } else {
                    gpui::CursorStyle::ResizeLeftRight
                },
            )
        })
    }
}

fn prefs_ai_enabled(prefs: &Entity<Prefs>, cx: &gpui::App) -> bool {
    prefs.read(cx).bool("ai.enabled")
}
