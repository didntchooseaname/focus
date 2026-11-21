use crate::controls::{self, Force};
use crate::core::{FrameTimes, GraphCore, Pending};
use crate::surface::{GraphHost, ensure_ticking, follow_frames, rebuild, surface};
use focus_core::{NoteId, Vault, VaultEvent};
use focus_graph::{Params, Spec};
use focus_kit::segments::{Segment, Segments};
use focus_kit::{ActiveTokens, Button, ButtonKind, Icon, IconButton, Type, ZoomControls, size};
use focus_search::SearchIndex;
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Subscription, Window, div, prelude::FluentBuilder,
    px,
};
use gpui_component::input::{InputEvent, InputState};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const REBUILD_DELAY: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq)]
pub enum GraphEvent {
    OpenNote { note: NoteId, new_tab: bool },
    SettingsChanged(GraphSettings),
    Built,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphSettings {
    pub tags: bool,
    pub unresolved: bool,
    pub attachments: bool,
    pub orphans: bool,
    pub hidden_folders: Vec<String>,
    pub repulsion: f32,
    pub link_distance: f32,
    pub gravity: f32,
    pub node_size: f32,
    pub labels: f32,
}

impl Default for GraphSettings {
    fn default() -> Self {
        let p = Params::default();
        let s = Spec::default();
        GraphSettings {
            tags: s.tags,
            unresolved: s.unresolved,
            attachments: s.attachments,
            orphans: s.orphans,
            hidden_folders: s.hidden_folders,
            repulsion: p.repulsion,
            link_distance: p.link_distance,
            gravity: p.gravity,
            node_size: 100.0,
            labels: DEFAULT_LABELS,
        }
    }
}

impl GraphSettings {
    fn spec(&self) -> Spec {
        Spec {
            tags: self.tags,
            unresolved: self.unresolved,
            attachments: self.attachments,
            orphans: self.orphans,
            hidden_folders: self.hidden_folders.clone(),
            ..Spec::default()
        }
    }
    fn label_scale(&self) -> f32 {
        (self.labels.max(0.0) / DEFAULT_LABELS).powi(2)
    }
    fn params(&self) -> Params {
        Params {
            repulsion: self.repulsion,
            link_distance: self.link_distance,
            gravity: self.gravity,
            ..Params::default()
        }
    }
}

const DEFAULT_LABELS: f32 = 120.0;
const SPEEDS: [(f32, &str); 4] = [
    (0.5, "graph-animate-speed-half"),
    (1.0, "graph-animate-speed-one"),
    (2.0, "graph-animate-speed-two"),
    (4.0, "graph-animate-speed-four"),
];

pub struct GraphView {
    pub(crate) core: GraphCore,
    pub(crate) settings: GraphSettings,
    pub(crate) panel_open: bool,
    pub(crate) query_input: Entity<InputState>,
    pub(crate) find_input: Entity<InputState>,
    pub(crate) find_missed: Option<String>,
    pub(crate) slider: Option<Force>,
    pub(crate) slider_bounds: controls::SliderBounds,
    language: u64,
    reduce_motion: bool,
    speed: f32,
    preparing: Option<gpui::Task<()>>,
    replay_date: Option<(i64, gpui::SharedString)>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<GraphEvent> for GraphView {}

impl Focusable for GraphView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl GraphView {
    pub fn new(
        vault: Arc<Vault>,
        search: Arc<SearchIndex>,
        cache_dir: Option<PathBuf>,
        settings: GraphSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(focus_i18n::t("graph-filter-query")));
        let find_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(focus_i18n::t("graph-find-node")));
        let subscriptions = vec![
            cx.subscribe(
                &query_input,
                |this: &mut GraphView, input, event: &InputEvent, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        let query = input.read(cx).value().to_string();
                        this.set_query(Some(query), cx);
                    }
                },
            ),
            cx.subscribe(
                &find_input,
                |this: &mut GraphView, input, event: &InputEvent, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        let query = input.read(cx).value().to_string();
                        this.find(&query, cx);
                    }
                },
            ),
        ];
        let mut core = GraphCore::new(
            vault,
            Some(search),
            cache_dir,
            settings.spec(),
            settings.params(),
        );
        core.node_scale = settings.node_size.clamp(10.0, 400.0) / 100.0;
        let mut view = GraphView {
            core,
            settings,
            panel_open: false,
            query_input,
            find_input,
            find_missed: None,
            slider: None,
            slider_bounds: controls::new_slider_bounds(),
            language: focus_i18n::generation(),
            reduce_motion: false,
            speed: 1.0,
            preparing: None,
            replay_date: None,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        rebuild(&mut view, cx);
        view
    }

    pub fn set_active_note(&mut self, note: Option<NoteId>, cx: &mut Context<Self>) {
        if self.core.active != note {
            self.core.active = note;
            cx.notify();
        }
    }

    pub fn vault_changed(&mut self, event: &VaultEvent, cx: &mut Context<Self>) {
        self.core.pending.push(match event {
            VaultEvent::NoteAdded(id)
            | VaultEvent::NoteChanged(id)
            | VaultEvent::NoteMoved { id, .. } => Pending::Changed(*id),
            VaultEvent::NoteRemoved { id, .. } => Pending::Removed(*id),
            VaultEvent::TreeChanged if self.settings.attachments => Pending::Everything,
            VaultEvent::TreeChanged => return,
        });
        self.core.build_generation += 1;
        let generation = self.core.build_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REBUILD_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if this.core.build_generation == generation {
                    rebuild(this, cx);
                }
            });
        })
        .detach();
    }

    pub fn settings(&self) -> &GraphSettings {
        &self.settings
    }

    pub fn counts(&self) -> (usize, usize) {
        (self.core.graph.nodes.len(), self.core.graph.edges.len())
    }

    pub fn is_idle(&self) -> bool {
        !self.core.building && self.core.is_settled() && !self.core.ticking
    }

    pub fn frame_times(&self) -> FrameTimes {
        self.core.frame.get()
    }

    pub fn set_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.panel_open = open;
        cx.notify();
    }

    pub fn hover_label(&mut self, label: &str, cx: &mut Context<Self>) -> bool {
        let found = self.core.hover_label(label);
        cx.notify();
        found
    }

    pub fn zoom(&mut self, factor: Option<f32>, cx: &mut Context<Self>) {
        match factor {
            Some(f) => self.core.zoom_by(f),
            None => self.core.fit(),
        }
        cx.notify();
    }

    pub fn set_query(&mut self, query: Option<String>, cx: &mut Context<Self>) {
        self.core.query = query.filter(|q| !q.trim().is_empty());
        self.core.refit_on_install = true;
        rebuild(self, cx);
    }

    pub fn set_relations(&mut self, relations: Vec<(PathBuf, PathBuf)>, cx: &mut Context<Self>) {
        if relations != self.core.spec.relations {
            self.core.spec.relations = relations;
            rebuild(self, cx);
        }
    }

    pub fn set_settings(&mut self, settings: GraphSettings, cx: &mut Context<Self>) {
        let mut spec = settings.spec();
        spec.relations = self.core.spec.relations.clone();
        let structural = spec != self.core.spec;
        let params = settings.params();
        self.core.node_scale = settings.node_size.clamp(10.0, 400.0) / 100.0;
        self.settings = settings.clone();
        self.core.refit_on_install |= settings.hidden_folders != self.core.spec.hidden_folders;
        self.core.spec = spec;
        if structural {
            self.core.params = params;
            rebuild(self, cx);
        } else {
            self.core.reheat(params);
            ensure_ticking(self, cx);
        }
        cx.emit(GraphEvent::SettingsChanged(settings));
        cx.notify();
    }

    pub fn set_reduce_motion(&mut self, reduce: bool, cx: &mut Context<Self>) {
        self.reduce_motion = reduce;
        if reduce {
            self.end_animation(cx);
        }
        cx.notify();
    }

    pub fn animate(&mut self, cx: &mut Context<Self>) {
        if self.reduce_motion || self.core.building || self.core.graph.nodes.is_empty() {
            return;
        }
        let (vault, graph) = (self.core.vault.clone(), self.core.graph.clone());
        let task = cx.background_spawn(async move {
            let (order, times) = crate::replay::birth_order(&vault, &graph);
            (order, times, crate::replay::Links::of(&graph))
        });
        self.preparing = Some(cx.spawn(async move |this, cx| {
            let (order, times, links) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.preparing = None;
                this.replay_date = None;
                this.core.start_replay(order, times, links, this.speed);
                ensure_ticking(this, cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub fn set_animation_playing(&mut self, playing: bool, cx: &mut Context<Self>) {
        self.core.set_replay_playing(playing);
        if playing {
            ensure_ticking(self, cx);
        }
        cx.notify();
    }

    pub fn set_animation_speed(&mut self, speed: f32, cx: &mut Context<Self>) {
        self.speed = speed;
        if let Some(replay) = &mut self.core.replay {
            replay.speed = speed;
        }
        cx.notify();
    }

    pub fn end_animation(&mut self, cx: &mut Context<Self>) {
        self.preparing = None;
        self.core.end_replay();
        ensure_ticking(self, cx);
        cx.notify();
    }

    pub fn animation_progress(&self) -> Option<f32> {
        self.core.replay.as_ref().map(|r| r.progress)
    }

    pub fn set_motion_probe(&mut self, on: bool) {
        *self.core.probe.borrow_mut() = on.then(|| crate::probe::Probe::new(&self.core.graph));
    }

    pub fn spread_figures(&self) -> Option<crate::probe::SpreadFigures> {
        self.core
            .probe
            .borrow()
            .as_ref()
            .map(crate::probe::Probe::end)
    }

    pub fn motion_report(&self) -> Option<crate::probe::MotionReport> {
        self.core
            .probe
            .borrow()
            .as_ref()
            .map(crate::probe::Probe::report)
    }

    pub fn frames_painted(&self) -> u64 {
        self.core.painted.get()
    }

    pub fn find(&mut self, query: &str, cx: &mut Context<Self>) -> bool {
        match self.core.find(query) {
            Some(node) => {
                self.core.center_on(node);
                self.find_missed = None;
                cx.notify();
                true
            }
            None => {
                self.find_missed = Some(query.to_string());
                cx.notify();
                false
            }
        }
    }
}

pub(crate) fn at_top_start(el: gpui::Div) -> gpui::Div {
    match focus_i18n::direction() {
        focus_i18n::Direction::Ltr => el.top(size::S3).left(size::S3),
        focus_i18n::Direction::Rtl => el.top(size::S3).right(size::S3),
    }
}

pub(crate) fn at_bottom_end(el: gpui::Div) -> gpui::Div {
    match focus_i18n::direction() {
        focus_i18n::Direction::Ltr => el.bottom(size::S3).right(size::S3),
        focus_i18n::Direction::Rtl => el.bottom(size::S3).left(size::S3),
    }
}

pub(crate) fn at_bottom_start(el: gpui::Div) -> gpui::Div {
    match focus_i18n::direction() {
        focus_i18n::Direction::Ltr => el.bottom(size::S3).left(size::S3),
        focus_i18n::Direction::Rtl => el.bottom(size::S3).right(size::S3),
    }
}

impl GraphView {
    fn animation_bar(
        &mut self,
        t: &focus_kit::Tokens,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if self.reduce_motion {
            return None;
        }
        let bar = || {
            div()
                .occlude()
                .flex()
                .items_center()
                .gap(size::S2)
                .px(size::S2)
                .py(size::S1)
                .bg(t.card)
                .border_1()
                .border_color(t.float_border)
        };
        let Some(replay) = &self.core.replay else {
            let label = if self.preparing.is_some() {
                "graph-animate-preparing"
            } else {
                "graph-animate"
            };
            return Some(bottom_center(
                bar().child(
                    Button::new("graph-animate", focus_i18n::t(label))
                        .kind(ButtonKind::Ghost)
                        .icon(Icon::HISTORY)
                        .on_click(cx.listener(|this, _, _, cx| this.animate(cx))),
                ),
            ));
        };
        let playing = replay.playing;
        let day = replay.date().map(|t| t.div_euclid(86_400));
        if day != self.replay_date.as_ref().map(|(d, _)| *d) {
            self.replay_date = replay
                .date()
                .map(|t| (t.div_euclid(86_400), focus_i18n::format_date(t).into()));
        }
        let speed = SPEEDS
            .iter()
            .position(|(s, _)| *s == self.speed)
            .unwrap_or(1);
        let view = cx.entity().downgrade();
        let speeds = Segments::new(
            "graph-animate-speed",
            SPEEDS
                .iter()
                .map(|(_, key)| Segment::new(focus_i18n::t(key)))
                .collect(),
            speed,
        )
        .on_select(move |choice, _, cx| {
            let _ = view.update(cx, |this, cx| {
                this.set_animation_speed(SPEEDS[choice.min(SPEEDS.len() - 1)].0, cx)
            });
        });
        Some(bottom_center(
            bar()
                .child(
                    Button::new(
                        "graph-animate-play",
                        focus_i18n::t(if playing {
                            "graph-animate-pause"
                        } else {
                            "graph-animate-play"
                        }),
                    )
                    .kind(ButtonKind::Ghost)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.set_animation_playing(!playing, cx)),
                    ),
                )
                .child(speeds)
                .child(
                    div()
                        .mono_sm()
                        .text_color(t.ink_3)
                        .min_w(px(96.))
                        .children(self.replay_date.as_ref().map(|(_, date)| date.clone())),
                )
                .child(
                    Button::new("graph-animate-end", focus_i18n::t("graph-animate-end"))
                        .kind(ButtonKind::Ghost)
                        .on_click(cx.listener(|this, _, _, cx| this.end_animation(cx))),
                ),
        ))
    }
}

fn bottom_center(bar: gpui::Div) -> gpui::AnyElement {
    div()
        .absolute()
        .bottom(size::S3)
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(bar)
        .into_any_element()
}

impl GraphHost for GraphView {
    fn core(&self) -> &GraphCore {
        &self.core
    }
    fn core_mut(&mut self) -> &mut GraphCore {
        &mut self.core
    }
    fn focus(&self) -> &FocusHandle {
        &self.focus
    }
    fn pointer_moved(&mut self, pointer: [f32; 2], cx: &mut Context<Self>) -> bool {
        match self.slider {
            Some(force) => {
                controls::drag_slider(self, force, pointer, cx);
                true
            }
            None => false,
        }
    }
    fn pointer_released(&mut self) -> bool {
        self.slider.take().is_some()
    }
}

impl Render for GraphView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.language != focus_i18n::generation() {
            self.language = focus_i18n::generation();
            self.query_input.update(cx, |s, cx| {
                s.set_placeholder(focus_i18n::t("graph-filter-query"), window, cx)
            });
            self.find_input.update(cx, |s, cx| {
                s.set_placeholder(focus_i18n::t("graph-find-node"), window, cx)
            });
        }
        follow_frames(self, window, cx);
        let t = *cx.tokens();
        let empty = !self.core.building && self.core.graph.nodes.is_empty();
        let settling =
            (self.core.building || !self.core.is_settled()) && self.core.replay.is_none();
        let animation = self.animation_bar(&t, cx);
        let zoom = focus_i18n::t_args(
            "graph-zoom-level",
            &[("percent", focus_i18n::Arg::Int(self.core.zoom_percent()))],
        );
        surface(self, "graph-view", self.settings.label_scale(), cx)
            .when(empty, |el| {
                el.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .ui()
                                .text_color(t.ink_3)
                                .child(focus_i18n::t("graph-empty")),
                        ),
                )
            })
            .when(settling && !empty, |el| {
                el.child(
                    at_bottom_start(div().absolute())
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(focus_i18n::t("graph-settling")),
                )
            })
            .when(!empty, |el| el.children(animation))
            .child(
                at_top_start(div().absolute())
                    .occlude()
                    .child(controls::panel(self, window, cx)),
            )
            .child(
                at_bottom_end(div().absolute())
                    .occlude()
                    .child(ZoomControls::new(
                        zoom,
                        IconButton::new(
                            "graph-zoom-out",
                            Icon::MINUS,
                            focus_i18n::t("graph-zoom-out"),
                        )
                        .shortcut(focus_i18n::keys_text(crate::surface::ZOOM_OUT_KEY))
                        .on_click(cx.listener(|this, _, _, cx| this.zoom(Some(0.8), cx))),
                        IconButton::new(
                            "graph-zoom-in",
                            Icon::PLUS,
                            focus_i18n::t("graph-zoom-in"),
                        )
                        .shortcut(focus_i18n::keys_text(crate::surface::ZOOM_IN_KEY))
                        .on_click(cx.listener(|this, _, _, cx| this.zoom(Some(1.25), cx))),
                        IconButton::new(
                            "graph-zoom-fit",
                            Icon::MAXIMIZE,
                            focus_i18n::t("graph-zoom-fit"),
                        )
                        .shortcut(focus_i18n::keys_text(crate::surface::FIT_KEY))
                        .on_click(cx.listener(|this, _, _, cx| this.zoom(None, cx))),
                    )),
            )
    }
}
