use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use focus_core::{HistoryConfig, HistoryKind, OpenOptions, Vault};
use focus_kit::{ActiveTokens, Mode, Type};
use focus_view_history::{VersionsDialog, VersionsEvent, model};
use gpui::{
    AppContext, Application, AsyncWindowContext, Bounds, Context, Entity, InteractiveElement,
    IntoElement, Keystroke, ParentElement, Render, Styled, Subscription, WeakEntity, Window,
    WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::Root;

struct Demo {
    vault: Arc<Vault>,
    dialog: Option<Entity<VersionsDialog>>,
    log: Vec<String>,
    _subscription: Option<Subscription>,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        div()
            .id("demo")
            .relative()
            .size_full()
            .bg(t.bg)
            .p(px(16.))
            .mono_sm()
            .text_color(t.ink_3)
            .children(self.log.iter().rev().take(20).cloned())
            .children(self.dialog.clone())
    }
}

#[derive(Debug)]
enum Step {
    Key(String),
    Wait(u64),
    Shot(String),
    ShotNow(String),
    Theme(bool),
    Lang(String),
    Settle,
    Open(String),
    Seed(usize),
    Large(usize),
    Select(usize),
    Tab(bool),
    Restore,
    Copy,
    History(bool),
    Measure,
    Edit(String),
    Quit,
}

fn parse_script(text: &str) -> anyhow::Result<Vec<Step>> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(w, r)| (w, r.trim()));
        let number = || {
            rest.parse::<usize>()
                .with_context(|| format!("line {}: a number is expected", index + 1))
        };
        let need = || {
            if rest.is_empty() {
                bail!("line {}: argument missing for {word}", index + 1)
            } else {
                Ok(rest.to_string())
            }
        };
        steps.push(match word {
            "key" => Step::Key(need()?),
            "wait" => Step::Wait(number()? as u64),
            "shot" => Step::Shot(need()?),
            "shotnow" => Step::ShotNow(need()?),
            "theme" => Step::Theme(match rest {
                "dark" => true,
                "light" => false,
                _ => bail!("line {}: dark or light expected", index + 1),
            }),
            "lang" => Step::Lang(need()?),
            "settle" => Step::Settle,
            "open" => Step::Open(need()?),
            "seed" => Step::Seed(number()?),
            "large" => Step::Large(number()?),
            "select" => Step::Select(number()?),
            "tab" => Step::Tab(match rest {
                "changes" => false,
                "rendered" => true,
                _ => bail!("line {}: changes or rendered expected", index + 1),
            }),
            "restore" => Step::Restore,
            "copy" => Step::Copy,
            "history" => Step::History(match rest {
                "on" => true,
                "off" => false,
                _ => bail!("line {}: on or off expected", index + 1),
            }),
            "measure" => Step::Measure,
            "edit" => Step::Edit(need()?),
            "quit" => Step::Quit,
            other => bail!("line {}: unknown step {other}", index + 1),
        });
    }
    Ok(steps)
}

const FRAME: Duration = Duration::from_millis(20);

async fn settle(demo: &WeakEntity<Demo>, cx: &mut AsyncWindowContext) {
    cx.background_executor().timer(FRAME * 3).await;
    loop {
        let busy = demo
            .upgrade()
            .and_then(|d| {
                cx.update(|_, cx| {
                    d.read(cx)
                        .dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.read(cx).busy())
                })
                .ok()
            })
            .unwrap_or(false);
        if !busy {
            break;
        }
        cx.background_executor().timer(FRAME).await;
    }
    cx.background_executor().timer(FRAME * 3).await;
}

async fn shot(name: &str, cx: &mut AsyncWindowContext) -> Result<(), String> {
    let command =
        std::env::var("FOCUS_SHOT_CMD").map_err(|_| "FOCUS_SHOT_CMD missing".to_string())?;
    cx.update(|window, _| window.refresh())
        .map_err(|e| e.to_string())?;
    for _ in 0..3 {
        cx.background_executor().timer(FRAME * 2).await;
    }
    let name = name.to_string();
    let status = cx
        .background_spawn(async move { std::process::Command::new(&command).arg(&name).status() })
        .await
        .map_err(|e| format!("FOCUS_SHOT_CMD: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("FOCUS_SHOT_CMD failed ({status})"))
    }
}

fn open_note(demo: &Entity<Demo>, cx: &gpui::App) -> Result<focus_core::NoteId, String> {
    demo.read(cx)
        .dialog
        .as_ref()
        .map(|d| d.read(cx).note())
        .ok_or_else(|| "no dialog open".to_string())
}

fn with_dialog(
    demo: &Entity<Demo>,
    cx: &mut gpui::App,
    f: impl FnOnce(&mut VersionsDialog, &mut Context<VersionsDialog>),
) -> Result<(), String> {
    let dialog = demo.read(cx).dialog.clone();
    dialog
        .map(|d| d.update(cx, |d, cx| f(d, cx)))
        .ok_or_else(|| "no dialog open".to_string())
}

fn lines_of(count: usize, step: usize) -> String {
    let mut text = String::from("# Large note\n\n");
    for n in 1..=count {
        if step > 0 && n % 97 == 0 {
            text.push_str(&format!("Paragraph {n} rewritten in step {step}\n"));
        } else {
            text.push_str(&format!(
                "Paragraph {n} of the large note, with some words in it\n"
            ));
        }
        if n % 10 == 0 {
            text.push('\n');
        }
    }
    text
}

fn open_dialog(
    demo: &WeakEntity<Demo>,
    path: &str,
    window: &mut Window,
    cx: &mut gpui::App,
) -> Result<(), String> {
    let demo = demo.upgrade().ok_or("window closed")?;
    let vault = demo.read(cx).vault.clone();
    let note = vault
        .note_by_path(Path::new(path))
        .ok_or_else(|| format!("no note {path}"))?;
    let dialog = cx.new(|cx| VersionsDialog::new(vault, note, window, cx));
    let subscription = cx.subscribe(&dialog, {
        let demo = demo.downgrade();
        move |_, event: &VersionsEvent, cx| {
            let line = format!("event: {event:?}");
            println!("{line}");
            let _ = demo.update(cx, |demo, cx| {
                if matches!(event, VersionsEvent::Close) {
                    demo.dialog = None;
                }
                demo.log.push(line);
                cx.notify();
            });
        }
    });
    demo.update(cx, |demo, cx| {
        demo.dialog = Some(dialog);
        demo._subscription = Some(subscription);
        cx.notify();
    });
    Ok(())
}

fn seed(demo: &Entity<Demo>, count: usize, cx: &mut gpui::App) -> Result<(), String> {
    let vault = demo.read(cx).vault.clone();
    let note = open_note(demo, cx)?;
    for step in 1..=count {
        let current = vault.read(note).map_err(|e| e.to_string())?;
        let mut lines: Vec<String> = current.lines().map(str::to_string).collect();
        let at = step.min(lines.len().saturating_sub(1));
        if let Some(line) = lines.get_mut(at) {
            *line = format!("{line} (revised {step})");
        }
        lines.push(format!("Added in step {step}"));
        let text = lines.join("\n") + "\n";
        vault
            .write_with(note, &text, None, HistoryKind::Bulk)
            .map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(3));
    }
    let dialog = demo.read(cx).dialog.clone();
    if let Some(dialog) = dialog {
        dialog.update(cx, |dialog, cx| dialog.reload(cx));
    }
    Ok(())
}

fn large(
    demo: &Entity<Demo>,
    lines: usize,
    window: &mut Window,
    cx: &mut gpui::App,
) -> Result<(), String> {
    let vault = demo.read(cx).vault.clone();
    let relative = Path::new("Large note.md");
    let note = match vault.note_by_path(relative) {
        Some(note) => note,
        None => vault
            .create_note(relative, &lines_of(lines, 0))
            .map_err(|e| e.to_string())?,
    };
    for step in 1..=3 {
        vault
            .write_with(note, &lines_of(lines, step), None, HistoryKind::Bulk)
            .map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(3));
    }
    open_dialog(&demo.downgrade(), "Large note.md", window, cx)
}

fn measure(demo: &Entity<Demo>, cx: &mut gpui::App) -> Result<(), String> {
    let vault = demo.read(cx).vault.clone();
    let note = open_note(demo, cx)?;
    let started = Instant::now();
    let snapshot = model::snapshot(&vault, note).map_err(|e| e.to_string())?;
    let snapshot_time = started.elapsed();
    let first = snapshot.versions.first().ok_or("no version")?;
    let started = Instant::now();
    let comparison =
        model::compare(&vault, note, &first.id, &snapshot.current).map_err(|e| e.to_string())?;
    println!(
        "snapshot: {:.1} ms ({} versions, {} bytes now); compare: {:.1} ms ({} rows, +{} -{})",
        snapshot_time.as_secs_f32() * 1000.0,
        snapshot.versions.len(),
        snapshot.current.len(),
        started.elapsed().as_secs_f32() * 1000.0,
        comparison.rows.len(),
        comparison.added,
        comparison.removed
    );
    Ok(())
}

async fn run(demo: WeakEntity<Demo>, steps: Vec<Step>, cx: &mut AsyncWindowContext) {
    for step in steps {
        let outcome: Result<(), String> = match &step {
            Step::Quit => break,
            Step::Wait(ms) => {
                cx.background_executor()
                    .timer(Duration::from_millis(*ms))
                    .await;
                Ok(())
            }
            Step::Key(keys) => match Keystroke::parse(keys) {
                Ok(keystroke) => cx
                    .update(|window, cx| window.dispatch_keystroke(keystroke, cx))
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                Err(error) => Err(error.to_string()),
            },
            Step::Theme(dark) => cx
                .update(|window, cx| {
                    let result = focus_kit::theme::apply(
                        if *dark { Mode::Dark } else { Mode::Light },
                        Some(window),
                        cx,
                    );
                    window.refresh();
                    result.map_err(|e| format!("{e:#}"))
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Lang(code) => focus_i18n::set_language(code)
                .map_err(|e| e.to_string())
                .inspect(|_| {
                    let _ = cx.update(|window, _| window.refresh());
                }),
            Step::Settle => {
                settle(&demo, cx).await;
                Ok(())
            }
            Step::Shot(name) => {
                settle(&demo, cx).await;
                shot(name, cx).await
            }
            Step::ShotNow(name) => shot(name, cx).await,
            other => cx
                .update(|window, cx| {
                    let demo = demo.upgrade().ok_or("window closed".to_string())?;
                    match other {
                        Step::Open(path) => open_dialog(&demo.downgrade(), path, window, cx),
                        Step::Seed(count) => seed(&demo, *count, cx),
                        Step::Large(lines) => large(&demo, *lines, window, cx),
                        Step::Select(index) => with_dialog(&demo, cx, |d, cx| d.select(*index, cx)),
                        Step::Tab(rendered) => {
                            with_dialog(&demo, cx, |d, cx| d.set_rendered(*rendered, cx))
                        }
                        Step::Restore => with_dialog(&demo, cx, |d, cx| d.restore(cx)),
                        Step::Copy => with_dialog(&demo, cx, |d, cx| d.copy(cx)),
                        Step::History(on) => {
                            let vault = demo.read(cx).vault.clone();
                            let history = vault.history().ok_or("no history")?;
                            history.set_config(HistoryConfig {
                                enabled: *on,
                                ..history.config()
                            });
                            Ok(())
                        }
                        Step::Measure => measure(&demo, cx),
                        Step::Edit(text) => {
                            let vault = demo.read(cx).vault.clone();
                            let note = open_note(&demo, cx)?;
                            let meta = vault.note(note).ok_or("note gone")?;
                            std::fs::write(vault.absolute(&meta.path), text.replace("\\n", "\n"))
                                .map_err(|e| e.to_string())
                        }
                        _ => Ok(()),
                    }
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
        };
        if let Err(message) = outcome {
            eprintln!("demo: {message}");
        }
        cx.background_executor().timer(FRAME).await;
    }
    let _ = cx.update(|_, cx| cx.quit());
}

fn main() -> anyhow::Result<()> {
    let mut vault_path: Option<PathBuf> = None;
    let mut script: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vault" => vault_path = Some(args.next().context("--vault needs a path")?.into()),
            "--script" => script = Some(args.next().context("--script needs a file")?.into()),
            other => bail!("unknown argument {other}"),
        }
    }
    let vault_path = vault_path.context("--vault is required")?;
    let steps = match &script {
        Some(path) => Some(parse_script(
            &std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?,
        )?),
        None => None,
    };
    let scripted = steps.is_some();
    let app_id =
        std::env::var("FOCUS_CAPTURE_CLASS").unwrap_or_else(|_| "focus-view-history".into());
    let window_size = match std::env::var("FOCUS_CAPTURE").as_deref() {
        Ok("sm") => size(px(1180.), px(760.)),
        _ => size(px(1600.), px(1000.)),
    };
    focus_i18n::init(None);
    let fonts_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/");
    let fonts: Vec<Vec<u8>> = [
        "Inter-Regular.ttf",
        "Inter-Medium.ttf",
        "Inter-SemiBold.ttf",
    ]
    .iter()
    .filter_map(|n| std::fs::read(format!("{fonts_dir}{n}")).ok())
    .collect();
    focus_md::mermaid::init_fonts(&fonts.iter().map(Vec::as_slice).collect::<Vec<_>>());
    let cache = PathBuf::from(
        std::env::var("FOCUS_DEMO_CACHE").unwrap_or_else(|_| "target/view-history-cache".into()),
    );
    let store = PathBuf::from(
        std::env::var("FOCUS_DEMO_HISTORY").unwrap_or_else(|_| "target/view-history-demo".into()),
    );
    let _ = std::fs::remove_dir_all(&store);
    let vault = Vault::open(
        &vault_path,
        OpenOptions {
            cache_dir: cache,
            ignore: Vec::new(),
            watch: false,
        },
    )?;
    vault.set_history(&store, HistoryConfig::default())?;

    Application::new().run(move |cx| {
        gpui_component::init(cx);
        if let Err(error) = focus_kit::init(Mode::Dark, cx) {
            eprintln!("demo: {error:#}");
            cx.quit();
            return;
        }
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let options = WindowOptions {
            focus: !scripted,
            show: true,
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                window_size,
                cx,
            ))),
            app_id: Some(app_id),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            let demo = cx.new(|_| Demo {
                vault: vault.clone(),
                dialog: None,
                log: Vec::new(),
                _subscription: None,
            });
            if let Some(steps) = steps {
                let weak = demo.downgrade();
                window
                    .spawn(cx, async move |cx| run(weak, steps, cx).await)
                    .detach();
            }
            cx.new(|cx| Root::new(demo, window, cx))
        });
        if let Err(error) = opened {
            eprintln!("demo: opening the window: {error:#}");
            cx.quit();
        }
    });
    Ok(())
}
