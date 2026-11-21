use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use focus_core::{OpenOptions, Vault};
use focus_kit::{ActiveTokens, Mode};
use focus_view_reading::model::LinkGoal;
use focus_view_reading::{
    CodeDecoration, CrumbClicked, Decor, Decorated, FoldState, LanguageChosen, LinkPreview,
    LiveKind, LiveOpen, ReadingEvent, ReadingView, ScriptMouse, Typesetting, VariableChosen,
};
use gpui::{
    AppContext, Application, AsyncWindowContext, Bounds, Context, Entity, IntoElement, Keystroke,
    ParentElement, Pixels, Point, Render, Styled, TitlebarOptions, WeakEntity, Window,
    WindowBounds, WindowOptions, div, point, prelude::FluentBuilder, px, size,
};
use gpui_component::Root;
use gpui_component::input::{Input, InputEvent, InputState};

const FRAME: Duration = Duration::from_millis(16);

enum Step {
    Open(String),
    Line(u32),
    Scroll(f32),
    Fold(bool),
    Width(Option<f32>),
    Compact(Option<f32>),
    Move(String),
    Click(String, usize, gpui::Modifiers),
    Drag(String, String),
    Mouse(f32, f32),
    ClickAt(f32, f32),
    Wheel(f32),
    Viewer(usize),
    Diagram(usize),
    Language(u32, String),
    Copied(u32),
    Secrets(u32),
    Variables(bool),
    Stamp,
    VariablePicker(u32, String),
    Decorate(bool),
    Figure(bool),
    Fonts(String, String),
    Remote(Option<usize>),
    Live(bool),
    Tab(String),
    BasePage(String),
    OpenBlockAt(u32),
    LiveMove(bool),
    LiveClose,
    Theme(bool),
    Lang(String),
    MeasureOpen(String),
    MeasureScroll(usize),
    Key(String),
    Type(String),
    Wait(u64),
    Shot(String),
    Quit,
}

fn parse(script: &str) -> Result<Vec<(usize, Step)>, String> {
    let mut steps = Vec::new();
    for (i, raw) in script.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line
            .split_once(' ')
            .map_or((line, ""), |(w, r)| (w, r.trim()));
        let fail = |m: &str| format!("line {}: {m}", i + 1);
        let number = |s: &str| s.parse::<f32>().map_err(|_| fail("expected a number"));
        let pair = |s: &str| -> Result<(f32, f32), String> {
            let (x, y) = s.split_once(' ').ok_or_else(|| fail("expected x y"))?;
            Ok((number(x.trim())?, number(y.trim())?))
        };
        let on_off = |s: &str| match s {
            "on" => Ok(true),
            "off" => Ok(false),
            _ => Err(fail("expected on or off")),
        };
        let step = match word {
            "open" => Step::Open(rest.to_string()),
            "line" => Step::Line(rest.parse().map_err(|_| fail("expected a line number"))?),
            "scroll" => Step::Scroll(number(rest)?),
            "fold" => Step::Fold(rest == "all"),
            "width" if rest == "sheet" => Step::Width(None),
            "width" => Step::Width(Some(number(rest)?)),
            "compact" if rest == "off" => Step::Compact(None),
            "compact" => Step::Compact(Some(number(rest)?)),
            "move" => Step::Move(rest.to_string()),
            "click" => Step::Click(rest.to_string(), 1, gpui::Modifiers::default()),
            "dclick" => Step::Click(rest.to_string(), 2, gpui::Modifiers::default()),
            "tclick" => Step::Click(rest.to_string(), 3, gpui::Modifiers::default()),
            "shiftclick" => Step::Click(
                rest.to_string(),
                1,
                gpui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            ),
            "altclick" => Step::Click(
                rest.to_string(),
                1,
                gpui::Modifiers {
                    alt: true,
                    ..Default::default()
                },
            ),
            "drag" => {
                let (from, to) = rest
                    .split_once(" | ")
                    .ok_or_else(|| fail("expected <text> | <text>"))?;
                Step::Drag(from.trim().to_string(), to.trim().to_string())
            }
            "mouse" => {
                let (x, y) = pair(rest)?;
                Step::Mouse(x, y)
            }
            "clickat" => {
                let (x, y) = pair(rest)?;
                Step::ClickAt(x, y)
            }
            "wheel" => Step::Wheel(number(rest)?),
            "viewer" => Step::Viewer(rest.parse().map_err(|_| fail("expected an image index"))?),
            "diagram" => Step::Diagram(rest.parse().map_err(|_| fail("expected a link index"))?),
            "language" => {
                let (line, text) = rest
                    .split_once(' ')
                    .ok_or_else(|| fail("expected <line> <text>"))?;
                Step::Language(
                    line.parse().map_err(|_| fail("expected a line number"))?,
                    text.to_string(),
                )
            }
            "variables" => Step::Variables(on_off(rest)?),
            "stamp" => Step::Stamp,
            "varpicker" => {
                let (line, text) = rest
                    .split_once(' ')
                    .ok_or_else(|| fail("expected <line> <text>"))?;
                Step::VariablePicker(
                    line.parse().map_err(|_| fail("expected a line number"))?,
                    text.to_string(),
                )
            }
            "secrets" => Step::Secrets(rest.parse().map_err(|_| fail("expected a source line"))?),
            "copied" => Step::Copied(rest.parse().map_err(|_| fail("expected a source line"))?),
            "decorate" => Step::Decorate(on_off(rest)?),
            "figure" => Step::Figure(on_off(rest)?),
            "live" => Step::Live(rest == "on"),
            "tab" => Step::Tab(rest.to_string()),
            "basepage" => Step::BasePage(rest.to_string()),
            "liveopen" => {
                Step::OpenBlockAt(rest.parse().map_err(|_| fail("expected a source line"))?)
            }
            "liveup" => Step::LiveMove(true),
            "livedown" => Step::LiveMove(false),
            "liveclose" => Step::LiveClose,
            "remote" => Step::Remote(match rest {
                "never" => Some(0),
                "click" => Some(1),
                "always" => Some(2),
                "load" => None,
                _ => return Err(fail("expected never, click, always or load")),
            }),
            "fonts" => match rest.split_once('|') {
                Some((text, code)) => Step::Fonts(text.trim().to_string(), code.trim().to_string()),
                None => return Err(fail("expected <text family> | <code family>")),
            },
            "theme" => Step::Theme(match rest {
                "dark" => true,
                "light" => false,
                _ => return Err(fail("expected dark or light")),
            }),
            "lang" => Step::Lang(rest.to_string()),
            "measure-open" => Step::MeasureOpen(rest.to_string()),
            "measure-scroll" => {
                Step::MeasureScroll(rest.parse().map_err(|_| fail("expected a frame count"))?)
            }
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

struct DemoVariables;

impl CodeDecoration for DemoVariables {
    fn decorate(&self, _lang: &str, code: &str) -> Option<Decorated> {
        let known = [
            ("RHOST", "10.10.10.5"),
            ("LPORT", "4444"),
            ("USER", "svc_backup"),
            ("PASS", "Winter2026!"),
        ];
        let secret = |name: &str| name == "PASS";
        let mut spans = Vec::new();
        let mut resolved_spans = Vec::new();
        let mut resolved = String::new();
        let (mut clear, mut clear_spans) = (String::new(), Vec::new());
        let mut masked = false;
        let mut at = 0;
        let bytes = code.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' {
                let end = code[i + 1..]
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .map_or(code.len(), |e| i + 1 + e);
                if end > i + 1 {
                    let name = &code[i + 1..end];
                    resolved.push_str(&code[at..i]);
                    clear.push_str(&code[at..i]);
                    match known.iter().find(|(n, _)| *n == name) {
                        Some((_, value)) => {
                            spans.push((i..end, Decor::Resolved));
                            let shown = if secret(name) { "********" } else { value };
                            masked |= secret(name);
                            resolved_spans.push((
                                resolved.len()..resolved.len() + shown.len(),
                                Decor::Resolved,
                            ));
                            resolved.push_str(shown);
                            clear_spans
                                .push((clear.len()..clear.len() + value.len(), Decor::Resolved));
                            clear.push_str(value);
                        }
                        None => {
                            spans.push((i..end, Decor::Unresolved));
                            resolved_spans.push((
                                resolved.len()..resolved.len() + end - i,
                                Decor::Unresolved,
                            ));
                            resolved.push_str(&code[i..end]);
                            clear_spans
                                .push((clear.len()..clear.len() + end - i, Decor::Unresolved));
                            clear.push_str(&code[i..end]);
                        }
                    }
                    at = end;
                    i = end;
                    continue;
                }
            }
            i += 1;
        }
        resolved.push_str(&code[at..]);
        clear.push_str(&code[at..]);
        let revealed = masked.then_some((clear, clear_spans));
        (!spans.is_empty()).then_some(Decorated {
            spans,
            resolved,
            resolved_spans,
            revealed,
        })
    }
}

struct Demo {
    vault: Option<Arc<Vault>>,
    full: Entity<ReadingView>,
    compact: Option<(f32, Entity<ReadingView>)>,
    width: Option<f32>,
    figure: bool,
    note: Option<String>,
    error: Option<String>,
    frame_start: Rc<Cell<Option<Instant>>>,
    painted: Rc<Cell<Option<Instant>>>,
    preview: Option<(Entity<LinkPreview>, Point<Pixels>)>,
    folds: std::collections::HashMap<String, FoldState>,
    text: Option<String>,
    meta: Option<Arc<focus_core::NoteMeta>>,
    live: Option<LiveEditor>,
    live_request: Option<LiveOpen>,
}

struct LiveEditor {
    input: Entity<InputState>,
    lines: std::ops::Range<u32>,
    newline: bool,
    _changes: gpui::Subscription,
}

struct BlockEditor(Entity<InputState>);

impl Render for BlockEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(6.))
            .font_family(focus_kit::typography::MONO)
            .text_size(px(13.5))
            .line_height(px(21.))
            .child(Input::new(&self.0).appearance(false).p_0())
    }
}

fn source_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn typesetting() -> Typesetting {
    Typesetting {
        base: 15.,
        measure: 760.,
        leading: 25. / 15.,
        tinted: true,
        text_font: focus_kit::typography::SANS.into(),
        code_font: focus_kit::typography::MONO.into(),
    }
}

impl Demo {
    fn new(root: PathBuf, cx: &mut Context<Self>) -> Self {
        let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/demo-cache/reading")
            .join(root.file_name().unwrap_or_default());
        let task = cx.background_spawn(async move {
            Vault::open(
                &root,
                OpenOptions {
                    cache_dir: cache,
                    ignore: Vec::new(),
                    watch: false,
                },
            )
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(vault) => this.vault = Some(vault),
                    Err(error) => this.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        let full = cx.new(|cx| ReadingView::new(typesetting(), cx));
        Self::listen(&full, cx);
        Demo {
            vault: None,
            full,
            compact: None,
            width: None,
            figure: false,
            note: None,
            error: None,
            frame_start: Rc::new(Cell::new(None)),
            painted: Rc::new(Cell::new(None)),
            preview: None,
            folds: Default::default(),
            text: None,
            meta: None,
            live: None,
            live_request: None,
        }
    }

    fn listen(view: &Entity<ReadingView>, cx: &mut Context<Self>) {
        cx.subscribe(
            view,
            |this: &mut Demo, _, event: &ReadingEvent, cx| match event {
                ReadingEvent::OpenLink { goal, new_tab } => {
                    println!("event: open {goal:?} new_tab={new_tab}")
                }
                ReadingEvent::Hover { goal, position } => {
                    println!("event: hover {goal:?}");
                    match (goal, this.vault.clone()) {
                        (Some(LinkGoal::Note { id, anchor }), Some(vault)) => {
                            let preview = this
                                .preview
                                .take()
                                .map(|(p, _)| p)
                                .unwrap_or_else(|| cx.new(LinkPreview::new));
                            preview.update(cx, |p, cx| p.show(*id, anchor.clone(), vault, cx));
                            this.preview = Some((preview, *position));
                        }
                        _ => this.preview = None,
                    }
                    cx.notify();
                }
                ReadingEvent::ToggleTask(line) => println!("event: toggle task on line {line}"),
                ReadingEvent::Zoom(_) => println!("event: zoom diagram"),
                ReadingEvent::StartWriting => println!("event: start writing"),
            },
        )
        .detach();
        cx.subscribe(view, |_, _, event: &LanguageChosen, _| {
            println!("event: language {} on line {}", event.lang, event.line)
        })
        .detach();
        cx.subscribe(view, |_, _, event: &VariableChosen, _| {
            println!("event: variable {} on line {}", event.name, event.line)
        })
        .detach();
        cx.subscribe(view, |_, _, event: &CrumbClicked, _| {
            println!("event: crumb {}", event.folder.display())
        })
        .detach();
        cx.subscribe(view, |this: &mut Demo, _, event: &LiveOpen, cx| {
            println!(
                "event: live open {:?} lines {:?} at {}:{}",
                event.kind, event.lines, event.line, event.column
            );
            this.live_request = Some(event.clone());
            cx.notify();
        })
        .detach();
    }

    fn idle(&self, cx: &gpui::App) -> bool {
        self.vault.is_some()
            && self
                .preview
                .as_ref()
                .is_none_or(|(p, _)| !p.read(cx).busy())
            && !self.full.read(cx).busy()
            && self
                .compact
                .as_ref()
                .is_none_or(|(_, c)| !c.read(cx).busy())
            && focus_view_reading::media::loading(cx) == 0
    }

    fn open_live(&mut self, open: LiveOpen, window: &mut Window, cx: &mut Context<Self>) {
        self.close_live(cx);
        let Some(text) = self.text.as_mut() else {
            return;
        };
        let mut lines = open.lines.clone();
        if open.kind == LiveKind::End {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            if !text.is_empty() && !text.ends_with("\n\n") {
                text.push('\n');
            }
            let at = source_lines(text).len() as u32;
            lines = at..at + 1;
        }
        let all = source_lines(text);
        let block: String = all
            [(lines.start as usize).min(all.len())..(lines.end as usize).min(all.len())]
            .concat();
        let newline = block.ends_with('\n');
        let shown = block.strip_suffix('\n').unwrap_or(&block).to_string();
        let caret = open.line.saturating_sub(lines.start);
        let column = open.column;
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(1, 400)
                .soft_wrap(true);
            state.set_value(shown, window, cx);
            state.set_cursor_position(
                gpui_component::input::Position {
                    line: caret,
                    character: column,
                },
                window,
                cx,
            );
            state
        });
        let changes = cx.subscribe(&input, |this: &mut Demo, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.live_changed(cx);
            }
        });
        let editor = cx.new(|_| BlockEditor(input.clone()));
        self.full
            .update(cx, |v, cx| v.open_block(lines.clone(), editor.into(), cx));
        self.live = Some(LiveEditor {
            input,
            lines,
            newline,
            _changes: changes,
        });
        if open.kind == LiveKind::End {
            self.live_changed(cx);
        }
    }

    fn live_changed(&mut self, cx: &mut Context<Self>) {
        let (Some(live), Some(text), Some(vault)) =
            (self.live.as_mut(), self.text.as_ref(), self.vault.clone())
        else {
            return;
        };
        let all = source_lines(text);
        let (start, end) = (
            (live.lines.start as usize).min(all.len()),
            (live.lines.end as usize).min(all.len()),
        );
        let value = live.input.read(cx).value().to_string();
        let mut next = all[..start].concat();
        next.push_str(&value);
        if live.newline || end < all.len() {
            next.push('\n');
        }
        next.push_str(&all[end..].concat());
        let count = value.split('\n').count() as u32;
        live.lines = live.lines.start..live.lines.start + count;
        let lines = live.lines.clone();
        self.text = Some(next.clone());
        let meta = self.meta.clone();
        self.full.update(cx, |v, cx| {
            v.set_markdown(next, meta, vault, cx);
            v.set_open_lines(lines, cx);
        });
    }

    fn close_live(&mut self, cx: &mut Context<Self>) {
        if self.live.take().is_some() {
            self.full.update(cx, |v, cx| v.close_block(cx));
        }
    }

    fn views(&self) -> Vec<Entity<ReadingView>> {
        std::iter::once(self.full.clone())
            .chain(self.compact.as_ref().map(|(_, c)| c.clone()))
            .collect()
    }

    fn load(&mut self, path: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let vault = self.vault.clone().ok_or("vault not loaded")?;
        let id = vault
            .note_by_path(std::path::Path::new(path))
            .ok_or_else(|| format!("no note {path}"))?;
        let text = vault.read(id).map_err(|e| e.to_string())?;
        let meta = vault.note(id);
        if let Some(left) = self.note.clone() {
            let state = self.full.read(cx).fold_state();
            self.folds.insert(left, state);
        }
        for view in self.views() {
            let (text, meta, vault) = (text.clone(), meta.clone(), vault.clone());
            view.update(cx, |v, cx| v.set_markdown(text, meta, vault, cx));
        }
        let state = self.folds.get(path).cloned().unwrap_or_default();
        self.full.update(cx, |v, cx| v.set_fold_state(state, cx));
        self.close_live(cx);
        self.text = Some(text);
        self.meta = meta;
        self.note = Some(path.to_string());
        Ok(())
    }

    fn target(&self, at: Point<Pixels>, window: &Window) -> Entity<ReadingView> {
        match &self.compact {
            Some((width, view))
                if f32::from(at.x) > f32::from(window.viewport_size().width) - width =>
            {
                view.clone()
            }
            _ => self.full.clone(),
        }
    }

    fn mouse(&self, input: ScriptMouse, window: &mut Window, cx: &mut Context<Self>) {
        let at = match input {
            ScriptMouse::Move(p) | ScriptMouse::Drag(p) | ScriptMouse::Up(p) => p,
            ScriptMouse::Down { at, .. } | ScriptMouse::Press { at, .. } => at,
        };
        self.target(at, window)
            .update(cx, |v, cx| v.script_mouse(input, window, cx));
    }

    fn click(
        &self,
        at: Point<Pixels>,
        count: usize,
        modifiers: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mouse(ScriptMouse::Move(at), window, cx);
        for count in 1..=count {
            self.mouse(
                ScriptMouse::Press {
                    at,
                    count,
                    modifiers,
                },
                window,
                cx,
            );
            self.mouse(ScriptMouse::Up(at), window, cx);
        }
    }

    fn point(&self, spec: &str, cx: &gpui::App) -> Result<Point<Pixels>, String> {
        let (text, offset) = match spec.rsplit_once(" @") {
            Some((t, d)) => {
                let (dx, dy) = d.split_once(',').unwrap_or((d, "0"));
                let number = |s: &str| {
                    s.trim()
                        .parse::<f32>()
                        .map_err(|_| "expected @<dx> or @<dx>,<dy>".to_string())
                };
                (t, point(px(number(dx)?), px(number(dy)?)))
            }
            None => (spec, point(px(0.), px(0.))),
        };
        if let Some((kind, nth)) = text.strip_prefix('#').and_then(|z| z.split_once(' ')) {
            let nth = nth
                .trim()
                .parse::<usize>()
                .map_err(|_| "expected #<kind> <n>")?;
            return self
                .full
                .read(cx)
                .script_zone_point(kind, nth)
                .map(|p| p + offset)
                .ok_or_else(|| format!("no {kind} {nth} drawn"));
        }
        self.views()
            .iter()
            .find_map(|v| v.read(cx).point_of(text))
            .map(|p| p + offset)
            .ok_or_else(|| format!("text not drawn: {text}"))
    }

    fn apply(
        &mut self,
        step: &Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match step {
            Step::Open(path) => self.load(path, cx)?,
            Step::Line(line) => self.full.update(cx, |v, _| v.scroll_to_line(*line)),
            Step::Scroll(fraction) => self.full.update(cx, |v, _| v.scroll_to_fraction(*fraction)),
            Step::Fold(all) => self.full.update(cx, |v, cx| {
                if *all {
                    v.fold_all_sections(cx)
                } else {
                    v.unfold_all_sections(cx)
                }
            }),
            Step::Width(width) => self.width = *width,
            Step::Compact(None) => self.compact = None,
            Step::Compact(Some(width)) => {
                let view = cx.new(|cx| ReadingView::compact(Typesetting::compact(560.), cx));
                Self::listen(&view, cx);
                self.compact = Some((*width, view));
                if let Some(path) = self.note.clone() {
                    self.load(&path, cx)?;
                }
            }
            Step::Remote(Some(index)) => focus_view_reading::set_remote_images(
                focus_view_reading::RemoteImages::from_index(*index),
                cx,
            ),
            Step::Remote(None) => self.full.update(cx, |v, cx| v.load_remote_images(cx)),
            Step::Tab(title) => self.full.update(cx, |v, cx| v.select_tab(title, cx)),
            Step::BasePage(path) => {
                let vault = self.vault.clone().ok_or("vault not loaded")?;
                self.full
                    .update(cx, |v, cx| v.set_base_page(PathBuf::from(path), vault, cx));
            }
            Step::Live(on) => {
                if !*on {
                    self.live = None;
                }
                self.full.update(cx, |v, cx| v.set_live(*on, cx));
            }
            Step::OpenBlockAt(line) => {
                let (kind, lines) = self
                    .full
                    .read(cx)
                    .live_block_at(*line)
                    .ok_or_else(|| format!("no block on line {line}"))?;
                self.open_live(
                    LiveOpen {
                        kind,
                        lines,
                        line: *line,
                        column: 0,
                    },
                    window,
                    cx,
                );
            }
            Step::LiveMove(up) => {
                let lines = self
                    .live
                    .as_ref()
                    .map(|l| l.lines.clone())
                    .ok_or("no open block")?;
                let (before, after) = self.full.read(cx).live_neighbors(&lines);
                let (kind, lines) = if *up { before } else { after }.ok_or("no neighbour")?;
                let line = if *up { lines.end - 1 } else { lines.start };
                self.open_live(
                    LiveOpen {
                        kind,
                        lines,
                        line,
                        column: 0,
                    },
                    window,
                    cx,
                );
            }
            Step::LiveClose => self.close_live(cx),
            Step::Fonts(text, code) => {
                let typesetting = Typesetting {
                    text_font: text.clone().into(),
                    code_font: code.clone().into(),
                    ..typesetting()
                };
                self.full
                    .update(cx, |v, cx| v.set_typesetting(typesetting, cx));
            }
            Step::Figure(on) => {
                self.figure = *on;
                let width =
                    on.then(|| typesetting().measure + f32::from(focus_kit::size::SHEET_PAD) * 2.);
                self.full.update(cx, |v, cx| v.set_sheet_width(width, cx));
            }
            Step::Decorate(on) => {
                let decoration: Option<Arc<dyn CodeDecoration>> =
                    on.then(|| Arc::new(DemoVariables) as Arc<dyn CodeDecoration>);
                for view in self.views() {
                    let decoration = decoration.clone();
                    view.update(cx, |v, cx| v.set_code_decoration(decoration, cx));
                }
            }
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
                if let Some(path) = self.note.clone() {
                    self.load(&path, cx)?;
                }
                cx.refresh_windows();
            }
            Step::Move(spec) => {
                let at = self.point(spec, cx)?;
                self.mouse(ScriptMouse::Move(at), window, cx);
            }
            Step::Click(spec, count, modifiers) => {
                let at = self.point(spec, cx)?;
                self.click(at, *count, *modifiers, window, cx);
            }
            Step::ClickAt(x, y) => self.click(
                point(px(*x), px(*y)),
                1,
                gpui::Modifiers::default(),
                window,
                cx,
            ),
            Step::Drag(from, to) => {
                let (a, b) = (self.point(from, cx)?, self.point(to, cx)?);
                self.mouse(
                    ScriptMouse::Down {
                        at: a,
                        count: 1,
                        shift: false,
                    },
                    window,
                    cx,
                );
                for step in 1..=8 {
                    self.mouse(
                        ScriptMouse::Drag(a + (b - a) * (step as f32 / 8.)),
                        window,
                        cx,
                    );
                }
                self.mouse(ScriptMouse::Up(b), window, cx);
            }
            Step::Mouse(x, y) => self.mouse(ScriptMouse::Move(point(px(*x), px(*y))), window, cx),
            Step::Wheel(dy) => self.full.update(cx, |v, _| v.scroll_by(px(*dy))),
            Step::Viewer(nth) => {
                if !self
                    .full
                    .update(cx, |v, cx| v.script_open_viewer(*nth, window, cx))
                {
                    return Err(format!("no image {nth} drawn"));
                }
            }
            Step::Diagram(nth) => {
                let at = self
                    .full
                    .read(cx)
                    .script_diagram_point(*nth)
                    .ok_or_else(|| format!("no diagram link {nth} drawn"))?;
                self.mouse(ScriptMouse::Move(at), window, cx);
            }
            Step::Variables(on) => {
                let variable =
                    |name: &str, value: &str, secret: bool| focus_view_reading::CodeVariable {
                        name: name.into(),
                        value: value.into(),
                        description: String::new(),
                        secret,
                    };
                let list = on.then(|| {
                    Arc::new(focus_view_reading::CodeVariables {
                        list: vec![
                            variable("RHOST", "10.10.10.5", false),
                            variable("LPORT", "4444", false),
                            variable("USER", "svc_backup", false),
                            variable("PASS", "demo-pass", true),
                        ],
                        engaged: true,
                    })
                });
                self.full.update(cx, |v, cx| v.set_code_variables(list, cx));
            }
            Step::VariablePicker(line, spec) => {
                let at = self.point(spec, cx)?;
                self.full
                    .update(cx, |v, cx| v.script_open_variables(*line, at, window, cx));
            }
            Step::Secrets(line) => {
                if !self
                    .full
                    .update(cx, |v, cx| v.script_toggle_secrets(*line, cx))
                {
                    return Err(format!("no code block on line {line}"));
                }
            }
            Step::Copied(line) => {
                if !self
                    .full
                    .update(cx, |v, cx| v.script_show_copied(*line, cx))
                {
                    return Err(format!("no code block or diagram on line {line}"));
                }
            }
            Step::Language(line, spec) => {
                let at = self.point(spec, cx)?;
                self.full
                    .update(cx, |v, cx| v.script_open_language(*line, at, window, cx));
            }
            Step::Stamp
            | Step::MeasureOpen(_)
            | Step::MeasureScroll(_)
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(open) = self.live_request.take() {
            self.open_live(open, window, cx);
        }
        self.frame_start.set(Some(Instant::now()));
        let t = *cx.tokens();
        let measure = px(typesetting().measure);
        let painted = self.painted.clone();
        let column = match self.width {
            None if self.figure => div()
                .flex_none()
                .w(measure + focus_kit::size::SHEET_PAD * 2. + px(2.))
                .border_l_1()
                .border_r_1()
                .border_color(t.line),
            None => div()
                .flex_none()
                .w(measure + focus_kit::size::SHEET_PAD * 2. + px(2.))
                .px(focus_kit::size::SHEET_PAD)
                .border_l_1()
                .border_r_1()
                .border_color(t.line),
            Some(width) => div()
                .flex_none()
                .w(px(width))
                .px(focus_kit::size::SHEET_PAD_NARROW)
                .border_r_1()
                .border_color(t.line),
        };
        let inner = if self.figure {
            div().h_full().w_full()
        } else {
            div().h_full().w_full().max_w(measure)
        };
        div()
            .size_full()
            .flex()
            .bg(t.bg)
            .text_color(t.ink)
            .child(div().flex_1().min_w_0())
            .child(
                column
                    .h_full()
                    .bg(t.bg)
                    .child(inner.child(self.full.clone())),
            )
            .child(div().flex_1().min_w_0())
            .when_some(self.compact.clone(), |this, (width, view)| {
                this.child(
                    div()
                        .flex_none()
                        .w(px(width))
                        .h_full()
                        .border_l_1()
                        .border_color(t.line)
                        .bg(t.bg)
                        .p(focus_kit::size::S4)
                        .child(view),
                )
            })
            .when_some(self.error.clone(), |this, e| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .p_4()
                        .text_color(t.danger)
                        .child(e),
                )
            })
            .when_some(self.preview.clone(), |this, (preview, at)| {
                this.child(gpui::deferred(
                    gpui::anchored()
                        .position(at + point(px(0.), px(20.)))
                        .snap_to_window()
                        .child(preview),
                ))
            })
            .child(
                gpui::canvas(
                    |_, _, _| {},
                    move |_, _, _, _| painted.set(Some(Instant::now())),
                )
                .absolute()
                .size_0(),
            )
    }
}

async fn settle(demo: &WeakEntity<Demo>, cx: &mut AsyncWindowContext) {
    for _ in 0..3000 {
        if demo.read_with(cx, |d, cx| d.idle(cx)).unwrap_or(true) {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(2))
            .await;
    }
}

async fn painted_after(
    demo: &WeakEntity<Demo>,
    since: Instant,
    cx: &mut AsyncWindowContext,
) -> Option<Instant> {
    for _ in 0..5000 {
        let _ = cx.update(|window, _| window.refresh());
        if let Ok(Some(at)) = demo.read_with(cx, |d, _| d.painted.get())
            && at > since
        {
            return Some(at);
        }
        cx.background_executor()
            .timer(Duration::from_millis(1))
            .await;
    }
    None
}

async fn measure_open(
    demo: &WeakEntity<Demo>,
    path: &str,
    cx: &mut AsyncWindowContext,
) -> Result<(), String> {
    let mut lines = Vec::new();
    for _ in 0..5 {
        let started = demo
            .update(cx, |d, cx| -> Result<Instant, String> {
                d.full = cx.new(|cx| ReadingView::new(typesetting(), cx));
                let vault = d.vault.clone().ok_or("vault not loaded")?;
                let id = vault
                    .note_by_path(std::path::Path::new(path))
                    .ok_or_else(|| format!("no note {path}"))?;
                let text = vault.read(id).map_err(|e| e.to_string())?;
                let meta = vault.note(id);
                let started = Instant::now();
                d.full
                    .update(cx, |v, cx| v.set_markdown(text, meta, vault, cx));
                cx.notify();
                Ok(started)
            })
            .map_err(|e| e.to_string())??;
        let mut built = None;
        for _ in 0..5000 {
            if !demo
                .read_with(cx, |d, cx| d.full.read(cx).busy())
                .unwrap_or(false)
            {
                built = Some(Instant::now());
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
        }
        let built = built.ok_or("the parse did not end")?;
        let shown = painted_after(demo, built, cx)
            .await
            .ok_or("no frame painted")?;
        lines.push((
            (built - started).as_secs_f64() * 1000.,
            (shown - started).as_secs_f64() * 1000.,
        ));
        settle(demo, cx).await;
    }
    lines.sort_by(|a, b| a.1.total_cmp(&b.1));
    let median = lines[lines.len() / 2];
    println!(
        "measure open {path}: model ready {:.1} ms, first frame {:.1} ms (median of {} opens, fresh view each)",
        median.0,
        median.1,
        lines.len()
    );
    Ok(())
}

async fn measure_scroll(
    demo: &WeakEntity<Demo>,
    frames: usize,
    cx: &mut AsyncWindowContext,
) -> Result<(), String> {
    let mut samples = Vec::new();
    for i in 0..frames {
        let since = Instant::now();
        let dy = if i < frames / 2 { -60. } else { 60. };
        demo.update(cx, |d, cx| {
            d.full.update(cx, |v, cx| {
                v.scroll_by(px(dy));
                cx.notify();
            })
        })
        .map_err(|e| e.to_string())?;
        let end = painted_after(demo, since, cx)
            .await
            .ok_or("no frame painted")?;
        if let Ok(Some(start)) = demo.read_with(cx, |d, _| d.frame_start.get()) {
            samples.push((end - start).as_secs_f64() * 1000.);
        }
        cx.background_executor().timer(FRAME).await;
    }
    samples.sort_by(f64::total_cmp);
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    println!(
        "measure scroll: frame median {:.2} ms, p95 {:.2} ms, max {:.2} ms ({} frames, 60 px per step)",
        at(0.5),
        at(0.95),
        at(1.),
        samples.len()
    );
    Ok(())
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

fn stamp() {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to locals that live through the calls.
    unsafe {
        libc::clock_gettime(libc::CLOCK_REALTIME, &mut now);
        libc::localtime_r(&now.tv_sec, &mut local);
    }
    eprintln!(
        "[{:02}:{:02}:{:02}.{:06}] stamp",
        local.tm_hour,
        local.tm_min,
        local.tm_sec,
        now.tv_nsec / 1000
    );
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
            Step::Stamp => {
                stamp();
                Ok(())
            }
            Step::MeasureOpen(path) => {
                settle(&demo, cx).await;
                measure_open(&demo, path, cx).await
            }
            Step::MeasureScroll(frames) => {
                settle(&demo, cx).await;
                measure_scroll(&demo, *frames, cx).await
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
        for _ in 0..2 {
            let _ = cx.update(|window, _| window.refresh());
            cx.background_executor().timer(FRAME).await;
        }
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
        focus_view_reading::bind(cx);
        focus_md::mermaid::init_fonts(&focus_kit::fonts::text_faces());
        let options = WindowOptions {
            focus: !scripted,
            show: true,
            titlebar: Some(TitlebarOptions {
                title: Some("Focus reading".into()),
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
            let demo = cx.new(|cx| Demo::new(vault, cx));
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
