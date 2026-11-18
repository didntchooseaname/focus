use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, bail};
use focus_core::{AttachmentPolicy, OpenOptions, Vault};
use focus_kit::{ActiveTokens, Mode, Type};
use focus_view_transfer::export::Format;
use focus_view_transfer::{
    AttachmentsDialog, AttachmentsMode, ExportDialog, ImportDialog, TransferEvent, TransferServices,
};
use gpui::{
    AnyView, AppContext, Application, AsyncWindowContext, Bounds, Context, Entity,
    InteractiveElement, IntoElement, Keystroke, ParentElement, Render, Styled, Subscription,
    WeakEntity, Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::Root;

enum Current {
    None,
    Export(Entity<ExportDialog>),
    Import(Entity<ImportDialog>),
    Attachments(Entity<AttachmentsDialog>),
}

struct Demo {
    services: TransferServices,
    current: Current,
    log: Vec<String>,
    _subscription: Option<Subscription>,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let dialog: Option<AnyView> = match &self.current {
            Current::None => None,
            Current::Export(d) => Some(d.clone().into()),
            Current::Import(d) => Some(d.clone().into()),
            Current::Attachments(d) => Some(d.clone().into()),
        };
        div()
            .id("demo")
            .relative()
            .size_full()
            .bg(t.bg)
            .p(px(16.))
            .mono_sm()
            .text_color(t.ink_3)
            .children(self.log.iter().rev().take(20).cloned())
            .children(dialog)
    }
}

#[derive(Debug)]
enum Step {
    Type(String),
    Key(String),
    Wait(u64),
    Shot(String),
    ShotNow(String),
    Theme(bool),
    Lang(String),
    Settle,
    Dialog(String, Option<String>),
    Scope(usize),
    Format(String),
    Destination(String),
    Source(String),
    NoPdf,
    Go,
    Copy(bool, String),
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
        let need = || {
            if rest.is_empty() {
                bail!("line {}: argument missing for {word}", index + 1)
            } else {
                Ok(rest.to_string())
            }
        };
        steps.push(match word {
            "type" => Step::Type(need()?),
            "key" => Step::Key(need()?),
            "wait" => Step::Wait(
                rest.parse()
                    .with_context(|| format!("line {}: milliseconds expected", index + 1))?,
            ),
            "shot" => Step::Shot(need()?),
            "shotnow" => Step::ShotNow(need()?),
            "theme" => Step::Theme(rest == "dark"),
            "lang" => Step::Lang(need()?),
            "settle" => Step::Settle,
            "dialog" => {
                let (kind, note) = rest
                    .split_once(char::is_whitespace)
                    .map_or((rest, None), |(k, n)| (k, Some(n.trim().to_string())));
                Step::Dialog(kind.to_string(), note)
            }
            "scope" => Step::Scope(
                rest.parse()
                    .with_context(|| format!("line {}: index expected", index + 1))?,
            ),
            "format" => Step::Format(need()?),
            "destination" => Step::Destination(need()?),
            "source" => Step::Source(need()?),
            "nopdf" => Step::NoPdf,
            "go" => Step::Go,
            "copyhtml" => Step::Copy(true, need()?),
            "copymd" => Step::Copy(false, need()?),
            "quit" => Step::Quit,
            other => bail!("line {}: unknown instruction {other}", index + 1),
        });
    }
    Ok(steps)
}

const FRAME: Duration = Duration::from_millis(20);

fn busy(demo: &Entity<Demo>, cx: &gpui::App) -> bool {
    match &demo.read(cx).current {
        Current::None => false,
        Current::Export(d) => d.read(cx).is_busy(),
        Current::Import(d) => d.read(cx).is_busy(),
        Current::Attachments(d) => d.read(cx).is_busy(),
    }
}

async fn settle(demo: &WeakEntity<Demo>, cx: &mut AsyncWindowContext) {
    cx.background_executor().timer(FRAME * 3).await;
    loop {
        let busy = demo
            .upgrade()
            .map(|d| cx.update(|_, cx| busy(&d, cx)).unwrap_or(false))
            .unwrap_or(false);
        if !busy {
            break;
        }
        cx.background_executor().timer(FRAME).await;
    }
    cx.background_executor().timer(FRAME * 3).await;
}

fn type_text(text: &str, window: &mut Window, cx: &mut gpui::App) {
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

fn open_dialog(
    demo: &WeakEntity<Demo>,
    kind: &str,
    note: Option<&str>,
    window: &mut Window,
    cx: &mut gpui::App,
) -> Result<(), String> {
    let demo = demo.upgrade().ok_or("window closed")?;
    let services = demo.read(cx).services.clone();
    let note_id = note.and_then(|p| services.vault.note_by_path(Path::new(p)));
    let folder = note.and_then(|p| Path::new(p).parent().map(Path::to_path_buf));
    let (current, subscription): (Current, Subscription) = match kind {
        "export" | "export-plain" => {
            let options = if kind == "export" {
                focus_view_transfer::export::Options::default()
            } else {
                focus_view_transfer::export::Options {
                    dark: true,
                    embed_images: false,
                    embed_fonts: false,
                    ..Default::default()
                }
            };
            let dialog = cx.new(|cx| {
                ExportDialog::new(services, note_id, folder, window, cx).with_options(options)
            });
            let sub = cx.subscribe(&dialog, log_event(&demo));
            (Current::Export(dialog), sub)
        }
        "import" => {
            let dialog = cx.new(|cx| ImportDialog::new(services, "Imports", window, cx));
            let sub = cx.subscribe(&dialog, log_event(&demo));
            (Current::Import(dialog), sub)
        }
        "tidy" => {
            let dialog = cx.new(|cx| {
                AttachmentsDialog::new(services, AttachmentsMode::Tidy(None), window, cx)
            });
            let sub = cx.subscribe(&dialog, log_event(&demo));
            (Current::Attachments(dialog), sub)
        }
        "orphans" => {
            let dialog =
                cx.new(|cx| AttachmentsDialog::new(services, AttachmentsMode::Orphans, window, cx));
            let sub = cx.subscribe(&dialog, log_event(&demo));
            (Current::Attachments(dialog), sub)
        }
        other => return Err(format!("unknown dialog {other}")),
    };
    demo.update(cx, |demo, cx| {
        demo.current = current;
        demo._subscription = Some(subscription);
        cx.notify();
    });
    Ok(())
}

fn log_event<E: 'static>(
    demo: &Entity<Demo>,
) -> impl Fn(Entity<E>, &TransferEvent, &mut gpui::App) + 'static {
    let weak = demo.downgrade();
    move |_, event, cx| {
        weak.update(cx, |demo, cx| {
            demo.log.push(format!("{event:?}"));
            cx.notify();
        })
        .ok();
    }
}

fn with_export(
    demo: &Entity<Demo>,
    cx: &mut gpui::App,
    f: impl FnOnce(&Entity<ExportDialog>, &mut gpui::App),
) -> Result<(), String> {
    match &demo.read(cx).current {
        Current::Export(d) => {
            let d = d.clone();
            f(&d, cx);
            Ok(())
        }
        _ => Err("no export dialog open".into()),
    }
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
            Step::Type(text) => cx
                .update(|window, cx| type_text(text, window, cx))
                .map_err(|e| e.to_string()),
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
            Step::Dialog(kind, note) => cx
                .update(|window, cx| open_dialog(&demo, kind, note.as_deref(), window, cx))
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Scope(index) => cx
                .update(|_, cx| {
                    demo.upgrade()
                        .ok_or("window closed".to_string())
                        .and_then(|d| {
                            with_export(&d, cx, |dialog, cx| {
                                dialog.update(cx, |dialog, cx| dialog.set_scope(*index, cx))
                            })
                        })
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Format(name) => {
                let format = match name.as_str() {
                    "html" => Format::Html,
                    "markdown" => Format::Markdown,
                    "pdf" => Format::Pdf,
                    "zip" => Format::Zip,
                    other => {
                        eprintln!("demo: unknown format {other}");
                        continue;
                    }
                };
                cx.update(|_, cx| {
                    demo.upgrade()
                        .ok_or("window closed".to_string())
                        .and_then(|d| {
                            with_export(&d, cx, |dialog, cx| {
                                dialog.update(cx, |dialog, cx| dialog.set_format(format, cx))
                            })
                        })
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r)
            }
            Step::Destination(path) => cx
                .update(|window, cx| {
                    demo.upgrade()
                        .ok_or("window closed".to_string())
                        .and_then(|d| {
                            with_export(&d, cx, |dialog, cx| {
                                dialog.update(cx, |dialog, cx| {
                                    dialog.set_destination(path, window, cx)
                                })
                            })
                        })
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::NoPdf => cx
                .update(|_, cx| {
                    demo.upgrade()
                        .ok_or("window closed".to_string())
                        .and_then(|d| {
                            with_export(&d, cx, |dialog, cx| {
                                dialog.update(cx, |dialog, cx| dialog.set_pdf_available(false, cx))
                            })
                        })
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Source(path) => cx
                .update(
                    |window, cx| match demo.upgrade().map(|d| d.read(cx).current_import()) {
                        Some(Some(dialog)) => {
                            dialog.update(cx, |dialog, cx| dialog.set_source(path, window, cx));
                            Ok(())
                        }
                        _ => Err("no import dialog open".to_string()),
                    },
                )
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Go => cx
                .update(|window, cx| {
                    let demo = demo.upgrade().ok_or("window closed".to_string())?;
                    match &demo.read(cx).current {
                        Current::Export(d) => d.clone().update(cx, |d, cx| d.start(window, cx)),
                        Current::Import(d) => d.clone().update(cx, |d, cx| d.start(window, cx)),
                        Current::Attachments(d) => {
                            d.clone().update(cx, |d, cx| d.apply(window, cx))
                        }
                        Current::None => return Err("no dialog open".to_string()),
                    }
                    Ok(())
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Copy(html, path) => cx
                .update(|_, cx| {
                    let demo = demo.upgrade().ok_or("window closed".to_string())?;
                    let vault = demo.read(cx).services.vault.clone();
                    let id = vault
                        .note_by_path(Path::new(path))
                        .ok_or_else(|| format!("no note {path}"))?;
                    let started = std::time::Instant::now();
                    let text = if *html {
                        focus_view_transfer::export::copy_html(&vault, id)
                    } else {
                        focus_view_transfer::export::copy_markdown(&vault, id)
                    }
                    .map_err(|e| format!("{e:#}"))?;
                    println!(
                        "{} of {path}: {} bytes in {:.1} ms",
                        if *html { "html" } else { "markdown" },
                        text.len(),
                        started.elapsed().as_secs_f32() * 1000.0
                    );
                    Ok(())
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

impl Demo {
    fn current_import(&self) -> Option<Entity<ImportDialog>> {
        match &self.current {
            Current::Import(d) => Some(d.clone()),
            _ => None,
        }
    }
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
        std::env::var("FOCUS_CAPTURE_CLASS").unwrap_or_else(|_| "focus-view-transfer".into());
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
        "Inter-Bold.ttf",
    ]
    .iter()
    .filter_map(|n| std::fs::read(format!("{fonts_dir}{n}")).ok())
    .collect();
    focus_md::mermaid::init_fonts(&fonts.iter().map(Vec::as_slice).collect::<Vec<_>>());
    let cache = PathBuf::from(
        std::env::var("FOCUS_DEMO_CACHE").unwrap_or_else(|_| "target/view-transfer-cache".into()),
    );
    let vault = Vault::open(
        &vault_path,
        OpenOptions {
            cache_dir: cache,
            ignore: Vec::new(),
            watch: false,
        },
    )?;
    let services = TransferServices {
        vault,
        default_directory: PathBuf::from("target/transfer-out"),
        attachment_policy: AttachmentPolicy::default(),
    };

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
                services: services.clone(),
                current: Current::None,
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
