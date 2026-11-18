use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use focus_ai::agent::{AgentEvent, ToolHost, ToolOutcome};
use focus_ai::prompts::{self, Output, Scope, Template, Vars};
use focus_ai::redact::Options;
use focus_ai::{Cancel, Error, Message, Role, StreamEvent, ToolCall, Usage, history};
use focus_core::Vault;
use focus_i18n::{Arg, t, t_args};
use focus_search::{FuzzyHit, FuzzyScope, SearchIndex};
use focus_view_reading::model::LinkGoal;
use focus_view_reading::{LinkPreview, ReadingEvent, ReadingView, Resolver, Typesetting};
use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, ListAlignment,
    ListState, Subscription, Task, Window, actions, px,
};
use gpui_component::input::{InputEvent, InputState};

const MAX_PANEL_WIDTH: f32 = 560.;

use crate::backend::{ChatBackend, RunOptions};
use crate::context::{self, Chip, ChipKind, EditorContext, Snapshot};
use crate::errors;
use crate::events::{AiEvent, NoticeTone};
use crate::gate::{Proposal, WriteGate};
use crate::mask::{Mask, MaskingHost, StreamRestorer};
use crate::model::{
    ActionRun, Assistant, Decision, Entry, FailureRow, Offer, Part, ToolLine, ToolState, UserTurn,
    WriteCard, strip_fence,
};
use crate::settings::{AiSettings, language_name};

actions!(
    focus_ai_panel,
    [
        Submit,
        Newline,
        MentionNext,
        MentionPrev,
        MentionAccept,
        Dismiss,
        PageUp,
        PageDown,
        FirstItem,
        LastItem
    ]
);

pub fn bind(cx: &mut App) {
    use gpui::KeyBinding;
    const IN_COMPOSER: Option<&str> = Some("AiComposer > Input");
    cx.bind_keys([
        KeyBinding::new("enter", Submit, IN_COMPOSER),
        KeyBinding::new("shift-enter", Newline, IN_COMPOSER),
        KeyBinding::new("down", MentionNext, IN_COMPOSER),
        KeyBinding::new("up", MentionPrev, IN_COMPOSER),
        KeyBinding::new("tab", MentionAccept, IN_COMPOSER),
        KeyBinding::new("escape", Dismiss, Some("AiPanel")),
        KeyBinding::new("pageup", PageUp, Some("AiPanel")),
        KeyBinding::new("pagedown", PageDown, Some("AiPanel")),
        KeyBinding::new("home", FirstItem, Some("AiPanel")),
        KeyBinding::new("end", LastItem, Some("AiPanel")),
        KeyBinding::new("escape", Dismiss, IN_COMPOSER),
    ]);
}

pub struct Services {
    pub vault: Arc<Vault>,
    pub index: Arc<SearchIndex>,
    pub editor: std::rc::Rc<dyn EditorContext>,
    pub tools: Arc<dyn ToolHost + Send + Sync>,
    pub gate: Arc<WriteGate>,
    pub history_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Overlay {
    None,
    Actions,
    Models,
}

pub(crate) struct Mention {
    pub start: usize,
    pub hits: Vec<FuzzyHit>,
    pub selected: usize,
}

pub(crate) struct Run {
    pub cancel: Cancel,
    pub started: Instant,
}

enum Msg {
    Masked(usize),
    McpFailed(String, String),
    Event(AgentEvent),
    Finished {
        result: Result<Usage, Error>,
        new: Vec<Message>,
    },
}

pub struct AiPanel {
    pub(crate) focus: FocusHandle,
    pub(crate) input: Entity<InputState>,
    pub(crate) services: Services,
    pub(crate) backend: Option<Arc<dyn ChatBackend>>,
    pub(crate) settings: AiSettings,
    pub(crate) model: String,
    pub(crate) entries: Vec<Entry>,
    pub(crate) wire: Vec<Message>,
    pub(crate) conversation_id: Option<String>,
    pub(crate) usage_total: Usage,
    pub(crate) run: Option<Run>,
    pub(crate) mask: Arc<Mask>,
    known_secrets: Vec<String>,
    pub(crate) masked_last: usize,
    pub(crate) snapshot: Snapshot,
    pub(crate) note_dismissed: Option<PathBuf>,
    pub(crate) selection_dismissed: bool,
    pub(crate) added: Vec<Chip>,
    pub(crate) mention: Option<Mention>,
    pub(crate) overlay: Overlay,
    pub(crate) history_open: bool,
    pub(crate) history_scroll: gpui::ScrollHandle,
    pub(crate) copied: Option<(usize, u64)>,
    pub(crate) history: Vec<(String, String, u64)>,
    pub(crate) list: ListState,
    pub(crate) listed: usize,
    pub(crate) follow: bool,
    pub(crate) new_below: bool,
    pub(crate) last_extent: gpui::Pixels,
    pub(crate) preview: Option<(Entity<LinkPreview>, gpui::Point<gpui::Pixels>)>,
    preview_entity: Option<Entity<LinkPreview>>,
    pub(crate) models: Vec<String>,
    models_requested: bool,
    last_parse: Instant,
    parse_due: Option<Task<()>>,
    pub(crate) blink: bool,
    pub(crate) input_tokens: u32,
    pub(crate) placeholder_generation: u64,
    _subscriptions: Vec<Subscription>,
    _tasks: Vec<Task<()>>,
}

impl EventEmitter<AiEvent> for AiPanel {}

impl Focusable for AiPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn iso_date(secs: u64) -> String {
    let days = (secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    format!(
        "{:04}-{month:02}-{day:02}",
        if month <= 2 { year + 1 } else { year }
    )
}

fn mask_for(settings: &AiSettings) -> Mask {
    Mask::new(settings.redact.then_some(Options {
        secrets: true,
        ip_addresses: settings.redact_ips,
        hostnames: false,
    }))
}

impl AiPanel {
    pub fn new(
        services: Services,
        backend: Option<Arc<dyn ChatBackend>>,
        settings: AiSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(2, 8)
                .placeholder(t("ai-input-placeholder"))
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut AiPanel, _, event: &InputEvent, window, cx| {
                if let InputEvent::Change = event {
                    this.input_changed(window, cx);
                }
            },
        );
        let mut tasks = Vec::new();
        if let Some(receiver) = services.gate.take_receiver() {
            tasks.push(cx.spawn(async move |this, cx| {
                while let Ok(proposal) = receiver.recv().await {
                    if this
                        .update(cx, |this, cx| this.proposal_arrived(proposal, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        let snapshot = services.editor.snapshot(cx);
        let list = ListState::new(0, ListAlignment::Top, px(240.));
        let weak = cx.weak_entity();
        let tracked = list.clone();
        let last_offset = std::rc::Rc::new(std::cell::Cell::new(px(0.)));
        list.set_scroll_handler(move |_, _, cx| {
            let offset = -tracked.scroll_px_offset_for_scrollbar().y;
            let at_bottom = tracked.max_offset_for_scrollbar().height - offset <= px(4.);
            let moved_up = offset + px(1.) < last_offset.get();
            last_offset.set(offset);
            weak.update(cx, |this, _| {
                if at_bottom {
                    this.follow = true;
                    this.new_below = false;
                } else if moved_up {
                    this.follow = false;
                }
            })
            .ok();
        });
        AiPanel {
            focus: cx.focus_handle(),
            input,
            mask: Arc::new(mask_for(&settings)),
            known_secrets: Vec::new(),
            model: settings.model.clone(),
            services,
            backend,
            settings,
            entries: Vec::new(),
            wire: Vec::new(),
            conversation_id: None,
            usage_total: Usage::default(),
            run: None,
            masked_last: 0,
            snapshot,
            note_dismissed: None,
            selection_dismissed: false,
            added: Vec::new(),
            mention: None,
            overlay: Overlay::None,
            history_open: false,
            history_scroll: gpui::ScrollHandle::new(),
            copied: None,
            history: Vec::new(),
            list,
            listed: 0,
            follow: true,
            new_below: false,
            last_extent: px(0.),
            preview: None,
            preview_entity: None,
            models: Vec::new(),
            models_requested: false,
            last_parse: Instant::now(),
            parse_due: None,
            blink: true,
            input_tokens: 0,
            placeholder_generation: focus_i18n::generation(),
            _subscriptions: vec![subscription],
            _tasks: tasks,
        }
    }

    pub fn set_backend(&mut self, backend: Option<Arc<dyn ChatBackend>>, cx: &mut Context<Self>) {
        self.backend = backend;
        self.models.clear();
        self.models_requested = false;
        cx.notify();
    }

    pub(crate) fn live_backend(&self) -> Option<Arc<dyn ChatBackend>> {
        self.settings
            .enabled
            .then(|| self.backend.clone())
            .flatten()
    }

    pub fn set_settings(&mut self, settings: AiSettings, cx: &mut Context<Self>) {
        if !settings.enabled {
            if let Some(run) = &self.run {
                run.cancel.cancel();
            }
            self.overlay = Overlay::None;
            self.mention = None;
        }
        self.mask = Arc::new(mask_for(&settings));
        self.mask.learn(&self.known_secrets);
        self.model = settings.model.clone();
        self.settings = settings;
        cx.notify();
    }

    pub fn set_known_secrets(&mut self, secrets: Vec<String>) {
        self.mask.learn(&secrets);
        self.known_secrets = secrets;
    }

    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    pub fn refresh_context(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.services.editor.snapshot(cx);
        if snapshot.note.as_ref().map(|n| &n.path) != self.snapshot.note.as_ref().map(|n| &n.path) {
            self.note_dismissed = None;
        }
        if snapshot.selection != self.snapshot.selection {
            self.selection_dismissed = false;
        }
        self.snapshot = snapshot;
        cx.notify();
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub(crate) fn chips(&self) -> Vec<Chip> {
        let mut chips = Vec::new();
        if let Some(note) = &self.snapshot.note
            && self.note_dismissed.as_ref() != Some(&note.path)
        {
            chips.push(Chip::new(
                ChipKind::Note,
                note.title.clone(),
                Some(note.path.clone()),
                note.text.clone(),
            ));
        }
        if let Some(selection) = &self.snapshot.selection
            && !selection.is_empty()
            && !self.selection_dismissed
        {
            chips.push(Chip::new(
                ChipKind::Selection,
                t("ai-context-selection"),
                self.snapshot.note.as_ref().map(|n| n.path.clone()),
                selection.clone(),
            ));
        }
        chips.extend(self.added.iter().cloned());
        chips
    }

    pub(crate) fn remove_chip(
        &mut self,
        kind: ChipKind,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        match kind {
            ChipKind::Note => {
                self.note_dismissed = self.snapshot.note.as_ref().map(|n| n.path.clone())
            }
            ChipKind::Selection => self.selection_dismissed = true,
            ChipKind::Added => self.added.retain(|c| c.path != path),
        }
        cx.notify();
    }

    fn input_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (text, cursor) = self
            .input
            .read_with(cx, |input, _| (input.value().to_string(), input.cursor()));
        self.input_tokens = focus_ai::estimate_tokens(&text);
        self.mention = mention_at(&text, cursor).map(|(start, query)| {
            let hits = self.services.index.fuzzy(&query, FuzzyScope::Notes, &[], 6);
            Mention {
                start,
                hits,
                selected: 0,
            }
        });
        let _ = window;
        cx.notify();
    }

    pub(crate) fn mention_move(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let Some(mention) = &mut self.mention else {
            return false;
        };
        if mention.hits.is_empty() {
            return false;
        }
        let count = mention.hits.len();
        mention.selected = if forward {
            (mention.selected + 1) % count
        } else {
            (mention.selected + count - 1) % count
        };
        cx.notify();
        true
    }

    pub(crate) fn mention_accept(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mention) = self.mention.take() else {
            return false;
        };
        let Some(hit) = mention.hits.get(index.unwrap_or(mention.selected)).cloned() else {
            cx.notify();
            return false;
        };
        let (text, cursor) = self
            .input
            .read_with(cx, |input, _| (input.value().to_string(), input.cursor()));
        let end = cursor.min(text.len());
        let start = mention.start.min(end);
        let rest = format!("{}{}", &text[..start], &text[end..]);
        self.input
            .update(cx, |input, cx| input.set_value(rest, window, cx));
        if let Some(id) = hit.note {
            self.add_note_chip(id, cx);
        }
        cx.notify();
        true
    }

    pub(crate) fn add_note_chip(&mut self, id: focus_core::NoteId, cx: &mut Context<Self>) {
        let vault = self.services.vault.clone();
        let Some(meta) = vault.note(id) else { return };
        let path: PathBuf = meta.path.to_path_buf();
        if self.added.iter().any(|c| c.path.as_ref() == Some(&path)) {
            return;
        }
        let title = meta.title.to_string();
        cx.spawn(async move |this, cx| {
            let read = cx.background_spawn(async move { vault.read(id) }).await;
            this.update(cx, |this, cx| match read {
                Ok(text) => {
                    this.added
                        .push(Chip::new(ChipKind::Added, title, Some(path), text.into()));
                    cx.notify();
                }
                Err(error) => this.notify(NoticeTone::Danger, error.to_string(), cx),
            })
            .ok();
        })
        .detach();
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.mention.take().is_some() || self.overlay != Overlay::None {
            self.overlay = Overlay::None;
            cx.notify();
            return true;
        }
        false
    }

    pub fn show_actions(&mut self, cx: &mut Context<Self>) {
        self.overlay = Overlay::None;
        self.toggle_overlay(Overlay::Actions, cx);
    }

    pub fn show_models(&mut self, cx: &mut Context<Self>) {
        self.overlay = Overlay::None;
        self.toggle_overlay(Overlay::Models, cx);
    }

    fn fetch_models(&mut self, cx: &mut Context<Self>) {
        let Some(backend) = self.live_backend() else {
            return;
        };
        if std::mem::replace(&mut self.models_requested, true) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let fetched = cx.background_spawn(async move { backend.models() }).await;
            this.update(cx, |this, cx| {
                if let Ok(models) = fetched {
                    this.models = models;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn answer_pending_write(&mut self, apply: bool, cx: &mut Context<Self>) {
        let found = self
            .entries
            .iter()
            .enumerate()
            .rev()
            .find_map(|(entry, e)| match e {
                Entry::Assistant(a) => a
                    .parts
                    .iter()
                    .position(|p| matches!(p, Part::Write(c) if c.decision == Decision::Pending))
                    .map(|part| (entry, part)),
                _ => None,
            });
        if let Some((entry, part)) = found {
            self.answer_write(entry, part, apply, cx);
        }
    }

    pub(crate) fn toggle_overlay(&mut self, overlay: Overlay, cx: &mut Context<Self>) {
        self.overlay = if self.overlay == overlay {
            Overlay::None
        } else {
            overlay
        };
        self.mention = None;
        if overlay == Overlay::Actions && self.overlay == Overlay::Actions {
            self.refresh_context(cx);
        }
        if overlay == Overlay::Models && self.overlay == Overlay::Models {
            self.fetch_models(cx);
        }
        cx.notify();
    }

    pub(crate) fn notify(&mut self, tone: NoticeTone, text: String, cx: &mut Context<Self>) {
        cx.emit(AiEvent::Notify { tone, text });
    }

    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if text.is_empty() || self.run.is_some() || self.live_backend().is_none() {
            return;
        }
        self.refresh_context(cx);
        let chips = self.chips();
        let prompt = context::compose(&chips, &text);
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.input_tokens = 0;
        self.begin_turn(text, chips, prompt, None, cx);
    }

    pub fn ask(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_value(text.to_string(), window, cx)
        });
        self.submit(window, cx);
    }

    pub fn run_action(&mut self, id: &str, cx: &mut Context<Self>) {
        self.overlay = Overlay::None;
        if self.run.is_some() || self.live_backend().is_none() {
            return;
        }
        self.refresh_context(cx);
        let Some(template) = prompts::merged(&self.settings.actions)
            .into_iter()
            .find(|t| t.id == id)
        else {
            return;
        };
        let Some((target, path)) = self.action_target(&template) else {
            return;
        };
        let note = self.snapshot.note.clone();
        let language = language_name(&self.settings.response_language(&target));
        let tags = note.as_ref().map(|n| n.tags.clone()).unwrap_or_default();
        let note_text = note
            .as_ref()
            .map(|n| n.text.to_string())
            .unwrap_or_default();
        let selection = if template.scope == Scope::Selection {
            target.as_str()
        } else {
            ""
        };
        let note_var = if template.scope == Scope::Note {
            target.as_str()
        } else {
            note_text.as_str()
        };
        let title = note.as_ref().map(|n| n.title.as_str()).unwrap_or_default();
        let path_text = path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let date = iso_date(now_secs());
        let prompt = prompts::render(
            &template,
            &Vars {
                selection,
                note: note_var,
                title,
                path: &path_text,
                tags: &tags,
                date: &date,
                language: &language,
            },
        );
        let kind = if template.scope == Scope::Selection {
            ChipKind::Selection
        } else {
            ChipKind::Note
        };
        let label = if template.scope == Scope::Selection {
            t("ai-context-selection")
        } else {
            title.to_string()
        };
        let chip = Chip::new(kind, label, path, target.clone().into());
        let shown = action_label(&template);
        let run = ActionRun {
            scope: template.scope,
            template,
            target,
        };
        self.begin_turn(shown, vec![chip], prompt, Some(run), cx);
    }

    pub(crate) fn action_target(&self, template: &Template) -> Option<(String, Option<PathBuf>)> {
        let note = self.snapshot.note.as_ref();
        let path = note.map(|n| n.path.clone());
        match template.scope {
            Scope::Selection => self
                .snapshot
                .selection
                .as_ref()
                .filter(|s| !s.is_empty())
                .map(|s| (s.to_string(), path)),
            Scope::Note => note.map(|n| (n.text.to_string(), path)),
        }
    }

    fn begin_turn(
        &mut self,
        shown: String,
        chips: Vec<Chip>,
        prompt: String,
        action: Option<ActionRun>,
        cx: &mut Context<Self>,
    ) {
        self.history_open = false;
        self.overlay = Overlay::None;
        self.follow = true;
        let wire_start = self.wire.len();
        self.wire.push(Message {
            role: Role::User,
            content: prompt.clone(),
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        });
        self.entries.push(Entry::User(UserTurn {
            text: shown,
            chips,
            masked: 0,
            wire_start,
            prompt,
            action,
        }));
        self.launch(cx);
    }

    fn launch(&mut self, cx: &mut Context<Self>) {
        let Some(backend) = self.live_backend() else {
            return;
        };
        let mut assistant = Assistant::new();
        assistant.live = true;
        self.entries.push(Entry::Assistant(assistant));
        let cancel = Cancel::new();
        self.run = Some(Run {
            cancel: cancel.clone(),
            started: Instant::now(),
        });
        self.mask.begin_run();

        let note_text = self
            .snapshot
            .note
            .as_ref()
            .map(|n| n.text.to_string())
            .unwrap_or_default();
        let system = prompts::system(
            self.settings.ops_mode,
            &self.settings.preprompt,
            &self.settings.response_language(&note_text),
        );
        let mut messages = vec![Message {
            role: Role::System,
            content: system,
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }];
        messages.extend(self.wire.iter().cloned());
        let options = RunOptions {
            base_url: self.settings.base_url.clone(),
            model: self.model.clone(),
            thinking: self.settings.thinking,
            temperature: self.settings.temperature,
            max_tokens: self.settings.max_tokens,
            max_steps: self.settings.max_steps,
        };
        let (tools, gate, servers, writes) = (
            self.services.tools.clone(),
            self.services.gate.clone(),
            self.settings.mcp_servers.clone(),
            self.settings.write_tools,
        );
        let host_mask = self.mask.clone();
        let mask = self.mask.clone();
        let (sender, receiver) = smol::channel::unbounded::<Msg>();

        std::thread::spawn(move || {
            let host = MaskingHost {
                inner: tools_with_servers(tools, &gate, &servers, &sender),
                mask: host_mask,
                writes,
            };
            let mut working: Vec<Message> = messages.iter().map(|m| mask.mask_message(m)).collect();
            let _ = sender.send_blocking(Msg::Masked(mask.count()));
            let sent = working.len();
            let (mut content, mut reasoning) =
                (StreamRestorer::default(), StreamRestorer::default());
            let result = backend.run(&options, &mut working, &host, &cancel, &mut |event| {
                let flush = |restorer: &mut StreamRestorer, make: fn(String) -> StreamEvent| {
                    let rest = restorer.flush(&mask);
                    (!rest.is_empty()).then(|| AgentEvent::Stream(make(rest)))
                };
                let mut out = Vec::new();
                match event {
                    AgentEvent::Stream(StreamEvent::Content(chunk)) => {
                        out.extend(flush(&mut reasoning, StreamEvent::Reasoning));
                        let text = content.push(&mask, &chunk);
                        out.push(AgentEvent::Stream(StreamEvent::Content(text)));
                    }
                    AgentEvent::Stream(StreamEvent::Reasoning(chunk)) => {
                        let text = reasoning.push(&mask, &chunk);
                        out.push(AgentEvent::Stream(StreamEvent::Reasoning(text)));
                    }
                    AgentEvent::Stream(StreamEvent::ToolCall(call)) => {
                        out.extend(flush(&mut reasoning, StreamEvent::Reasoning));
                        out.extend(flush(&mut content, StreamEvent::Content));
                        out.push(AgentEvent::Stream(StreamEvent::ToolCall(restore_call(
                            &mask, &call,
                        ))));
                    }
                    AgentEvent::ToolStarted(call) => {
                        out.push(AgentEvent::ToolStarted(restore_call(&mask, &call)))
                    }
                    AgentEvent::ToolFinished { call_id, outcome } => {
                        out.push(AgentEvent::ToolFinished {
                            call_id,
                            outcome: restore_outcome(&mask, outcome),
                        })
                    }
                    other => out.push(other),
                }
                for event in out {
                    let _ = sender.send_blocking(Msg::Event(event));
                }
            });
            for event in [
                flush_final(&mut reasoning, &mask, StreamEvent::Reasoning),
                flush_final(&mut content, &mask, StreamEvent::Content),
            ]
            .into_iter()
            .flatten()
            {
                let _ = sender.send_blocking(Msg::Event(event));
            }
            let new = working
                .get(sent..)
                .unwrap_or_default()
                .iter()
                .map(|m| mask.restore_message(m))
                .collect();
            let _ = sender.send_blocking(Msg::Finished { result, new });
        });

        self._tasks.push(cx.spawn(async move |this, cx| {
            while let Ok(first) = receiver.recv().await {
                let mut batch = vec![first];
                while let Ok(more) = receiver.try_recv() {
                    batch.push(more);
                }
                let finished = batch.iter().any(|m| matches!(m, Msg::Finished { .. }));
                if this
                    .update(cx, |this, cx| {
                        for msg in batch {
                            this.apply(msg, cx);
                        }
                        this.reparse_live(false, cx);
                        this.touch_last();
                        cx.notify();
                    })
                    .is_err()
                    || finished
                {
                    break;
                }
            }
        }));
        if !self.settings.reduce_motion {
            self._tasks.push(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    let alive = this.update(cx, |this, cx| {
                        this.blink = !this.blink;
                        this.update_cursors(cx);
                        cx.notify();
                        this.run.is_some()
                    });
                    if !alive.unwrap_or(false) {
                        break;
                    }
                }
            }));
        }
        cx.notify();
    }

    pub(crate) fn touch(&mut self, ix: usize) {
        if ix < self.listed {
            self.list.splice(ix..ix + 1, 1);
        }
    }

    fn touch_last(&mut self) {
        if let Some(ix) = self.entries.len().checked_sub(1) {
            self.touch(ix);
        }
    }

    fn reparse_live(&mut self, force: bool, cx: &mut Context<Self>) {
        const EVERY: Duration = Duration::from_millis(50);
        let since = self.last_parse.elapsed();
        if !force && since < EVERY {
            if self.parse_due.is_none() {
                self.parse_due = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(EVERY).await;
                    this.update(cx, |this, cx| {
                        this.parse_due = None;
                        this.reparse_live(true, cx);
                    })
                    .ok();
                }));
            }
            return;
        }
        self.last_parse = Instant::now();
        self.sync_texts(cx);
        self.touch_last();
        cx.notify();
    }

    fn sync_texts(&mut self, cx: &mut Context<Self>) {
        let link = self.linker();
        let resolver: Arc<dyn Resolver> = self.services.vault.clone();
        let mut work: Vec<(usize, usize, String, bool)> = Vec::new();
        for (e, entry) in self.entries.iter().enumerate() {
            if let Entry::Assistant(a) = entry {
                for (p, part) in a.parts.iter().enumerate() {
                    if let Part::Text {
                        text, view, dirty, ..
                    } = part
                        && (*dirty || view.is_none())
                    {
                        work.push((e, p, link(text), view.is_none()));
                    }
                }
            }
        }
        for (e, p, text, create) in work {
            let view = if create {
                let view =
                    cx.new(|cx| ReadingView::compact(Typesetting::compact(MAX_PANEL_WIDTH), cx));
                let events = cx
                    .subscribe(&view, |this: &mut AiPanel, _, event: &ReadingEvent, cx| {
                        this.reading_event(event, cx)
                    });
                let grown = cx.observe(&view, |this: &mut AiPanel, view, cx| {
                    if let Some(ix) = this.entry_of_view(&view) {
                        this.touch(ix);
                    }
                    cx.notify();
                });
                if let Some(Entry::Assistant(a)) = self.entries.get_mut(e)
                    && let Some(Part::Text {
                        view: slot,
                        subscriptions,
                        ..
                    }) = a.parts.get_mut(p)
                {
                    *slot = Some(view.clone());
                    subscriptions.extend([events, grown]);
                }
                view
            } else if let Some(Entry::Assistant(a)) = self.entries.get(e)
                && let Some(Part::Text {
                    view: Some(view), ..
                }) = a.parts.get(p)
            {
                view.clone()
            } else {
                continue;
            };
            view.update(cx, |view, cx| {
                view.set_markdown(text, None, resolver.clone(), cx)
            });
            if let Some(Entry::Assistant(a)) = self.entries.get_mut(e)
                && let Some(Part::Text { dirty, .. }) = a.parts.get_mut(p)
            {
                *dirty = false;
            }
        }
        self.update_cursors(cx);
    }

    pub(crate) fn update_cursors(&mut self, cx: &mut Context<Self>) {
        let visible = self.blink || self.settings.reduce_motion;
        for entry in &self.entries {
            if let Entry::Assistant(a) = entry {
                let last = a.parts.len().saturating_sub(1);
                for (p, part) in a.parts.iter().enumerate() {
                    if let Part::Text {
                        view: Some(view), ..
                    } = part
                    {
                        let on = (a.live && p == last).then_some(visible);
                        view.update(cx, |view, cx| view.set_cursor(on, cx));
                    }
                }
            }
        }
    }

    fn entry_of_view(&self, view: &Entity<ReadingView>) -> Option<usize> {
        self.entries.iter().position(|e| matches!(e, Entry::Assistant(a) if a.parts.iter().any(|p| matches!(p, Part::Text { view: Some(v), .. } if v == view))))
    }

    fn reading_event(&mut self, event: &ReadingEvent, cx: &mut Context<Self>) {
        match event {
            ReadingEvent::OpenLink { goal, .. } => match goal {
                LinkGoal::Note { id, .. } => {
                    if let Some(note) = self.services.vault.note(*id) {
                        cx.emit(AiEvent::OpenNote {
                            path: note.path.to_path_buf(),
                            line: None,
                        });
                    }
                }
                LinkGoal::Attachment(path) => cx.emit(AiEvent::OpenNote {
                    path: path.clone(),
                    line: None,
                }),
                LinkGoal::External(url) => cx.emit(AiEvent::OpenExternal(url.clone())),
                LinkGoal::Tag(name) => cx.emit(AiEvent::SearchTag(name.clone())),
                LinkGoal::Unresolved(target) => self.notify(
                    NoticeTone::Warn,
                    t_args(
                        "ai-link-unresolved",
                        &[("target", Arg::from(target.as_str()))],
                    ),
                    cx,
                ),
                LinkGoal::Footnote(_) => {}
            },
            ReadingEvent::Zoom(source) => cx.emit(AiEvent::ZoomDiagram(source.clone())),
            ReadingEvent::Hover {
                goal: Some(LinkGoal::Note { id, anchor }),
                position,
            } => {
                let preview = match &self.preview_entity {
                    Some(preview) => preview.clone(),
                    None => {
                        let preview = cx.new(LinkPreview::new);
                        self.preview_entity = Some(preview.clone());
                        preview
                    }
                };
                let resolver: Arc<dyn Resolver> = self.services.vault.clone();
                preview.update(cx, |preview, cx| {
                    preview.show(*id, anchor.clone(), resolver, cx)
                });
                self.preview = Some((preview, *position));
                cx.notify();
            }
            ReadingEvent::Hover { .. } => {
                if self.preview.take().is_some() {
                    cx.notify();
                }
            }
            ReadingEvent::ToggleTask(_) => {}
        }
    }

    fn live_assistant(&mut self) -> Option<&mut Assistant> {
        self.entries.iter_mut().rev().find_map(|e| {
            if let Entry::Assistant(a) = e {
                Some(a)
            } else {
                None
            }
        })
    }

    fn apply(&mut self, msg: Msg, cx: &mut Context<Self>) {
        match msg {
            Msg::Masked(count) => {
                self.masked_last = count;
                if let Some(Entry::User(turn)) = self
                    .entries
                    .iter_mut()
                    .rev()
                    .find(|e| matches!(e, Entry::User(_)))
                {
                    turn.masked = count;
                }
            }
            Msg::McpFailed(name, reason) => self.notify(
                NoticeTone::Warn,
                t_args(
                    "ai-mcp-failed",
                    &[
                        ("server", Arg::from(name.as_str())),
                        ("reason", Arg::from(reason.as_str())),
                    ],
                ),
                cx,
            ),
            Msg::Event(event) => self.apply_event(event),
            Msg::Finished { result, new } => self.finish(result, new, cx),
        }
    }

    fn apply_event(&mut self, event: AgentEvent) {
        let Some(assistant) = self.live_assistant() else {
            return;
        };
        match event {
            AgentEvent::Stream(StreamEvent::Content(chunk)) => assistant.push_content(&chunk),
            AgentEvent::Stream(StreamEvent::Reasoning(chunk)) => assistant.push_reasoning(&chunk),
            AgentEvent::Stream(StreamEvent::ToolsDisabled) => {
                assistant.close_reasoning();
                assistant.parts.push(Part::Notice("ai-tools-disabled"));
            }
            AgentEvent::ToolStarted(call) => {
                assistant.close_reasoning();
                assistant.parts.push(Part::Tool(ToolLine {
                    id: call.id,
                    name: call.name,
                    arguments: call.arguments,
                    started: Instant::now(),
                    took: None,
                    state: ToolState::Running,
                    open: false,
                }));
            }
            AgentEvent::ToolFinished { call_id, outcome } => {
                for part in &mut assistant.parts {
                    if let Part::Tool(line) = part
                        && line.id == call_id
                    {
                        line.took = Some(line.started.elapsed());
                        line.state = match &outcome {
                            ToolOutcome::Ok(text) => ToolState::Done(text.clone()),
                            ToolOutcome::Error(text) => ToolState::Failed(text.clone()),
                            ToolOutcome::Denied => ToolState::Refused,
                        };
                    }
                }
            }
            _ => {}
        }
    }

    fn finish(&mut self, result: Result<Usage, Error>, new: Vec<Message>, cx: &mut Context<Self>) {
        self.parse_due = None;
        self.sync_texts(cx);
        let started = self.run.take().map(|r| r.started);
        let model = self.model.clone();
        let user_index = self
            .entries
            .iter()
            .rposition(|e| matches!(e, Entry::User(_)));
        let action = user_index.and_then(|i| {
            if let Entry::User(turn) = &self.entries[i] {
                turn.action.clone()
            } else {
                None
            }
        });
        let wire_start = user_index.and_then(|i| {
            if let Entry::User(turn) = &self.entries[i] {
                Some(turn.wire_start)
            } else {
                None
            }
        });
        let provider = self.settings.provider.spec().label;
        let Some(assistant) = self.live_assistant() else {
            return;
        };
        assistant.live = false;
        assistant.close_reasoning();
        for part in &mut assistant.parts {
            match part {
                Part::Tool(line) if matches!(line.state, ToolState::Running) => {
                    line.state = ToolState::Refused
                }
                Part::Write(card) if card.decision == Decision::Pending => {
                    card.decision = Decision::Rejected
                }
                _ => {}
            }
        }
        let _ = started;
        match result {
            Ok(usage) => {
                assistant.cost = focus_ai::estimate_cost_usd(&model, &usage);
                assistant.usage = Some(usage.clone());
                let answer = assistant.answer();
                if let Some(action) = action
                    && action.template.output != Output::Chat
                    && !answer.trim().is_empty()
                {
                    let replacement = if action.template.output == Output::Replace {
                        strip_fence(&answer)
                    } else {
                        answer.trim().to_string()
                    };
                    let card = if action.template.output == Output::Replace {
                        WriteCard::replacement(&action.target, &replacement)
                    } else {
                        WriteCard::replacement("", "")
                    };
                    assistant.offer = Some(Offer {
                        output: action.template.output,
                        scope: action.scope,
                        original: action.target.clone(),
                        replacement,
                        title: action_label(&action.template),
                        card,
                    });
                }
                self.wire.extend(new);
                self.usage_total.prompt_tokens += usage.prompt_tokens;
                self.usage_total.completion_tokens += usage.completion_tokens;
                self.usage_total.cache_hit_tokens += usage.cache_hit_tokens;
                self.usage_total.cache_miss_tokens += usage.cache_miss_tokens;
                self.save_conversation(cx);
            }
            Err(Error::Cancelled) => {
                assistant.stopped = true;
                if let Some(start) = wire_start {
                    self.wire.truncate(start + 1);
                }
            }
            Err(error) => {
                let failure = errors::describe(&error, provider);
                let retry_at = failure
                    .retry_after
                    .map(|secs| Instant::now() + Duration::from_secs(u64::from(secs)));
                if assistant.is_empty() {
                    self.entries.pop();
                } else {
                    assistant.stopped = true;
                }
                if let Some(start) = wire_start {
                    self.wire.truncate(start + 1);
                }
                if let Some(at) = retry_at {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(at.saturating_duration_since(Instant::now()))
                            .await;
                        this.update(cx, |_, cx| cx.notify()).ok();
                    })
                    .detach();
                }
                self.entries
                    .push(Entry::Failure(FailureRow { failure, retry_at }));
            }
        }
        self.touch_last();
        if let Some(ix) = self.entries.len().checked_sub(2) {
            self.touch(ix);
        }
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = &self.run {
            run.cancel.cancel();
        }
        for entry in &mut self.entries {
            if let Entry::Assistant(a) = entry {
                for part in &mut a.parts {
                    if let Part::Write(card) = part
                        && card.decision == Decision::Pending
                    {
                        card.decision = Decision::Rejected;
                        card.reply = None;
                    }
                }
            }
        }
        self.touch_last_assistant();
        cx.notify();
    }

    pub fn retry(&mut self, cx: &mut Context<Self>) {
        if self.run.is_some() {
            return;
        }
        let Some(index) = self
            .entries
            .iter()
            .rposition(|e| matches!(e, Entry::User(_)))
        else {
            return;
        };
        let Entry::User(turn) = self.entries.remove(index) else {
            return;
        };
        self.entries.truncate(index);
        self.listed = index;
        self.list.reset(index);
        self.wire.truncate(turn.wire_start);
        self.begin_turn(turn.text, turn.chips, turn.prompt, turn.action, cx);
    }

    fn touch_last_assistant(&mut self) {
        if let Some(ix) = self
            .entries
            .iter()
            .rposition(|e| matches!(e, Entry::Assistant(_)))
        {
            self.touch(ix);
        }
    }

    fn proposal_arrived(&mut self, proposal: Proposal, cx: &mut Context<Self>) {
        let Some(assistant) = self.live_assistant() else {
            let _ = proposal.reply.send(false);
            return;
        };
        assistant.parts.push(Part::Write(WriteCard::proposal(
            &proposal.tool,
            proposal.path,
            proposal.before.as_deref(),
            &proposal.after,
            proposal.reply,
        )));
        self.touch_last_assistant();
        cx.notify();
    }

    pub(crate) fn answer_write(
        &mut self,
        entry: usize,
        part: usize,
        apply: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(Entry::Assistant(a)) = self.entries.get_mut(entry)
            && let Some(Part::Write(card)) = a.parts.get_mut(part)
            && card.decision == Decision::Pending
        {
            card.decision = if apply {
                Decision::Applied
            } else {
                Decision::Rejected
            };
            if let Some(reply) = card.reply.take() {
                let _ = reply.send(apply);
            }
            self.touch(entry);
            cx.notify();
        }
    }

    pub(crate) fn answer_offer(&mut self, entry: usize, apply: bool, cx: &mut Context<Self>) {
        let Some(Entry::Assistant(a)) = self.entries.get_mut(entry) else {
            return;
        };
        let Some(offer) = &mut a.offer else { return };
        if offer.card.decision != Decision::Pending {
            return;
        }
        offer.card.decision = if apply {
            Decision::Applied
        } else {
            Decision::Rejected
        };
        if !apply {
            self.touch(entry);
            cx.notify();
            return;
        }
        let event = match offer.output {
            Output::Replace => AiEvent::ReplaceText {
                scope: offer.scope,
                original: offer.original.clone(),
                replacement: offer.replacement.clone(),
            },
            Output::Insert => AiEvent::InsertBelow {
                scope: offer.scope,
                text: offer.replacement.clone(),
            },
            Output::Note | Output::Chat => AiEvent::CreateNote {
                title: note_title(&offer.replacement, &offer.title),
                content: offer.replacement.clone(),
            },
        };
        cx.emit(event);
        self.touch(entry);
        cx.notify();
    }

    pub fn toggle_part(&mut self, entry: usize, part: usize, cx: &mut Context<Self>) {
        if let Some(Entry::Assistant(a)) = self.entries.get_mut(entry) {
            match a.parts.get_mut(part) {
                Some(Part::Reasoning { open, .. }) => *open = !*open,
                Some(Part::Tool(line)) => line.open = !line.open,
                _ => {}
            }
            self.touch(entry);
            cx.notify();
        }
    }

    pub(crate) fn linker(&self) -> impl Fn(&str) -> String + 'static {
        let vault = self.services.vault.clone();
        move |text| crate::links::link_note_paths(text, &|path| vault.note_by_path(path).is_some())
    }

    pub(crate) fn scroll_jump(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.history_open {
            let (offset, max, height) = (
                self.history_scroll.offset(),
                self.history_scroll.max_offset(),
                self.history_scroll.bounds().size.height,
            );
            let y = if down {
                offset.y - height * 0.9
            } else {
                offset.y + height * 0.9
            };
            self.history_scroll
                .set_offset(gpui::point(offset.x, y.clamp(-max.height, px(0.))));
        } else {
            let height = self.list.viewport_bounds().size.height;
            self.list
                .scroll_by(if down { height * 0.9 } else { -height * 0.9 });
        }
        cx.notify();
    }

    pub(crate) fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.follow = true;
        self.new_below = false;
        cx.notify();
    }

    pub(crate) fn scroll_edge(&mut self, end: bool, cx: &mut Context<Self>) {
        if self.history_open {
            let max = self.history_scroll.max_offset();
            let offset = self.history_scroll.offset();
            self.history_scroll.set_offset(gpui::point(
                offset.x,
                if end { -max.height } else { px(0.) },
            ));
        } else {
            let item_ix = if end { self.list.item_count() } else { 0 };
            self.list.scroll_to(gpui::ListOffset {
                item_ix,
                offset_in_item: px(0.),
            });
        }
        cx.notify();
    }

    pub(crate) fn copy_answer(&mut self, entry: usize, cx: &mut Context<Self>) {
        if let Some(Entry::Assistant(a)) = self.entries.get(entry) {
            focus_kit::clipboard::copy_text(cx, a.answer());
            let token = self.copied.map_or(0, |(_, token)| token) + 1;
            self.copied = Some((entry, token));
            cx.notify();
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(1500))
                    .await;
                this.update(cx, |this, cx| {
                    if this.copied.is_some_and(|(_, current)| current == token) {
                        this.copied = None;
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    pub(crate) fn insert_answer(&mut self, entry: usize, cx: &mut Context<Self>) {
        if let Some(Entry::Assistant(a)) = self.entries.get(entry) {
            cx.emit(AiEvent::InsertAtCursor(a.answer()));
        }
    }

    pub fn new_conversation(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = &self.run {
            run.cancel.cancel();
        }
        self.run = None;
        self.entries.clear();
        self.wire.clear();
        self.conversation_id = None;
        self.usage_total = Usage::default();
        self.masked_last = 0;
        self.added.clear();
        self.mask.clear();
        self.history_open = false;
        self.overlay = Overlay::None;
        self.listed = 0;
        self.list.reset(0);
        self.follow = true;
        cx.notify();
    }

    pub fn save_as_note(&mut self, cx: &mut Context<Self>) {
        if self.wire.is_empty() {
            return;
        }
        let mut conversation = self.conversation();
        let link = self.linker();
        for message in conversation
            .messages
            .iter_mut()
            .filter(|m| m.role == Role::Assistant)
        {
            message.content = link(&message.content);
        }
        cx.emit(AiEvent::SaveConversationAsNote {
            title: conversation.title.clone(),
            markdown: history::to_markdown(&conversation),
        });
    }

    fn conversation(&self) -> history::Conversation {
        let title = self
            .wire
            .iter()
            .find(|m| m.role == Role::User)
            .map(|m| title_of(&strip_context(&m.content)))
            .unwrap_or_default();
        history::Conversation {
            id: self.conversation_id.clone().unwrap_or_default(),
            title,
            updated: now_secs(),
            model: self.model.clone(),
            messages: self.wire.clone(),
            usage: self.usage_total.clone(),
        }
    }

    fn save_conversation(&mut self, cx: &mut Context<Self>) {
        if !self.settings.keep_history {
            return;
        }
        let Some(dir) = self.services.history_dir.clone() else {
            return;
        };
        let id = self
            .conversation_id
            .get_or_insert_with(|| {
                format!(
                    "c{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis())
                )
            })
            .clone();
        let mut conversation = self.conversation();
        conversation.id = id;
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move { history::save(&dir, &conversation) })
                .await;
            if let Err(error) = saved {
                this.update(cx, |this, cx| {
                    this.notify(NoticeTone::Danger, error.to_string(), cx)
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn toggle_history(&mut self, cx: &mut Context<Self>) {
        self.history_open = !self.history_open;
        self.overlay = Overlay::None;
        if self.history_open {
            self.reload_history(cx);
        }
        cx.notify();
    }

    fn reload_history(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.services.history_dir.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let list = cx
                .background_spawn(async move { history::list(&dir) })
                .await;
            this.update(cx, |this, cx| {
                this.history = list;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn resume_nth(&mut self, n: usize, cx: &mut Context<Self>) {
        if let Some((id, _, _)) = self.history.get(n).cloned() {
            self.resume(id, cx);
        }
    }

    pub(crate) fn resume(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(dir) = self.services.history_dir.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move { history::load(&dir, &id) })
                .await;
            this.update(cx, |this, cx| match loaded {
                Ok(conversation) => this.open_conversation(conversation, cx),
                Err(error) => this.notify(NoticeTone::Danger, error.to_string(), cx),
            })
            .ok();
        })
        .detach();
    }

    fn open_conversation(&mut self, conversation: history::Conversation, cx: &mut Context<Self>) {
        self.new_conversation(cx);
        self.entries = rebuild_entries(&conversation.messages);
        self.sync_texts(cx);
        self.wire = conversation.messages;
        self.conversation_id = Some(conversation.id);
        self.usage_total = conversation.usage;
        self.model = conversation.model;
        self.listed = 0;
        self.list.reset(0);
        cx.notify();
    }

    pub(crate) fn delete_conversation(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(dir) = self.services.history_dir.clone() else {
            return;
        };
        let current = self.conversation_id.as_deref() == Some(id.as_str());
        self.history.retain(|(known, _, _)| *known != id);
        cx.spawn(async move |this, cx| {
            let target = id.clone();
            let deleted = cx
                .background_spawn(async move { history::delete(&dir, &target) })
                .await;
            if let Err(error) = deleted {
                this.update(cx, |this, cx| {
                    this.notify(NoticeTone::Danger, error.to_string(), cx)
                })
                .ok();
            }
        })
        .detach();
        if current {
            self.conversation_id = None;
        }
        cx.notify();
    }

    pub(crate) fn pick_model(&mut self, model: String, cx: &mut Context<Self>) {
        cx.emit(AiEvent::ModelChosen(model.clone()));
        self.model = model;
        self.overlay = Overlay::None;
        cx.notify();
    }
}

fn tools_with_servers(
    tools: Arc<dyn ToolHost + Send + Sync>,
    gate: &Arc<WriteGate>,
    servers: &[focus_ai::mcp::ServerConfig],
    sender: &smol::channel::Sender<Msg>,
) -> Box<dyn ToolHost> {
    struct Shared(Arc<dyn ToolHost + Send + Sync>);
    impl ToolHost for Shared {
        fn specs(&self) -> Vec<focus_ai::ToolSpec> {
            self.0.specs()
        }
        fn call(&self, call: &ToolCall, cancel: &Cancel) -> ToolOutcome {
            self.0.call(call, cancel)
        }
    }
    if servers.iter().all(|s| !s.enabled) {
        return Box::new(Shared(tools));
    }
    let resolved: Vec<focus_ai::mcp::ServerConfig> = servers
        .iter()
        .map(|s| crate::mcp_secrets::with_secrets(s, focus_ai::keystore::server_env::load))
        .collect();
    let (started, failed) = focus_ai::mcp::CompositeHost::<Shared>::start_servers(&resolved);
    for (name, error) in failed {
        let _ = sender.send_blocking(Msg::McpFailed(name, error.to_string()));
    }
    let confirm = gate.confirm();
    Box::new(focus_ai::mcp::CompositeHost::new(
        Shared(tools),
        started,
        move |request, cancel| {
            let label = format!("{}.{}", request.server, request.tool);
            confirm(
                &focus_mcp::WriteProposal {
                    tool: &label,
                    path: std::path::Path::new(request.server),
                    before: None,
                    after: request.arguments,
                },
                cancel,
            )
        },
    ))
}

fn flush_final(
    restorer: &mut StreamRestorer,
    mask: &Mask,
    make: fn(String) -> StreamEvent,
) -> Option<AgentEvent> {
    let rest = restorer.flush(mask);
    (!rest.is_empty()).then(|| AgentEvent::Stream(make(rest)))
}

fn restore_call(mask: &Mask, call: &ToolCall) -> ToolCall {
    ToolCall {
        id: call.id.clone(),
        name: call.name.clone(),
        arguments: mask.restore(&call.arguments),
    }
}

fn restore_outcome(mask: &Mask, outcome: ToolOutcome) -> ToolOutcome {
    match outcome {
        ToolOutcome::Ok(text) => ToolOutcome::Ok(mask.restore(&text)),
        ToolOutcome::Error(text) => ToolOutcome::Error(mask.restore(&text)),
        ToolOutcome::Denied => ToolOutcome::Denied,
    }
}

pub(crate) fn mention_at(text: &str, cursor: usize) -> Option<(usize, String)> {
    let cursor = cursor.min(text.len());
    if !text.is_char_boundary(cursor) {
        return None;
    }
    let before = &text[..cursor];
    let at = before.rfind('@')?;
    let query = &before[at + 1..];
    if query.chars().any(char::is_whitespace) {
        return None;
    }
    let opens = at == 0
        || before[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
    opens.then(|| (at, query.to_string()))
}

pub(crate) fn action_label(template: &Template) -> String {
    if template.builtin {
        t(&template.label)
    } else {
        template.label.clone()
    }
}

fn note_title(text: &str, fallback: &str) -> String {
    let heading = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("# ").map(str::trim));
    heading
        .filter(|h| !h.is_empty())
        .unwrap_or(fallback)
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect()
}

fn strip_context(content: &str) -> String {
    match content.rsplit_once("\n---\n\n") {
        Some((head, question)) if head.starts_with("Context provided by the user:") => {
            question.to_string()
        }
        _ => content.to_string(),
    }
}

fn title_of(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    title
}

fn rebuild_entries(messages: &[Message]) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        match message.role {
            Role::User => entries.push(Entry::User(UserTurn {
                text: strip_context(&message.content),
                chips: Vec::new(),
                masked: 0,
                wire_start: index,
                prompt: message.content.clone(),
                action: None,
            })),
            Role::Assistant => {
                if !matches!(entries.last(), Some(Entry::Assistant(_))) {
                    entries.push(Entry::Assistant(Assistant::default()));
                }
                if let Some(Entry::Assistant(a)) = entries.last_mut() {
                    if let Some(reasoning) = message.reasoning.as_deref().filter(|r| !r.is_empty())
                    {
                        a.parts.push(Part::Reasoning {
                            text: reasoning.to_string(),
                            open: false,
                            started: Instant::now(),
                            took: Some(Duration::ZERO),
                        });
                    }
                    for call in &message.tool_calls {
                        a.parts.push(Part::Tool(ToolLine {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                            started: Instant::now(),
                            took: None,
                            state: ToolState::Refused,
                            open: false,
                        }));
                    }
                    if !message.content.is_empty() {
                        a.push_content(&message.content);
                    }
                }
            }
            Role::Tool => {
                if let Some(Entry::Assistant(a)) = entries.last_mut() {
                    for part in &mut a.parts {
                        if let Part::Tool(line) = part
                            && Some(&line.id) == message.tool_call_id.as_ref()
                        {
                            line.state = ToolState::Done(message.content.clone());
                        }
                    }
                }
            }
            Role::System => {}
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.into(),
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    #[test]
    fn a_saved_conversation_comes_back_as_entries() {
        let mut calling = message(Role::Assistant, "");
        calling.reasoning = Some("think".into());
        calling.tool_calls = vec![ToolCall {
            id: "c1".into(),
            name: "read_note".into(),
            arguments: "{}".into(),
        }];
        let mut result = message(Role::Tool, "note text");
        result.tool_call_id = Some("c1".into());
        let messages = vec![
            message(
                Role::User,
                "Context provided by the user:\n\n### Note a.md\n\ntext\n\n---\n\nWhat is it?",
            ),
            calling,
            result,
            message(Role::Assistant, "It is a note."),
        ];
        let entries = rebuild_entries(&messages);
        assert_eq!(entries.len(), 2);
        let Entry::User(turn) = &entries[0] else {
            panic!("user entry expected")
        };
        assert_eq!((turn.text.as_str(), turn.wire_start), ("What is it?", 0));
        let Entry::Assistant(answer) = &entries[1] else {
            panic!("assistant entry expected")
        };
        assert!(matches!(answer.parts[0], Part::Reasoning { .. }));
        assert!(
            matches!(&answer.parts[1], Part::Tool(line) if matches!(&line.state, ToolState::Done(text) if text == "note text"))
        );
        assert_eq!(answer.answer(), "It is a note.");
        assert!(!answer.live);
    }

    #[test]
    fn conversations_survive_a_save_and_a_load() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("view-ai-history-{}", std::process::id()));
        let conversation = history::Conversation {
            id: "c1".into(),
            title: "t".into(),
            updated: 1,
            model: "m".into(),
            messages: vec![message(Role::User, "q"), message(Role::Assistant, "a")],
            usage: Usage::default(),
        };
        history::save(&dir, &conversation).unwrap();
        let loaded = history::load(&dir, "c1").unwrap();
        assert_eq!(rebuild_entries(&loaded.messages).len(), 2);
        history::delete(&dir, "c1").unwrap();
        assert!(history::list(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mention_opens_at_start_or_after_space() {
        assert_eq!(mention_at("@kerb", 5), Some((0, "kerb".into())));
        assert_eq!(mention_at("see @kerb", 9), Some((4, "kerb".into())));
        assert_eq!(mention_at("mail@kerb", 9), None);
        assert_eq!(mention_at("@kerb roasting", 14), None);
        assert_eq!(mention_at("plain", 5), None);
    }

    #[test]
    fn mention_works_with_multibyte_text() {
        assert_eq!(mention_at("é @né", "é @né".len()), Some((3, "né".into())));
    }

    #[test]
    fn iso_date_is_correct() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(1_794_441_600), "2026-11-12");
        assert_eq!(iso_date(951_782_400), "2000-02-29");
    }

    #[test]
    fn context_is_stripped_from_a_resumed_question() {
        let prompt = "Context provided by the user:\n\n### Note a.md\n\ntext\n\n---\n\nWhat is it?";
        assert_eq!(strip_context(prompt), "What is it?");
        assert_eq!(strip_context("plain"), "plain");
    }

    #[test]
    fn note_title_prefers_the_heading_and_drops_path_characters() {
        assert_eq!(
            note_title("intro\n# A/B: plan\nbody", "Fallback"),
            "AB plan"
        );
        assert_eq!(note_title("no heading", "Fallback"), "Fallback");
    }
}
