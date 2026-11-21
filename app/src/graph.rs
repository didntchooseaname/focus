use focus_core::NoteId;
use focus_view_graph::{GraphEvent, GraphSettings, GraphView, LocalGraph};
use gpui::{AppContext, Context, Window};

use crate::settings::schema::Value;
use crate::shell::{Pane, Shell};

const SLIDER_DEFAULT: f64 = 50.;
const DISTANCE_DEFAULT: f64 = 80.;
const GRAVITY_DEFAULT: f64 = 20.;

impl Shell {
    pub fn graph_settings(&self, cx: &gpui::App) -> GraphSettings {
        let prefs = self.prefs.read(cx);
        let base = GraphSettings::default();
        GraphSettings {
            tags: prefs.bool("graph.tags"),
            unresolved: prefs.bool("graph.unresolved"),
            attachments: prefs.bool("files.show_other"),
            orphans: prefs.bool("graph.orphans"),
            repulsion: base.repulsion * (prefs.number("graph.repulsion") / SLIDER_DEFAULT) as f32,
            link_distance: base.link_distance
                * (prefs.number("graph.distance") / DISTANCE_DEFAULT) as f32,
            gravity: base.gravity * (prefs.number("graph.gravity") / GRAVITY_DEFAULT) as f32,
            node_size: prefs.number("graph.node_size") as f32,
            labels: prefs.number("graph.labels") as f32,
            ..base
        }
    }

    fn store_graph_settings(&mut self, settings: &GraphSettings, cx: &mut Context<Self>) {
        let base = GraphSettings::default();
        let scaled =
            |value: f32, base: f32, default: f64| (f64::from(value / base) * default).round();
        let values = [
            ("graph.tags", Value::Bool(settings.tags)),
            ("graph.unresolved", Value::Bool(settings.unresolved)),
            ("graph.orphans", Value::Bool(settings.orphans)),
            (
                "graph.repulsion",
                Value::Number(
                    scaled(settings.repulsion, base.repulsion, SLIDER_DEFAULT).clamp(0., 100.),
                ),
            ),
            (
                "graph.distance",
                Value::Number(
                    scaled(settings.link_distance, base.link_distance, DISTANCE_DEFAULT)
                        .clamp(10., 300.),
                ),
            ),
            (
                "graph.gravity",
                Value::Number(
                    scaled(settings.gravity, base.gravity, GRAVITY_DEFAULT).clamp(0., 100.),
                ),
            ),
        ];
        self.graph_echo = true;
        for (key, value) in values {
            self.set_pref(key, value, cx);
        }
        self.graph_echo = false;
    }

    pub fn open_graph(
        &mut self,
        note: Option<NoteId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = note.or_else(|| self.last_note(cx));
        self.show_graph(active, window, cx);
    }

    pub fn open_vault_graph(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_graph(None, window, cx);
    }

    fn show_graph(&mut self, active: Option<NoteId>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let graph = match &self.graph {
            Some(graph) => graph.clone(),
            None => {
                let cache = self
                    .vault_root
                    .as_deref()
                    .map(focus_core::settings::cache_dir_for);
                let settings = self.graph_settings(cx);
                let graph = cx.new(|cx| {
                    GraphView::new(
                        library.vault().clone(),
                        library.index().clone(),
                        cache,
                        settings,
                        window,
                        cx,
                    )
                });
                self._subscriptions.push(cx.subscribe_in(
                    &graph,
                    window,
                    |this, _, event: &GraphEvent, window, cx| match event {
                        GraphEvent::OpenNote { note, new_tab } => {
                            this.open_note(*note, *new_tab, window, cx)
                        }
                        GraphEvent::SettingsChanged(settings) => {
                            this.store_graph_settings(settings, cx)
                        }
                        GraphEvent::Built => cx.notify(),
                    },
                ));
                let reduce = self.prefs.read(cx).bool("appearance.reduce_motion");
                graph.update(cx, |graph, cx| graph.set_reduce_motion(reduce, cx));
                self.graph = Some(graph.clone());
                self.spread_graph_relations(cx);
                graph
            }
        };
        graph.update(cx, |graph, cx| graph.set_active_note(active, cx));
        self.show_pane(Pane::Graph, window, cx);
    }

    pub fn graph_status(&self, cx: &gpui::App) -> Option<String> {
        if !matches!(
            self.active_pane.and_then(|i| self.panes.get(i)),
            Some(Pane::Graph)
        ) {
            return None;
        }
        let (notes, links) = self.graph.as_ref()?.read(cx).counts();
        Some(focus_i18n::t_args(
            "graph-stats",
            &[
                ("notes", focus_i18n::Arg::Int(notes as i64)),
                ("links", focus_i18n::Arg::Int(links as i64)),
            ],
        ))
    }

    pub fn mount_local_graph(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let graph = cx.new(|cx| LocalGraph::new(library.vault().clone(), 1, cx));
        self._subscriptions.push(cx.subscribe_in(
            &graph,
            window,
            |this, _, event: &GraphEvent, window, cx| {
                if let GraphEvent::OpenNote { note, new_tab } = event {
                    this.open_note(*note, *new_tab, window, cx);
                }
            },
        ));
        self.local_graph = Some(graph);
    }

    pub fn follow_local_graph(&mut self, cx: &mut Context<Self>) {
        let note = self.last_note(cx);
        if let Some(graph) = &self.local_graph {
            graph.update(cx, |graph, cx| graph.set_active_note(note, cx));
        }
    }

    pub fn graph_prefs_changed(&mut self, cx: &mut Context<Self>) {
        if self.graph_echo {
            return;
        }
        let settings = self.graph_settings(cx);
        if let Some(graph) = &self.graph {
            graph.update(cx, |graph, cx| graph.set_settings(settings, cx));
        }
    }

    pub fn graph_vault_event(&mut self, event: &focus_core::VaultEvent, cx: &mut Context<Self>) {
        if let Some(graph) = &self.graph {
            graph.update(cx, |graph, cx| graph.vault_changed(event, cx));
        }
        if let Some(graph) = &self.local_graph {
            graph.update(cx, |graph, cx| graph.vault_changed(event, cx));
        }
    }
}
