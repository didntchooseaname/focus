use anyhow::{Context as _, bail};
use focus_core::{OpenOptions, Vault};
use focus_kit::{ActiveTokens, Mode};
use focus_search::SearchIndex;
use focus_view_graph::{GraphEvent, GraphSettings, GraphView, LocalGraph};
use gpui::{
    AppContext, Application, AsyncWindowContext, Bounds, Context, Entity, Focusable, IntoElement,
    Keystroke, ParentElement, Render, Styled, TitlebarOptions, WeakEntity, Window, WindowBounds,
    WindowOptions, div, prelude::FluentBuilder, px, size,
};
use gpui_component::Root;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const FRAME: Duration = Duration::from_millis(20);

#[derive(Debug, Clone)]
enum Step {
    Hover(String),
    Zoom(Option<f32>),
    Find(String),
    Controls(bool),
    Filter(String),
    Tags(bool),
    Unresolved(bool),
    NodeSize(f32),
    Labels(f32),
    Folder(String, bool),
    Theme(bool),
    Lang(String),
    Active(String),
    Local(String),
    Measure(String),
    Animate(Option<&'static str>),
    Speed(f32),
    Reduce(bool),
    Hidden(bool),
    MeasureReplay(String, f32),
    MeasureMotion(String, f32),
    MeasureSpread(String),
    Snap(String),
    Progress,
    Key(String),
    Type(String),
    Wait(u64),
    Shot(String),
    Quit,
}

fn on_off(rest: &str) -> Result<bool, String> {
    match rest {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err("expected on or off".into()),
    }
}

fn parse(text: &str) -> Result<Vec<(usize, Step)>, String> {
    let mut steps = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(w, r)| (w, r.trim()));
        let fail = |m: &str| format!("line {}: {m}", i + 1);
        let step = match word {
            "hover" => Step::Hover(rest.to_string()),
            "zoom" if rest == "fit" => Step::Zoom(None),
            "zoom" => Step::Zoom(Some(
                rest.parse().map_err(|_| fail("expected a factor or fit"))?,
            )),
            "find" => Step::Find(rest.to_string()),
            "controls" => Step::Controls(rest == "open"),
            "filter" => Step::Filter(rest.to_string()),
            "tags" => Step::Tags(on_off(rest).map_err(|e| fail(&e))?),
            "unresolved" => Step::Unresolved(on_off(rest).map_err(|e| fail(&e))?),
            "nodesize" => Step::NodeSize(rest.parse().map_err(|_| fail("expected a percentage"))?),
            "labels" => Step::Labels(rest.parse().map_err(|_| fail("expected a percentage"))?),
            "folder" => {
                let (name, state) = rest
                    .rsplit_once(' ')
                    .ok_or_else(|| fail("expected a folder and on or off"))?;
                Step::Folder(name.to_string(), on_off(state).map_err(|e| fail(&e))?)
            }
            "theme" => Step::Theme(match rest {
                "dark" => true,
                "light" => false,
                _ => return Err(fail("expected dark or light")),
            }),
            "lang" => Step::Lang(rest.to_string()),
            "active" => Step::Active(rest.to_string()),
            "local" => Step::Local(rest.to_string()),
            "measure" => Step::Measure(rest.to_string()),
            "animate" => Step::Animate(match rest {
                "" => None,
                "play" => Some("play"),
                "pause" => Some("pause"),
                "end" => Some("end"),
                _ => return Err(fail("expected nothing, play, pause or end")),
            }),
            "speed" => Step::Speed(rest.parse().map_err(|_| fail("expected a factor"))?),
            "reduce" => Step::Reduce(on_off(rest).map_err(|e| fail(&e))?),
            "hidden" => Step::Hidden(on_off(rest).map_err(|e| fail(&e))?),
            "measure-replay" => {
                let (name, seconds) = rest
                    .rsplit_once(' ')
                    .ok_or_else(|| fail("expected a name and seconds"))?;
                Step::MeasureReplay(
                    name.to_string(),
                    seconds.parse().map_err(|_| fail("expected seconds"))?,
                )
            }
            "measure-motion" => {
                let (name, seconds) = rest
                    .rsplit_once(' ')
                    .ok_or_else(|| fail("expected a name and seconds"))?;
                Step::MeasureMotion(
                    name.to_string(),
                    seconds.parse().map_err(|_| fail("expected seconds"))?,
                )
            }
            "measure-spread" => Step::MeasureSpread(rest.to_string()),
            "snap" => Step::Snap(rest.to_string()),
            "progress" => Step::Progress,
            "key" => Step::Key(rest.to_string()),
            "type" => Step::Type(rest.to_string()),
            "wait" => Step::Wait(rest.parse().map_err(|_| fail("expected milliseconds"))?),
            "shot" => Step::Shot(rest.to_string()),
            "quit" => Step::Quit,
            other => return Err(fail(&format!("unknown instruction: {other}"))),
        };
        steps.push((i + 1, step));
    }
    Ok(steps)
}

struct Demo {
    vault: Option<Arc<Vault>>,
    graph: Option<Entity<GraphView>>,
    local: Option<Entity<LocalGraph>>,
    loading: bool,
    error: Option<String>,
    hidden: bool,
}

impl Demo {
    fn new(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/demo-cache")
            .join(root.file_name().unwrap_or_default());
        let task_cache = cache.clone();
        let task = cx.background_spawn(async move {
            let vault = Vault::open(
                &root,
                OpenOptions {
                    cache_dir: task_cache.clone(),
                    ignore: Vec::new(),
                    watch: false,
                },
            )?;
            let index = SearchIndex::open(vault.clone(), &task_cache, &|_| {})?;
            anyhow::Ok((vault, index))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match result {
                    Ok((vault, index)) => {
                        let graph = cx.new(|cx| {
                            GraphView::new(
                                vault.clone(),
                                index,
                                Some(cache),
                                GraphSettings::default(),
                                window,
                                cx,
                            )
                        });
                        window.focus(&graph.focus_handle(cx));
                        cx.subscribe(&graph, |_, _, event: &GraphEvent, _| {
                            println!("event: {event:?}")
                        })
                        .detach();
                        this.graph = Some(graph);
                        this.vault = Some(vault);
                    }
                    Err(error) => this.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        Demo {
            vault: None,
            graph: None,
            local: None,
            loading: true,
            error: None,
            hidden: false,
        }
    }

    fn idle(&self, cx: &gpui::App) -> bool {
        !self.loading
            && self.graph.as_ref().is_none_or(|g| g.read(cx).is_idle())
            && self.local.as_ref().is_none_or(|l| l.read(cx).is_idle())
    }

    fn note(&self, path: &str) -> Result<focus_core::NoteId, String> {
        self.vault
            .as_ref()
            .and_then(|v| v.note_by_path(Path::new(path)))
            .ok_or_else(|| format!("note not found: {path}"))
    }

    fn apply(
        &mut self,
        step: &Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let graph = self.graph.clone().ok_or("vault not loaded")?;
        match step {
            Step::Hover(label) => {
                if !graph.update(cx, |g, cx| g.hover_label(label, cx)) && !label.is_empty() {
                    return Err(format!("no node labelled {label}"));
                }
            }
            Step::Zoom(f) => graph.update(cx, |g, cx| g.zoom(*f, cx)),
            Step::Find(label) => {
                if !graph.update(cx, |g, cx| g.find(label, cx)) {
                    return Err(format!("no node labelled {label}"));
                }
            }
            Step::Controls(open) => graph.update(cx, |g, cx| g.set_panel_open(*open, cx)),
            Step::Filter(query) => graph.update(cx, |g, cx| g.set_query(Some(query.clone()), cx)),
            Step::Tags(on) => graph.update(cx, |g, cx| {
                let s = GraphSettings {
                    tags: *on,
                    ..g.settings().clone()
                };
                g.set_settings(s, cx)
            }),
            Step::NodeSize(percent) => graph.update(cx, |g, cx| {
                let s = GraphSettings {
                    node_size: *percent,
                    ..g.settings().clone()
                };
                g.set_settings(s, cx)
            }),
            Step::Labels(percent) => graph.update(cx, |g, cx| {
                let s = GraphSettings {
                    labels: *percent,
                    ..g.settings().clone()
                };
                g.set_settings(s, cx)
            }),
            Step::Unresolved(on) => graph.update(cx, |g, cx| {
                let s = GraphSettings {
                    unresolved: *on,
                    ..g.settings().clone()
                };
                g.set_settings(s, cx)
            }),
            Step::Folder(name, on) => graph.update(cx, |g, cx| {
                let mut s = g.settings().clone();
                s.hidden_folders.retain(|f| f != name);
                if !*on {
                    s.hidden_folders.push(name.clone());
                }
                g.set_settings(s, cx)
            }),
            Step::Theme(dark) => {
                focus_kit::theme::apply(
                    if *dark { Mode::Dark } else { Mode::Light },
                    Some(window),
                    cx,
                )
                .map_err(|e| e.to_string())?;
                cx.refresh_windows();
            }
            Step::Lang(code) => {
                focus_i18n::set_language(code).map_err(|e| e.to_string())?;
                cx.refresh_windows();
            }
            Step::Active(path) => {
                let id = self.note(path)?;
                graph.update(cx, |g, cx| g.set_active_note(Some(id), cx));
            }
            Step::Local(path) => {
                let id = self.note(path)?;
                let vault = self.vault.clone().ok_or("vault not loaded")?;
                let local = self
                    .local
                    .get_or_insert_with(|| cx.new(|cx| LocalGraph::new(vault, 1, cx)))
                    .clone();
                local.update(cx, |l, cx| l.set_active_note(Some(id), cx));
            }
            Step::Animate(action) => graph.update(cx, |g, cx| match action {
                None => g.animate(cx),
                Some("play") => g.set_animation_playing(true, cx),
                Some("pause") => g.set_animation_playing(false, cx),
                _ => g.end_animation(cx),
            }),
            Step::Speed(factor) => graph.update(cx, |g, cx| g.set_animation_speed(*factor, cx)),
            Step::Reduce(on) => graph.update(cx, |g, cx| g.set_reduce_motion(*on, cx)),
            Step::Hidden(on) => self.hidden = *on,
            Step::Progress => println!("progress {:?}", graph.read(cx).animation_progress()),
            Step::MeasureReplay(..)
            | Step::MeasureMotion(..)
            | Step::MeasureSpread(_)
            | Step::Snap(_)
            | Step::Measure(_)
            | Step::Key(_)
            | Step::Type(_)
            | Step::Wait(_)
            | Step::Shot(_)
            | Step::Quit => {}
        }
        cx.notify();
        Ok(())
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        div()
            .size_full()
            .flex()
            .bg(t.bg)
            .text_color(t.ink)
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .when_some(self.graph.clone().filter(|_| !self.hidden), |el, g| {
                        el.child(g)
                    })
                    .when_some(self.error.clone(), |el, e| el.child(e)),
            )
            .when_some(self.local.clone(), |el, local| {
                el.child(
                    div()
                        .w(focus_kit::size::RIGHT)
                        .h_full()
                        .border_l_1()
                        .border_color(t.line)
                        .child(local),
                )
            })
    }
}

async fn settle(demo: &WeakEntity<Demo>, cx: &mut AsyncWindowContext) {
    for _ in 0..3000 {
        if demo.read_with(cx, |d, cx| d.idle(cx)).unwrap_or(true) {
            return;
        }
        cx.background_executor().timer(FRAME).await;
    }
}

async fn measure(demo: &WeakEntity<Demo>, name: &str, cx: &mut AsyncWindowContext) {
    let mut samples = Vec::new();
    for _ in 0..12 {
        let _ = cx.update(|window, _| window.refresh());
        cx.background_executor().timer(FRAME * 2).await;
        if let Ok(Some(f)) = demo.read_with(cx, |d, cx| {
            d.graph.as_ref().map(|g| g.read(cx).frame_times())
        }) {
            samples.push(f);
        }
    }
    let steady = &samples[samples.len().min(4)..];
    let mut totals: Vec<f32> = steady.iter().map(|f| f.plan_ms + f.paint_ms).collect();
    totals.sort_by(f32::total_cmp);
    if let (Some(f), Some(median)) = (steady.last(), totals.get(totals.len() / 2)) {
        println!(
            "measure {name}: median {median:.2} ms per frame (plan {:.2} ms, paint {:.2} ms), {} nodes, {} edges, {} labels, {} frames",
            f.plan_ms,
            f.paint_ms,
            f.nodes,
            f.edges,
            f.labels,
            totals.len()
        );
    }
}

async fn measure_replay(
    demo: &WeakEntity<Demo>,
    name: &str,
    seconds: f32,
    cx: &mut AsyncWindowContext,
) {
    let read = |cx: &mut AsyncWindowContext| {
        demo.read_with(cx, |d, cx| {
            d.graph.as_ref().map(|g| {
                (
                    g.read(cx).frame_times(),
                    g.read(cx).frames_painted(),
                    g.read(cx).animation_progress(),
                )
            })
        })
        .ok()
        .flatten()
    };
    let Some((_, first, start_progress)) = read(cx) else {
        return;
    };
    let started = std::time::Instant::now();
    let (mut totals, mut plans, mut paints) = (Vec::new(), Vec::new(), Vec::new());
    while started.elapsed().as_secs_f32() < seconds {
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
        if let Some((f, _, _)) = read(cx) {
            totals.push(f.plan_ms + f.paint_ms);
            plans.push(f.plan_ms);
            paints.push(f.paint_ms);
        }
    }
    let elapsed = started.elapsed().as_secs_f32();
    let Some((last, frames, progress)) = read(cx) else {
        return;
    };
    for series in [&mut totals, &mut plans, &mut paints] {
        series.sort_by(f32::total_cmp);
    }
    let quantile = |series: &[f32], q: f32| {
        series
            .get(((series.len() as f32 - 1.0) * q).round() as usize)
            .copied()
            .unwrap_or(0.0)
    };
    let at = |q: f32| quantile(&totals, q);
    println!(
        "measure-replay {name}: {:.1} frames/s over {elapsed:.2} s, frame cost median {:.2} ms, p95 {:.2} ms, max {:.2} ms (plan median {:.2} ms, paint median {:.2} ms), last frame {} nodes {} edges, progress {:?} to {:?}",
        (frames - first) as f32 / elapsed,
        at(0.5),
        at(0.95),
        at(1.0),
        quantile(&plans, 0.5),
        quantile(&paints, 0.5),
        last.nodes,
        last.edges,
        start_progress,
        progress
    );
}

async fn measure_motion(
    demo: &WeakEntity<Demo>,
    name: &str,
    seconds: f32,
    cx: &mut AsyncWindowContext,
) {
    let Some(graph) = demo.read_with(cx, |d, _| d.graph.clone()).ok().flatten() else {
        return;
    };
    let _ = cx.update(|_, cx| graph.update(cx, |g, _| g.set_motion_probe(true)));
    let started = std::time::Instant::now();
    let mut seen = false;
    while started.elapsed().as_secs_f32() < seconds {
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
        let (playing, idle) = cx
            .update(|_, cx| {
                let g = graph.read(cx);
                (g.animation_progress().is_some(), g.is_idle())
            })
            .unwrap_or((false, true));
        if seen && !playing && idle {
            break;
        }
        seen |= playing;
    }
    let report = cx
        .update(|_, cx| {
            graph.update(cx, |g, _| {
                let report = g.motion_report();
                g.set_motion_probe(false);
                report
            })
        })
        .ok()
        .flatten();
    let Some(r) = report else { return };
    let s = |s: focus_view_graph::Spread| format!("{:.2} {:.2} {:.2}", s.median, s.p95, s.max);
    let f = |f: focus_view_graph::SpreadFigures| {
        format!(
            "{} {:.2} {:.2} {:.2}",
            f.nodes, f.overlap_pct, f.link_ratio, f.fill_pct
        )
    };
    println!(
        "measure-links {name}: links_born {}; links_grown {:.3}; link_frames {:.3}; bright_s {}",
        r.links_born,
        r.links_grown,
        r.link_frames,
        s(r.bright_s)
    );
    println!(
        "measure-layout {name}: overlap_pct {}; link_ratio {}; fill_pct {}; end {}",
        s(r.spread.overlap_pct),
        s(r.spread.link_ratio),
        s(r.spread.fill_pct),
        f(r.end)
    );
    println!(
        "measure-rhythm {name}: births_frame_max {}; births_second_max {}; placed_px_max {:.2}; pan_px_max {:.2}; zoom_pct_max {:.2}; zoom_turns {}",
        r.births_per_frame,
        r.births_per_second,
        r.placed_px.max,
        r.pan_px.max,
        r.zoom_pct.max,
        r.zoom_turns
    );
    println!(
        "measure-motion {name}: frames {} over {:.2} s; interval_ms {}; node_px {}; placed_px {}; jerk_px {}; placed_jerk_px {}; frame_max_px {}; pan_px {}; zoom_pct {}; births {} unlinked {} per_frame_max {}; birth_gap_px {}; birth_gap_link {}; birth_scale {}; travel_px {}; worst move frame {} age {} zoom frame {}",
        r.frames,
        started.elapsed().as_secs_f32(),
        s(r.interval_ms),
        s(r.node_px),
        s(r.placed_px),
        s(r.jerk_px),
        s(r.placed_jerk_px),
        s(r.frame_max_px),
        s(r.pan_px),
        s(r.zoom_pct),
        r.births,
        r.births_unlinked,
        r.births_per_frame,
        s(r.birth_gap_px),
        s(r.birth_gap_link),
        s(r.birth_scale),
        s(r.travel_px),
        r.worst_move_frame,
        r.worst_move_age,
        r.worst_zoom_frame,
    );
}

async fn measure_spread(demo: &WeakEntity<Demo>, name: &str, cx: &mut AsyncWindowContext) {
    let Some(graph) = demo.read_with(cx, |d, _| d.graph.clone()).ok().flatten() else {
        return;
    };
    let _ = cx.update(|_, cx| graph.update(cx, |g, _| g.set_motion_probe(true)));
    for _ in 0..3 {
        let _ = cx.update(|window, _| window.refresh());
        cx.background_executor().timer(FRAME * 2).await;
    }
    let figures = cx
        .update(|_, cx| {
            graph.update(cx, |g, _| {
                let figures = g.spread_figures();
                g.set_motion_probe(false);
                figures
            })
        })
        .ok()
        .flatten();
    if let Some(f) = figures {
        println!(
            "measure-spread {name}: nodes {}; overlap_pct {:.2}; link_ratio {:.2}; fill_pct {:.2}",
            f.nodes, f.overlap_pct, f.link_ratio, f.fill_pct
        );
    }
}

async fn snap(name: &str, cx: &mut AsyncWindowContext) -> Result<(), String> {
    let command =
        std::env::var("FOCUS_SHOT_CMD").map_err(|_| "FOCUS_SHOT_CMD missing".to_string())?;
    let _ = cx.update(|window, _| window.refresh());
    cx.background_executor().timer(FRAME * 2).await;
    let name = name.to_string();
    let status = cx
        .background_spawn(async move { std::process::Command::new(&command).arg(&name).status() })
        .await
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("FOCUS_SHOT_CMD failed ({status})"))
    }
}

async fn shot(
    demo: &WeakEntity<Demo>,
    name: &str,
    cx: &mut AsyncWindowContext,
) -> Result<(), String> {
    let command =
        std::env::var("FOCUS_SHOT_CMD").map_err(|_| "FOCUS_SHOT_CMD missing".to_string())?;
    settle(demo, cx).await;
    for _ in 0..3 {
        let _ = cx.update(|window, _| window.refresh());
        cx.background_executor().timer(FRAME * 2).await;
    }
    let name = name.to_string();
    let status = cx
        .background_spawn(async move { std::process::Command::new(&command).arg(&name).status() })
        .await
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("FOCUS_SHOT_CMD failed ({status})"))
    }
}

async fn run(demo: WeakEntity<Demo>, steps: Vec<(usize, Step)>, cx: &mut AsyncWindowContext) {
    for (line, step) in steps {
        let outcome = match &step {
            Step::Quit => break,
            Step::Wait(ms) => {
                cx.background_executor()
                    .timer(Duration::from_millis(*ms))
                    .await;
                Ok(())
            }
            Step::Key(keys) => match Keystroke::parse(keys) {
                Ok(k) => cx
                    .update(|window, cx| {
                        window.dispatch_keystroke(k, cx);
                    })
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            },
            Step::Type(text) => cx
                .update(|window, cx| {
                    for ch in text.chars() {
                        let key = if ch == ' ' {
                            "space".to_string()
                        } else {
                            ch.to_string()
                        };
                        window.dispatch_keystroke(
                            Keystroke {
                                modifiers: Default::default(),
                                key,
                                key_char: Some(ch.to_string()),
                            },
                            cx,
                        );
                    }
                })
                .map_err(|e| e.to_string()),
            Step::Shot(name) => shot(&demo, name, cx).await,
            Step::Measure(name) => {
                settle(&demo, cx).await;
                measure(&demo, name, cx).await;
                Ok(())
            }
            Step::MeasureReplay(name, seconds) => {
                measure_replay(&demo, name, *seconds, cx).await;
                Ok(())
            }
            Step::MeasureMotion(name, seconds) => {
                measure_motion(&demo, name, *seconds, cx).await;
                Ok(())
            }
            Step::MeasureSpread(name) => {
                measure_spread(&demo, name, cx).await;
                Ok(())
            }
            Step::Snap(name) => snap(name, cx).await,
            Step::Animate(_)
            | Step::Speed(_)
            | Step::Reduce(_)
            | Step::Hidden(_)
            | Step::Progress => {
                match demo.update_in(cx, |d, window, cx| d.apply(&step, window, cx)) {
                    Ok(r) => r,
                    Err(e) => Err(e.to_string()),
                }
            }
            _ => {
                settle(&demo, cx).await;
                match demo.update_in(cx, |d, window, cx| d.apply(&step, window, cx)) {
                    Ok(r) => r,
                    Err(e) => Err(e.to_string()),
                }
            }
        };
        if let Err(message) = outcome {
            eprintln!("demo: line {line}: {message}");
        }
        cx.background_executor().timer(FRAME).await;
    }
    let _ = cx.update(|_, cx| cx.quit());
}

fn main() -> anyhow::Result<()> {
    let mut vault = None;
    let mut script = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vault" => {
                vault = Some(PathBuf::from(
                    args.next().context("--vault expects a path")?,
                ))
            }
            "--script" => {
                script = Some(PathBuf::from(
                    args.next().context("--script expects a file")?,
                ))
            }
            other => bail!("unknown argument: {other} (expected --vault <path>, --script <file>)"),
        }
    }
    let vault = vault.context("--vault is required")?;
    let steps = match &script {
        Some(path) => Some(
            parse(
                &std::fs::read_to_string(path)
                    .with_context(|| format!("reading {}", path.display()))?,
            )
            .map_err(anyhow::Error::msg)?,
        ),
        None => None,
    };
    let scripted = steps.is_some();
    let app_id = std::env::var("FOCUS_CAPTURE_CLASS").unwrap_or_else(|_| "focus".into());
    let window_size = match std::env::var("FOCUS_CAPTURE").as_deref() {
        Ok("sm") => size(px(1180.), px(760.)),
        _ => size(px(1600.), px(1000.)),
    };
    Application::new().run(move |cx| {
        gpui_component::init(cx);
        focus_i18n::init(None);
        if let Err(error) = focus_kit::init(Mode::Dark, cx) {
            eprintln!("demo: interface: {error:#}");
            cx.quit();
            return;
        }
        let options = WindowOptions {
            focus: !scripted,
            show: true,
            titlebar: Some(TitlebarOptions {
                title: Some("Focus graph".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                window_size,
                cx,
            ))),
            app_id: Some(app_id),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            let demo = cx.new(|cx| Demo::new(vault, window, cx));
            if let Some(steps) = steps {
                let weak = demo.downgrade();
                window
                    .spawn(cx, async move |cx| run(weak, steps, cx).await)
                    .detach();
            }
            cx.new(|cx| Root::new(demo, window, cx))
        });
        if let Err(error) = opened {
            eprintln!("demo: window: {error:#}");
            cx.quit();
        }
    });
    Ok(())
}
