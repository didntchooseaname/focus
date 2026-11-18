use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use anyhow::{Context as _, bail};
use focus_core::{ExternalLinks, OpenOptions, Vault, VaultEvent};
use focus_kit::{ActiveTokens, Mode, Type};
use focus_view_vault::{
    Bookmarks, BrokenLinks, FileTree, Message, TagTree, TaskFilter, TaskList, Tone, TreeSettings,
    VaultViewEvent,
};
use gpui::{
    AppContext, Application, AsyncWindowContext, Bounds, Context, Entity, Focusable, IntoElement,
    Keystroke, ParentElement, Render, Styled, TitlebarOptions, WeakEntity, Window, WindowBounds,
    WindowOptions, div, prelude::FluentBuilder, px, size,
};
use gpui_component::Root;

const FRAME: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, PartialEq)]
enum Left {
    Files,
    Tags,
    Bookmarks,
}

enum Step {
    Left(Left),
    Bottom(Option<bool>),
    Select(String),
    Focus(String),
    OpenTag(String),
    TaskFilter(TaskFilter),
    Toggle(usize),
    Create(usize),
    Bookmark(String),
    Import(String, Vec<String>),
    Open(String),
    Exclude(String),
    Theme(bool),
    Lang(String),
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
        let number = |s: &str| s.parse::<usize>().map_err(|_| fail("expected a number"));
        let step = match word {
            "left" => Step::Left(match rest {
                "tags" => Left::Tags,
                "bookmarks" => Left::Bookmarks,
                _ => Left::Files,
            }),
            "bottom" => Step::Bottom(match rest {
                "tasks" => Some(true),
                "broken" => Some(false),
                _ => None,
            }),
            "select" => Step::Select(rest.to_string()),
            "focus" => match rest {
                "tree" | "filter" | "bookmarks" | "tags" | "tasks" | "broken" => {
                    Step::Focus(rest.to_string())
                }
                _ => {
                    return Err(fail(
                        "expected tree, filter, bookmarks, tags, tasks or broken",
                    ));
                }
            },
            "opentag" => Step::OpenTag(rest.to_lowercase()),
            "taskfilter" => Step::TaskFilter(match rest {
                "done" => TaskFilter::Done,
                "all" => TaskFilter::All,
                _ => TaskFilter::Open,
            }),
            "toggle" => Step::Toggle(number(rest)?),
            "create" => Step::Create(number(rest)?),
            "bookmark" => Step::Bookmark(rest.to_string()),
            "import" => {
                let (folder, paths) = rest
                    .split_once(" | ")
                    .ok_or_else(|| fail("expected <folder> | <path>..."))?;
                Step::Import(
                    folder.trim().to_string(),
                    paths.split(" | ").map(|p| p.trim().to_string()).collect(),
                )
            }
            "open" => Step::Open(rest.to_string()),
            "exclude" => Step::Exclude(rest.to_string()),
            "theme" => Step::Theme(rest != "light"),
            "lang" => Step::Lang(rest.to_string()),
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
    vault: Arc<Vault>,
    events: Receiver<VaultEvent>,
    tree: Entity<FileTree>,
    tags: Entity<TagTree>,
    tasks: Entity<TaskList>,
    broken: Entity<BrokenLinks>,
    bookmarks: Entity<Bookmarks>,
    left: Left,
    bottom: Option<bool>,
    log: Vec<String>,
    toasts: Vec<(Tone, Message)>,
}

impl Demo {
    fn new(vault: Arc<Vault>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let events = vault.subscribe();
        let tree = cx.new(|cx| FileTree::new(vault.clone(), TreeSettings::default(), window, cx));
        let tags = cx.new(|cx| TagTree::new(vault.clone(), cx));
        let tasks = cx.new(|cx| TaskList::new(vault.clone(), cx));
        let broken = cx.new(|cx| BrokenLinks::new(vault.clone(), String::new(), cx));
        let bookmarks = cx.new(|cx| Bookmarks::new(vault.clone(), None, Vec::new(), cx));
        tree.update(cx, |tree, _| tree.set_bookmarks(Some(bookmarks.clone())));
        cx.subscribe(&bookmarks, |this, _, e: &VaultViewEvent, cx| {
            this.on_event(e, cx)
        })
        .detach();
        cx.subscribe(&tree, |this, _, e: &VaultViewEvent, cx| {
            this.on_event(e, cx)
        })
        .detach();
        cx.subscribe(&tree, |_, _, e: &focus_view_vault::ImportStatus, _| {
            println!(
                "event: import {} files {} bytes running={}",
                e.files, e.bytes, e.running
            )
        })
        .detach();
        cx.subscribe(&tags, |this, _, e: &VaultViewEvent, cx| {
            this.on_event(e, cx)
        })
        .detach();
        cx.subscribe(&tasks, |this, _, e: &VaultViewEvent, cx| {
            this.on_event(e, cx)
        })
        .detach();
        cx.subscribe(&broken, |this, _, e: &VaultViewEvent, cx| {
            this.on_event(e, cx)
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(30))
                    .await;
                let alive = this.update(cx, |this, cx| this.drain(cx)).is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
        window.focus(&tree.focus_handle(cx));
        Demo {
            vault,
            events,
            tree,
            tags,
            tasks,
            broken,
            bookmarks,
            left: Left::Files,
            bottom: Some(true),
            log: Vec::new(),
            toasts: Vec::new(),
        }
    }

    fn drain(&mut self, cx: &mut Context<Self>) {
        let events: Vec<VaultEvent> = self.events.try_iter().collect();
        for event in &events {
            self.tree.update(cx, |v, cx| v.vault_changed(event, cx));
            self.tags.update(cx, |v, cx| v.vault_changed(event, cx));
            self.tasks.update(cx, |v, cx| v.vault_changed(event, cx));
            self.broken.update(cx, |v, cx| v.vault_changed(event, cx));
            self.bookmarks
                .update(cx, |v, cx| v.vault_changed(event, cx));
        }
    }

    fn on_event(&mut self, event: &VaultViewEvent, cx: &mut Context<Self>) {
        let line = match event {
            VaultViewEvent::OpenNote {
                note,
                line,
                new_tab,
            } => {
                let path = self.vault.note(*note).map(|m| m.path.clone());
                if let Some(path) = &path {
                    self.bookmarks.update(cx, |v, cx| v.opened(path, cx));
                }
                let path = path
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                format!(
                    "open {path}{}{}",
                    line.map(|l| format!(" at line {}", l + 1))
                        .unwrap_or_default(),
                    if *new_tab { " in a new tab" } else { "" }
                )
            }
            VaultViewEvent::OpenFile { path, new_tab } => format!(
                "open file {}{}",
                path.display(),
                if *new_tab { " in a new tab" } else { "" }
            ),
            VaultViewEvent::OpenWithSystem(path) => {
                format!("open with the system {}", path.display())
            }
            VaultViewEvent::ShowVersions(note) => format!("versions {note:?}"),
            VaultViewEvent::SearchTag(tag) => format!("search tag:{tag}"),
            VaultViewEvent::RenameTag(tag) => format!("rename tag {tag}"),
            VaultViewEvent::MergeNote(note) => format!("merge {note:?}"),
            VaultViewEvent::Notify { message, tone } => {
                self.toasts.push((*tone, message.clone()));
                format!("notify {}", message.text())
            }
            VaultViewEvent::BookmarksChanged(list) => format!("bookmarks {}", list.len()),
            VaultViewEvent::RecentChanged(list) => format!("recent {}", list.len()),
        };
        println!("event: {line}");
        self.log.push(line);
        cx.notify();
    }

    fn apply(
        &mut self,
        step: &Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match step {
            Step::Left(left) => self.left = *left,
            Step::Bottom(which) => self.bottom = *which,
            Step::Select(path) => {
                if !self.tree.update(cx, |v, cx| v.select(Path::new(path), cx)) {
                    return Err(format!("not in the tree: {path}"));
                }
                window.focus(&self.tree.focus_handle(cx));
            }
            Step::Focus(name) => match name.as_str() {
                "bookmarks" => window.focus(&self.bookmarks.focus_handle(cx)),
                "tags" => window.focus(&self.tags.focus_handle(cx)),
                "tasks" => window.focus(&self.tasks.focus_handle(cx)),
                "broken" => window.focus(&self.broken.focus_handle(cx)),
                _ => window.focus(&self.tree.focus_handle(cx)),
            },
            Step::OpenTag(tag) => self.tags.update(cx, |v, cx| v.toggle(tag, cx)),
            Step::TaskFilter(filter) => self.tasks.update(cx, |v, cx| v.set_filter(*filter, cx)),
            Step::Toggle(n) => {
                if !self.tasks.update(cx, |v, cx| v.script_toggle(*n, cx)) {
                    return Err(format!("no task {n}"));
                }
            }
            Step::Create(n) => {
                if !self.broken.update(cx, |v, cx| v.script_create(*n, cx)) {
                    return Err(format!("no broken link {n}"));
                }
            }
            Step::Bookmark(path) => self
                .bookmarks
                .update(cx, |v, cx| v.toggle(Path::new(path), cx)),
            Step::Import(folder, paths) => {
                let sources = paths.iter().map(PathBuf::from).collect();
                let folder = if folder == "/" {
                    PathBuf::new()
                } else {
                    PathBuf::from(folder)
                };
                self.tree.update(cx, |v, cx| {
                    v.import(sources, folder, focus_core::ImportMode::Copy, cx)
                });
            }
            Step::Open(path) => {
                let Some(note) = self.vault.note_by_path(Path::new(path)) else {
                    return Err(format!("no note at {path}"));
                };
                self.on_event(
                    &VaultViewEvent::OpenNote {
                        note,
                        line: None,
                        new_tab: false,
                    },
                    cx,
                );
            }
            Step::Exclude(folder) => {
                let folders: Vec<PathBuf> = if folder.is_empty() {
                    Vec::new()
                } else {
                    vec![PathBuf::from(folder)]
                };
                self.tasks
                    .update(cx, |v, cx| v.set_excluded_folders(folders.clone(), cx));
                self.broken
                    .update(cx, |v, cx| v.set_excluded_folders(folders, cx));
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
                cx.refresh_windows();
            }
            Step::Key(_) | Step::Type(_) | Step::Wait(_) | Step::Shot(_) | Step::Quit => {}
        }
        cx.notify();
        Ok(())
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let dir = focus_kit::Dir::current();
        let tab = |label: String, on: bool| {
            div()
                .px(focus_kit::size::S3)
                .h(px(32.))
                .flex()
                .items_center()
                .ui_sm()
                .text_color(if on { t.ink } else { t.ink_3 })
                .when(on, |d| d.border_b_1().border_color(t.ink))
                .child(label)
        };
        let left = div()
            .w(focus_kit::size::LEFT)
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .border_color(t.line)
            .map(|d| dir.border_end(d, t.line))
            .child(
                dir.row()
                    .border_b_1()
                    .border_color(t.line)
                    .child(tab(
                        focus_i18n::t("ui-left-files"),
                        self.left == Left::Files,
                    ))
                    .child(tab(focus_i18n::t("ui-left-tags"), self.left == Left::Tags))
                    .child(tab(
                        focus_i18n::t("ui-left-bookmarks"),
                        self.left == Left::Bookmarks,
                    )),
            )
            .child(div().flex_1().min_h_0().map(|d| match self.left {
                Left::Files => d.child(self.tree.clone()),
                Left::Tags => d.child(self.tags.clone()),
                Left::Bookmarks => d.child(self.bookmarks.clone()),
            }));
        let bottom = self.bottom.map(|tasks| {
            div()
                .h(px(280.))
                .flex_none()
                .flex()
                .flex_col()
                .border_t_1()
                .border_color(t.line)
                .child(
                    dir.row()
                        .border_b_1()
                        .border_color(t.line)
                        .child(tab(focus_i18n::t("ui-bottom-tasks"), tasks))
                        .child(tab(focus_i18n::t("ui-bottom-broken"), !tasks)),
                )
                .child(div().flex_1().min_h_0().map(|d| {
                    if tasks {
                        d.child(self.tasks.clone())
                    } else {
                        d.child(self.broken.clone())
                    }
                }))
        });
        let center = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(focus_kit::size::S6)
                    .flex()
                    .flex_col()
                    .gap(focus_kit::size::S1)
                    .mono_sm()
                    .text_color(t.ink_3)
                    .children(
                        self.log
                            .iter()
                            .rev()
                            .take(12)
                            .rev()
                            .map(|l| div().child(l.clone())),
                    ),
            )
            .children(bottom);
        let toasts = self
            .toasts
            .iter()
            .rev()
            .take(3)
            .rev()
            .map(|(tone, message)| {
                let tone = match tone {
                    Tone::Ok => focus_kit::toast::Tone::Ok,
                    Tone::Danger => focus_kit::toast::Tone::Danger,
                };
                focus_kit::toast::Toast::new(tone, message.text())
            });
        dir.row()
            .relative()
            .size_full()
            .bg(t.bg)
            .text_color(t.ink)
            .child(left)
            .child(center)
            .child(
                dir.end(div().absolute(), focus_kit::size::S4)
                    .bottom(px(300.))
                    .flex()
                    .flex_col()
                    .gap(focus_kit::size::S2)
                    .children(toasts),
            )
    }
}

async fn shot(name: &str, cx: &mut AsyncWindowContext) -> Result<(), String> {
    let command =
        std::env::var("FOCUS_SHOT_CMD").map_err(|_| "FOCUS_SHOT_CMD missing".to_string())?;
    for _ in 0..4 {
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
            Step::Shot(name) => shot(name, cx).await,
            Step::Focus(name) if name == "filter" => {
                match demo.update_in(cx, |d, window, cx| d.apply(&step, window, cx)) {
                    Ok(Ok(())) => cx
                        .update(|window, cx| {
                            if let Ok(k) = Keystroke::parse("ctrl-f") {
                                window.dispatch_keystroke(k, cx);
                            }
                        })
                        .map_err(|e| e.to_string()),
                    Ok(Err(e)) => Err(e),
                    Err(e) => Err(e.to_string()),
                }
            }
            _ => match demo.update_in(cx, |d, window, cx| d.apply(&step, window, cx)) {
                Ok(r) => r,
                Err(e) => Err(e.to_string()),
            },
        };
        if let Err(message) = outcome {
            eprintln!("demo: line {line}: {message}");
        }
        for _ in 0..3 {
            let _ = cx.update(|window, _| window.refresh());
            cx.background_executor().timer(FRAME).await;
        }
    }
    let _ = cx.update(|_, cx| cx.quit());
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut vault = None;
    let mut script = None;
    let mut copy = std::env::var_os("FOCUS_DEMO_COPY").is_some();
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
            "--copy" => copy = true,
            other => bail!(
                "unknown argument: {other} (expected --vault <path>, --copy, --script <file>)"
            ),
        }
    }
    let mut root = vault.context("--vault is required")?;
    let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/vault-demo");
    if copy {
        let _ = std::fs::remove_dir_all(&work);
        copy_dir(&root, &work.join("vault"))
            .with_context(|| format!("copying {}", root.display()))?;
        root = work.join("vault");
    }
    let external = if std::env::var_os("FOCUS_DEMO_FOLLOW_LINKS").is_some() {
        ExternalLinks::Follow
    } else {
        ExternalLinks::Skip
    };
    let opened = Vault::open_with(
        &root,
        OpenOptions {
            cache_dir: work
                .join("cache")
                .join(root.file_name().unwrap_or_default()),
            ignore: Vec::new(),
            watch: false,
        },
        external,
    )?;
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
        focus_view_vault::bind(cx);
        let options = WindowOptions {
            focus: !scripted,
            show: true,
            titlebar: Some(TitlebarOptions {
                title: Some("Focus vault".into()),
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
        let result = cx.open_window(options, |window, cx| {
            let demo = cx.new(|cx| Demo::new(opened, window, cx));
            if let Some(steps) = steps {
                let weak = demo.downgrade();
                window
                    .spawn(cx, async move |cx| run(weak, steps, cx).await)
                    .detach();
            }
            cx.new(|cx| Root::new(demo, window, cx))
        });
        if let Err(error) = result {
            eprintln!("demo: window: {error:#}");
            cx.quit();
        }
    });
    Ok(())
}
