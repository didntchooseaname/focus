use std::path::{Path, PathBuf};

use focus_view_ops::{
    CommandLibrary, CredentialsTable, FindingsList, HostsTable, JournalPanel, NewEngagementDialog,
    OpsDashboard, OpsEvent, OpsIndicator, OpsSidebar, OpsState, ReportView,
};
use gpui::{AppContext, Context, Entity, EventEmitter, Window};

use crate::shell::{Overlay, Pane, Shell};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpsScreen {
    Dashboard,
    Hosts,
    Findings,
    Credentials,
    Commands,
    Report,
}

impl OpsScreen {
    pub fn label(self) -> String {
        focus_i18n::t(match self {
            OpsScreen::Dashboard => "ops-section-overview",
            OpsScreen::Hosts => "ops-section-hosts",
            OpsScreen::Findings => "ops-section-findings",
            OpsScreen::Credentials => "ops-section-credentials",
            OpsScreen::Commands => "ops-section-commands",
            OpsScreen::Report => "ops-section-report",
        })
    }
}

pub struct OpsViews {
    pub state: Entity<OpsState>,
    pub dashboard: Entity<OpsDashboard>,
    pub hosts: Entity<HostsTable>,
    pub findings: Entity<FindingsList>,
    pub credentials: Entity<CredentialsTable>,
    pub commands: Entity<CommandLibrary>,
    pub report: Entity<ReportView>,
    pub sidebar: Entity<OpsSidebar>,
    pub journal: Entity<JournalPanel>,
    pub indicator: Entity<OpsIndicator>,
}

impl OpsViews {
    pub fn screen(&self, screen: OpsScreen) -> gpui::AnyElement {
        use gpui::IntoElement;
        match screen {
            OpsScreen::Dashboard => self.dashboard.clone().into_any_element(),
            OpsScreen::Hosts => self.hosts.clone().into_any_element(),
            OpsScreen::Findings => self.findings.clone().into_any_element(),
            OpsScreen::Credentials => self.credentials.clone().into_any_element(),
            OpsScreen::Commands => self.commands.clone().into_any_element(),
            OpsScreen::Report => self.report.clone().into_any_element(),
        }
    }
}

const REMEMBERED: &str = "ops_engagement";

impl Shell {
    pub fn engagements_dir(&self, cx: &gpui::App) -> Option<PathBuf> {
        let folder = match self.prefs.read(cx).get("ops.folder") {
            Some(crate::settings::schema::Value::Text(folder)) if !folder.is_empty() => {
                folder.to_string()
            }
            _ => "Engagements".to_string(),
        };
        Some(self.vault_root.as_ref()?.join(folder))
    }

    pub fn sync_ops(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = self.prefs.read(cx).bool("ops.enabled");
        if !enabled {
            self.ops = None;
            self.code_decoration = None;
            self.spread_code_decoration(cx);
            self.spread_code_variables(cx);
            self.panes.retain(|p| !matches!(p, Pane::Ops(_)));
            self.active_pane = self.active_pane.filter(|i| *i < self.panes.len());
            cx.notify();
            return;
        }
        if self.ops.is_some() {
            return;
        }
        let (Some(library), Some(engagements)) = (self.library.clone(), self.engagements_dir(cx))
        else {
            return;
        };
        let remembered = self
            .store
            .settings
            .ui
            .get(REMEMBERED)
            .and_then(|v| v.as_str())
            .map(PathBuf::from);
        let state = cx.new(|cx| OpsState::new(engagements, remembered, cx));
        let views = OpsViews {
            dashboard: cx.new(|cx| OpsDashboard::new(state.clone(), cx)),
            hosts: cx.new(|cx| HostsTable::new(state.clone(), window, cx)),
            findings: cx.new(|cx| FindingsList::new(state.clone(), window, cx)),
            credentials: cx.new(|cx| CredentialsTable::new(state.clone(), window, cx)),
            commands: cx.new(|cx| {
                CommandLibrary::new(
                    library.vault().clone(),
                    state.clone(),
                    Vec::new(),
                    window,
                    cx,
                )
            }),
            report: cx.new(|cx| ReportView::new(state.clone(), window, cx)),
            sidebar: cx.new(|cx| OpsSidebar::new(state.clone(), window, cx)),
            journal: cx.new(|cx| JournalPanel::new(state.clone(), window, cx)),
            indicator: cx.new(|cx| OpsIndicator::new(state.clone(), cx)),
            state,
        };
        self.listen_ops(&views.dashboard, window, cx);
        self.listen_ops(&views.hosts, window, cx);
        self.listen_ops(&views.findings, window, cx);
        self.listen_ops(&views.credentials, window, cx);
        self.listen_ops(&views.commands, window, cx);
        self.listen_ops(&views.report, window, cx);
        self.listen_ops(&views.sidebar, window, cx);
        self._subscriptions.push(cx.subscribe_in(
            &views.sidebar,
            window,
            |this, _, focus_view_ops::ShowPage(page): &focus_view_ops::ShowPage, window, cx| {
                let screen = match page {
                    focus_view_ops::OpsPage::Overview => OpsScreen::Dashboard,
                    focus_view_ops::OpsPage::Hosts => OpsScreen::Hosts,
                    focus_view_ops::OpsPage::Findings => OpsScreen::Findings,
                    focus_view_ops::OpsPage::Credentials => OpsScreen::Credentials,
                    focus_view_ops::OpsPage::Commands => OpsScreen::Commands,
                    focus_view_ops::OpsPage::Report => OpsScreen::Report,
                };
                this.show_ops(screen, window, cx);
            },
        ));
        self.listen_ops(&views.journal, window, cx);
        self.listen_ops(&views.indicator, window, cx);
        self._subscriptions.push(cx.subscribe(
            &views.state,
            |this, state, event: &focus_view_ops::StateEvent, cx| {
                if matches!(event, focus_view_ops::StateEvent::Loaded)
                    && state.read(cx).active.is_none()
                {
                    let gone = this
                        .store
                        .settings
                        .ui
                        .get(REMEMBERED)
                        .and_then(|v| v.as_str())
                        .is_some_and(|dir| !Path::new(dir).exists());
                    if gone {
                        this.store.settings.ui.remove(REMEMBERED);
                        this.schedule_save(cx);
                    }
                }
            },
        ));
        self.ops = Some(views);
        self.apply_engagement_trash(cx);
        self.apply_ops_settings(cx);
        self.watch_ops_variables(cx);
        cx.notify();
    }

    fn listen_ops<V: EventEmitter<OpsEvent>>(
        &mut self,
        view: &Entity<V>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._subscriptions.push(cx.subscribe_in(
            view,
            window,
            |this, _, event: &OpsEvent, window, cx| this.on_ops_event(event, window, cx),
        ));
    }

    pub fn show_ops(&mut self, screen: OpsScreen, window: &mut Window, cx: &mut Context<Self>) {
        if self.ops.is_some() {
            self.show_pane(Pane::Ops(screen), window, cx);
        }
    }

    pub fn new_engagement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ops) = &self.ops else { return };
        let state = ops.state.clone();
        let vault = self.library.as_ref().map(|l| l.vault().clone());
        let dialog = cx.new(|cx| {
            let dialog = NewEngagementDialog::new(state, window, cx);
            match vault {
                Some(vault) => dialog.with_note_templates(vault),
                None => dialog,
            }
        });
        self.listen_ops(&dialog, window, cx);
        self.overlay = Overlay::OpsDialog(dialog);
        cx.notify();
    }

    fn vault_relative(&self, path: &Path) -> Option<PathBuf> {
        let root = self.vault_root.as_ref()?;
        path.strip_prefix(root)
            .ok()
            .map(Path::to_path_buf)
            .or_else(|| path.is_relative().then(|| path.to_path_buf()))
    }

    fn on_ops_event(&mut self, event: &OpsEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            OpsEvent::OpenNote { path, line } => {
                let id = self
                    .vault_relative(path)
                    .and_then(|p| self.library.as_ref()?.note_by_path(&p));
                match id {
                    Some(id) => {
                        self.open_note(id, false, window, cx);
                        if let Some(line) = line {
                            self.pending_line = Some((id, *line));
                        }
                    }
                    None => self.notify_user(
                        focus_kit::Tone::Warn,
                        focus_i18n::t_args(
                            "reading-link-unresolved",
                            &[("target", path.display().to_string().as_str().into())],
                        ),
                        cx,
                    ),
                }
            }
            OpsEvent::ExportJournalCsv { file_name, content } => {
                self.save_file_as(file_name, content.clone().into_bytes(), window, cx)
            }
            OpsEvent::ActiveChanged(dir) => {
                self.store.settings.ui.insert(
                    REMEMBERED.into(),
                    serde_json::Value::String(dir.to_string_lossy().into_owned()),
                );
                self.schedule_save(cx);
            }
            OpsEvent::EngagementCreated(dir) => {
                if let Some(ops) = &self.ops {
                    let dir = dir.clone();
                    ops.state.update(cx, |state, cx| state.set_active(dir, cx));
                }
                self.overlay = Overlay::None;
                self.guard_ai_engagements(window, cx);
                self.show_ops(OpsScreen::Dashboard, window, cx);
            }
            OpsEvent::AttachEvidence {
                directory,
                name_prefix,
            } => self.paste_evidence(directory, name_prefix, window, cx),
            OpsEvent::ExportReport { path, format } => self.export_report(path, *format, cx),
            OpsEvent::OpenLink { target } => {
                let id = self.library.as_ref().and_then(|library| {
                    let plain = target.trim_start_matches("[[").trim_end_matches("]]");
                    let path = plain.split(['|', '#']).next().unwrap_or(plain);
                    library
                        .note_by_path(Path::new(&format!("{path}.md")))
                        .or_else(|| library.note_by_path(Path::new(path)))
                        .or_else(|| {
                            library
                                .fuzzy(path, focus_search::FuzzyScope::Notes, &[], 1)
                                .first()
                                .and_then(|hit| hit.note)
                        })
                });
                match id {
                    Some(id) => self.open_note(id, false, window, cx),
                    None => self.notify_user(
                        focus_kit::Tone::Warn,
                        focus_i18n::t_args(
                            "reading-link-unresolved",
                            &[("target", target.as_str().into())],
                        ),
                        cx,
                    ),
                }
            }
            OpsEvent::SearchTag(tag) => {
                self.show_search(&format!("tag:#{}", tag.trim_start_matches('#')), window, cx)
            }
            OpsEvent::HoverLink { target, position } => {
                let note = target.as_deref().and_then(|target| {
                    match self
                        .library
                        .as_ref()?
                        .vault()
                        .resolve_link_from_root(target)
                    {
                        focus_core::LinkTarget::Note { id, anchor } => Some((id, anchor)),
                        _ => None,
                    }
                });
                self.show_link_preview(note, *position, cx);
            }
            OpsEvent::ZoomDiagram(source) => {
                self.overlay = Overlay::Zoom(source.clone());
                cx.notify();
            }
            OpsEvent::DashboardRequested => self.show_ops(OpsScreen::Dashboard, window, cx),
            OpsEvent::DialogClosed => self.close_overlay(window, cx),
            OpsEvent::NewEngagementRequested => self.new_engagement(window, cx),
            OpsEvent::Notify { tone, text } => {
                let tone = match tone {
                    focus_view_ops::NoticeTone::Neutral => focus_kit::Tone::Neutral,
                    focus_view_ops::NoticeTone::Ok => focus_kit::Tone::Ok,
                    focus_view_ops::NoticeTone::Warn => focus_kit::Tone::Warn,
                    focus_view_ops::NoticeTone::Danger => focus_kit::Tone::Danger,
                };
                self.notify_user(tone, text.clone(), cx);
            }
        }
    }

    fn paste_evidence(
        &mut self,
        directory: &Path,
        name_prefix: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(directory) = self.vault_relative(directory) else {
            return;
        };
        let name_prefix = name_prefix.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let image = focus_kit::clipboard::read_image(cx).await;
            let _ = this.update_in(cx, |this, window, cx| match image {
                Ok(Some(image)) => this.attach_bytes(
                    image.bytes,
                    image.extension.to_string(),
                    None,
                    Some(directory),
                    Some(name_prefix),
                    window,
                    cx,
                ),
                Ok(None) => this.notify_user(
                    focus_kit::Tone::Warn,
                    focus_i18n::t("ui-paste-no-image"),
                    cx,
                ),
                Err(error) => this.notify_user(
                    focus_kit::Tone::Danger,
                    crate::errors::clipboard(&error),
                    cx,
                ),
            });
        })
        .detach();
    }

    pub fn save_file_as(
        &mut self,
        file_name: &str,
        bytes: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = dirs_home();
        let chosen = cx.prompt_for_new_path(&directory, Some(file_name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            let result = crate::private::write_private(&path, &bytes);
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => this.notify_user(
                    focus_kit::Tone::Ok,
                    focus_i18n::t_args(
                        "ui-file-saved",
                        &[("path", path.display().to_string().as_str().into())],
                    ),
                    cx,
                ),
                Err(error) => this.notify_user(
                    focus_kit::Tone::Danger,
                    focus_i18n::t_args(
                        "errors-core-io",
                        &[
                            ("path", path.display().to_string().as_str().into()),
                            ("detail", error.to_string().as_str().into()),
                        ],
                    ),
                    cx,
                ),
            });
        })
        .detach();
    }
}

fn dirs_home() -> PathBuf {
    focus_core::settings::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub struct OpsVariables(
    std::sync::RwLock<focus_ops::vars::Context>,
    std::sync::atomic::AtomicBool,
);

impl focus_view_reading::CodeDecoration for OpsVariables {
    fn decorate(&self, _lang: &str, code: &str) -> Option<focus_view_reading::Decorated> {
        if !self.1.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        let placeholders = focus_ops::vars::find(code);
        if placeholders.is_empty() {
            return None;
        }
        let context = self
            .0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let spans = placeholders
            .into_iter()
            .map(|p| {
                let known = focus_ops::vars::substitute(&code[p.range.clone()], &context)
                    .missing
                    .is_empty();
                (
                    p.range,
                    if known {
                        focus_view_reading::Decor::Resolved
                    } else {
                        focus_view_reading::Decor::Unresolved
                    },
                )
            })
            .collect();
        let styled = |substituted: focus_ops::vars::Substituted| {
            let mut ranges: Vec<_> = substituted
                .resolved
                .into_iter()
                .map(|(range, _)| (range, focus_view_reading::Decor::Resolved))
                .chain(
                    substituted
                        .missing
                        .into_iter()
                        .map(|(range, _)| (range, focus_view_reading::Decor::Unresolved)),
                )
                .collect();
            ranges.sort_by_key(|(range, _)| range.start);
            (substituted.text, ranges)
        };
        let (resolved, resolved_spans) = styled(focus_ops::vars::substitute_masked(code, &context));
        let clear = styled(focus_ops::vars::substitute(code, &context));
        let revealed = (clear.0 != resolved).then_some(clear);
        Some(focus_view_reading::Decorated {
            spans,
            resolved,
            resolved_spans,
            revealed,
        })
    }
}

impl Shell {
    pub fn watch_ops_variables(&mut self, cx: &mut Context<Self>) {
        let Some(ops) = &self.ops else { return };
        let state = ops.state.clone();
        let decoration = std::sync::Arc::new(OpsVariables(
            Default::default(),
            std::sync::atomic::AtomicBool::new(true),
        ));
        self.code_decoration = Some(decoration.clone());
        let update = move |this: &mut Shell, state: Entity<OpsState>, cx: &mut Context<Shell>| {
            let context = state
                .read(cx)
                .active
                .as_ref()
                .map(|a| a.context.clone())
                .unwrap_or_default();
            let settings = &state.read(cx).settings;
            let substitute = settings.substitute;
            decoration.1.store(
                substitute && settings.code_variables,
                std::sync::atomic::Ordering::Relaxed,
            );
            if this.prefs.read(cx).bool("ops.substitute") != substitute {
                this.set_pref(
                    "ops.substitute",
                    crate::settings::schema::Value::Bool(substitute),
                    cx,
                );
            }
            *decoration
                .0
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = context;
            this.spread_code_decoration(cx);
            this.spread_code_variables(cx);
            this.spread_graph_relations(cx);
        };
        update(self, state.clone(), cx);
        self._subscriptions
            .push(cx.observe(&state, move |this, state, cx| update(this, state, cx)));
    }

    pub fn apply_engagement_trash(&mut self, cx: &mut Context<Self>) {
        let (Some(ops), Some(library)) = (&self.ops, &self.library) else {
            return;
        };
        let kind = if self.prefs.read(cx).choice("files.trash") == 1 {
            focus_core::TrashKind::System
        } else {
            focus_core::TrashKind::Vault
        };
        let vault = library.vault().clone();
        let trash = focus_view_ops::EngagementTrash {
            system: kind == focus_core::TrashKind::System,
            mover: std::sync::Arc::new(move |absolute: &Path| {
                let relative = absolute
                    .strip_prefix(vault.root())
                    .map_err(|e| e.to_string())?;
                vault.trash_with(relative, kind).map_err(|e| e.to_string())
            }),
        };
        ops.state.update(cx, |state, _| state.trash = Some(trash));
    }

    pub fn apply_ops_settings(&mut self, cx: &mut Context<Self>) {
        let Some(ops) = &self.ops else { return };
        let settings = focus_view_ops::OpsSettings::from_core(&self.store.settings);
        let folders = settings.library_folders.clone();
        ops.state
            .update(cx, |state, cx| state.set_settings(settings, cx));
        ops.commands
            .update(cx, |commands, cx| commands.set_folders(folders, cx));
    }

    pub fn spread_graph_relations(&mut self, cx: &mut Context<Self>) {
        let (Some(graph), Some(root)) = (self.graph.clone(), self.vault_root.clone()) else {
            return;
        };
        let relations = self
            .ops
            .as_ref()
            .and_then(|ops| {
                ops.state
                    .read(cx)
                    .active
                    .as_ref()
                    .map(|active| active.graph_relations(&root))
            })
            .unwrap_or_default();
        graph.update(cx, |graph, cx| graph.set_relations(relations, cx));
    }

    pub fn spread_code_variables(&mut self, cx: &mut Context<Self>) {
        let list = self.code_variables(cx);
        for tab in self.all_tabs() {
            let list = list.clone();
            tab.update(cx, |editor, cx| editor.set_code_variables(list, cx));
        }
    }

    pub fn code_variables(
        &self,
        cx: &gpui::App,
    ) -> Option<std::sync::Arc<focus_view_reading::CodeVariables>> {
        let ops = self.ops.as_ref()?;
        let state = ops.state.read(cx);
        if !state.settings.code_variables {
            return None;
        }
        let context = state
            .active
            .as_ref()
            .map(|a| a.context.clone())
            .unwrap_or_default();
        let variable =
            |name: String, canonical: &str, description: String| focus_view_reading::CodeVariable {
                name,
                value: context.values.get(canonical).cloned().unwrap_or_default(),
                description,
                secret: focus_ops::links::is_secret(canonical),
            };
        let standard = focus_ops::vars::standard();
        let mut list: Vec<focus_view_reading::CodeVariable> = standard
            .iter()
            .map(|s| {
                variable(
                    s.name.trim_start_matches('$').to_string(),
                    s.key,
                    focus_i18n::t(&s.description_key),
                )
            })
            .collect();
        let known = |canonical: &str| standard.iter().any(|s| s.key == canonical);
        let mut added: Vec<(String, String)> = Vec::new();
        let stored = state.active.iter().flat_map(|a| a.engagement.vars.keys());
        for name in stored.chain(context.values.keys()) {
            let canonical = focus_ops::vars::canonical_name(name);
            if !known(&canonical) && !added.iter().any(|(_, c)| *c == canonical) {
                added.push((name.clone(), canonical));
            }
        }
        list.extend(
            added
                .into_iter()
                .map(|(name, canonical)| variable(name, &canonical, String::new())),
        );
        Some(std::sync::Arc::new(focus_view_reading::CodeVariables {
            list,
            engaged: state.active.is_some(),
        }))
    }

    pub fn on_variable_menu(
        &mut self,
        menu: &focus_view_reading::VariableMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match menu {
            focus_view_reading::VariableMenu::Set { name, value } => {
                let Some(ops) = &self.ops else { return };
                let name = focus_ops::vars::canonical_name(name);
                ops.state
                    .update(cx, |state, cx| state.set_variable(&name, value, cx));
            }
            focus_view_reading::VariableMenu::NewEngagement => self.new_engagement(window, cx),
        }
    }

    pub fn spread_code_decoration(&mut self, cx: &mut Context<Self>) {
        let decoration = self
            .code_decoration
            .clone()
            .map(|d| d as std::sync::Arc<dyn focus_view_reading::CodeDecoration>);
        for tab in self.all_tabs() {
            let decoration = decoration.clone();
            tab.update(cx, |editor, cx| editor.set_code_decoration(decoration, cx));
        }
        if let Some(preview) = &self.link_preview {
            preview.update(cx, |preview, cx| {
                preview.set_code_decoration(decoration, cx)
            });
        }
    }
}

impl Shell {
    fn export_report(
        &mut self,
        path: &Path,
        format: focus_view_ops::ReportFormat,
        cx: &mut Context<Self>,
    ) {
        let (Some(library), Some(relative)) = (self.library.clone(), self.vault_relative(path))
        else {
            return;
        };
        let Some(note) = library.note_by_path(&relative) else {
            return;
        };
        let prefs = self.prefs.read(cx);
        let options = focus_view_transfer::export::Options {
            dark: prefs.choice("transfer.html_theme") == 0,
            embed_images: prefs.bool("transfer.html_images"),
            embed_fonts: prefs.bool("transfer.html_fonts"),
            ..Default::default()
        };
        let request = focus_view_transfer::export::Request {
            scope: focus_view_transfer::export::Scope::Note(note),
            format: match format {
                focus_view_ops::ReportFormat::Html => focus_view_transfer::export::Format::Html,
                focus_view_ops::ReportFormat::Pdf => focus_view_transfer::export::Format::Pdf,
            },
            options,
            destination: path.parent().map(Path::to_path_buf).unwrap_or_default(),
            language: focus_i18n::language(),
        };
        let source = path.to_path_buf();
        let task = cx.background_spawn(async move {
            let report = focus_view_transfer::export::run(
                library.vault(),
                &request,
                &focus_view_transfer::export::Cancel::new(),
                &mut |_| {},
            )?;
            if let Some(file) = &report.main_file {
                crate::private::same_rights(&source, file)
                    .map_err(|error| anyhow::anyhow!("restricting {}: {error}", file.display()))?;
            }
            Ok::<_, anyhow::Error>(report)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(report) => {
                    let written = report.main_file.unwrap_or(report.folder);
                    this.notify_user(
                        focus_kit::Tone::Ok,
                        focus_i18n::t_args(
                            "ui-file-saved",
                            &[("path", written.display().to_string().as_str().into())],
                        ),
                        cx,
                    );
                }
                Err(error) => this.notify_user(
                    focus_kit::Tone::Danger,
                    focus_i18n::t_args(
                        "errors-export-report",
                        &[("detail", error.to_string().as_str().into())],
                    ),
                    cx,
                ),
            });
        })
        .detach();
    }
}

impl Shell {
    pub fn quick_journal_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ops) = &self.ops else { return };
        let sidebar = ops.sidebar.clone();
        self.layout.right_open = true;
        self.layout.right_tab = crate::shell::RightTab::Ops;
        sidebar.update(cx, |sidebar, cx| sidebar.focus_entry(window, cx));
        cx.notify();
    }
}
