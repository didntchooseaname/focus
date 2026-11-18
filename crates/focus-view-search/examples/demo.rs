use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use focus_core::{OpenOptions, Vault};
use focus_kit::{ActiveTokens, Mode, Type};
use focus_search::SearchIndex;
use focus_view_search::assist::{AssistBackend, ScriptedBackend};
use focus_view_search::{SearchEvent, SearchPanel, SearchServices};
use gpui::{
    AppContext, Application, AsyncWindowContext, Bounds, Context, Entity, InteractiveElement,
    IntoElement, Keystroke, ParentElement, Render, Styled, Subscription, WeakEntity, Window,
    WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::Root;

struct Demo {
    panel: Entity<SearchPanel>,
    width: f32,
    log: Vec<String>,
    _subscription: Subscription,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let rtl = focus_i18n::direction() == focus_i18n::Direction::Rtl;
        let bar = div()
            .flex_none()
            .w(px(self.width))
            .h_full()
            .border_color(t.line)
            .child(self.panel.clone());
        let bar = if rtl {
            bar.border_l_1()
        } else {
            bar.border_r_1()
        };
        let log = div()
            .flex()
            .flex_col()
            .flex_1()
            .p(px(16.))
            .mono_sm()
            .text_color(t.ink_3)
            .children(self.log.iter().rev().take(20).cloned());
        let root = div().id("demo").flex().size_full().bg(t.bg);
        if rtl {
            root.flex_row_reverse().child(bar).child(log)
        } else {
            root.child(bar).child(log)
        }
    }
}

#[derive(Debug)]
enum Step {
    Type(String),
    Settings(usize, bool),
    Key(String),
    Wait(u64),
    Shot(String),
    ShotNow(String),
    Theme(bool),
    Lang(String),
    Width(f32),
    Focus,
    EditQuery(usize),
    Replace(bool),
    Replacement(String),
    Apply,
    Settle,
    Typing(String),
    Stress(String),
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
            "theme" => Step::Theme(match rest {
                "dark" => true,
                "light" => false,
                _ => bail!("line {}: dark or light expected", index + 1),
            }),
            "lang" => Step::Lang(need()?),
            "width" => Step::Width(
                rest.parse()
                    .with_context(|| format!("line {}: pixels expected", index + 1))?,
            ),
            "settings" => {
                let (limit, accents) = rest.split_once(' ').with_context(|| {
                    format!("line {}: <limit> <accents 0|1> expected", index + 1)
                })?;
                Step::Settings(
                    limit
                        .parse()
                        .with_context(|| format!("line {}: limit expected", index + 1))?,
                    accents.trim() == "1",
                )
            }
            "focus" => Step::Focus,
            "replace" => Step::Replace(match rest {
                "on" => true,
                "off" => false,
                _ => bail!("line {}: on or off expected", index + 1),
            }),
            "replacement" => Step::Replacement(rest.to_string()),
            "apply" => Step::Apply,
            "editquery" => Step::EditQuery(
                rest.parse()
                    .with_context(|| format!("line {}: index expected", index + 1))?,
            ),
            "settle" => Step::Settle,
            "typing" => Step::Typing(need()?),
            "stress" => Step::Stress(need()?),
            "quit" => Step::Quit,
            other => bail!("line {}: unknown instruction {other}", index + 1),
        });
    }
    Ok(steps)
}

const FRAME: Duration = Duration::from_millis(20);

async fn settle(panel: &WeakEntity<SearchPanel>, cx: &mut AsyncWindowContext) {
    loop {
        if !panel
            .read_with(cx, |panel, _| panel.is_busy())
            .unwrap_or(false)
        {
            break;
        }
        cx.background_executor().timer(FRAME).await;
    }
    cx.background_executor().timer(FRAME * 2).await;
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

fn median(values: &mut [f32]) -> f32 {
    values.sort_by(|a, b| a.total_cmp(b));
    values[values.len() / 2]
}

async fn run(
    demo: WeakEntity<Demo>,
    panel: WeakEntity<SearchPanel>,
    steps: Vec<Step>,
    cx: &mut AsyncWindowContext,
) {
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
                    .update(|window, cx| {
                        window.dispatch_keystroke(keystroke, cx);
                    })
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
            Step::Lang(code) => focus_i18n::set_language(code).map_err(|e| e.to_string()),
            Step::Width(width) => demo
                .update(cx, |demo, cx| {
                    demo.width = *width;
                    cx.notify();
                })
                .map_err(|e| e.to_string()),
            Step::Settings(limit, accents) => cx
                .update(|window, cx| {
                    let settings = focus_view_search::SearchSettings {
                        respect_accents: *accents,
                        result_limit: *limit,
                    };
                    panel
                        .update(cx, |panel, cx| panel.set_settings(settings, window, cx))
                        .map_err(|e| e.to_string())
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Focus => cx
                .update(|window, cx| {
                    panel
                        .update(cx, |panel, cx| panel.focus_field(window, cx))
                        .map_err(|e| e.to_string())
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::EditQuery(at) => cx
                .update(|window, cx| {
                    panel
                        .update(cx, |panel, cx| panel.edit_query(*at, window, cx))
                        .map_err(|e| e.to_string())
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Replace(on) => cx
                .update(|window, cx| {
                    panel
                        .update(cx, |panel, cx| panel.set_replace_open(*on, window, cx))
                        .map_err(|e| e.to_string())
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Replacement(text) => cx
                .update(|window, cx| {
                    panel
                        .update(cx, |panel, cx| panel.set_replacement(text, window, cx))
                        .map_err(|e| e.to_string())
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Apply => cx
                .update(|window, cx| {
                    panel
                        .update(cx, |panel, cx| panel.apply_replace(window, cx))
                        .map_err(|e| e.to_string())
                })
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Step::Settle => {
                settle(&panel, cx).await;
                Ok(())
            }
            Step::Shot(name) => {
                settle(&panel, cx).await;
                shot(name, cx).await
            }
            Step::ShotNow(name) => shot(name, cx).await,
            Step::Typing(text) => {
                typing(&panel, text, cx).await;
                Ok(())
            }
            Step::Stress(query) => {
                stress(&panel, query, cx).await;
                Ok(())
            }
        };
        if let Err(message) = outcome {
            eprintln!("demo: {message}");
        }
        cx.background_executor().timer(FRAME).await;
    }
    let _ = cx.update(|_, cx| cx.quit());
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

async fn typing(panel: &WeakEntity<SearchPanel>, text: &str, cx: &mut AsyncWindowContext) {
    let mut results = Vec::new();
    let mut frames = Vec::new();
    for ch in text.chars() {
        let before = panel
            .read_with(cx, |panel, _| panel.last_timing())
            .unwrap_or_default();
        let _ = cx.update(|window, cx| type_text(&ch.to_string(), window, cx));
        let started = Instant::now();
        loop {
            let now = panel
                .read_with(cx, |panel, _| panel.last_timing())
                .unwrap_or_default();
            if now != before && now.frame_ms.is_some() {
                results.push(now.results_ms);
                frames.push(now.frame_ms.unwrap_or_default());
                break;
            }
            if started.elapsed() > Duration::from_secs(10) {
                eprintln!("demo: no result for {ch:?} after 10 s");
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
        }
        settle(panel, cx).await;
        cx.background_executor()
            .timer(Duration::from_millis(150))
            .await;
    }
    if !results.is_empty() {
        println!("typing {text:?}: per letter results_ms {results:.1?} frame_ms {frames:.1?}");
        println!(
            "typing {text:?}: median results {:.1} ms, median frame {:.1} ms (n={})",
            median(&mut results.clone()),
            median(&mut frames.clone()),
            results.len()
        );
    }
}

async fn stress(panel: &WeakEntity<SearchPanel>, query: &str, cx: &mut AsyncWindowContext) {
    let query = query.to_string();
    let _ =
        cx.update(|window, cx| panel.update(cx, |panel, cx| panel.set_query(&query, window, cx)));
    settle(panel, cx).await;
    let rows = panel
        .read_with(cx, |panel, _| panel.rows_len())
        .unwrap_or(0);
    let timing = panel
        .read_with(cx, |panel, _| panel.last_timing())
        .unwrap_or_default();
    let mut samples = Vec::new();
    for _ in 0..30 {
        let _ = cx.update(|window, cx| {
            window.refresh();
            let started = Instant::now();
            window.draw(cx).clear();
            samples.push(started.elapsed().as_secs_f32() * 1000.0);
        });
        cx.background_executor().timer(FRAME).await;
    }
    let first = samples.first().copied().unwrap_or_default();
    println!(
        "stress {query:?}: query to results {:.1} ms, query to first frame {:?} ms",
        timing.results_ms, timing.frame_ms
    );
    println!(
        "stress {query:?}: {rows} rows, first frame {first:.2} ms, median of {} frames {:.2} ms",
        samples.len(),
        median(&mut samples)
    );
}

fn main() -> anyhow::Result<()> {
    let mut vault_path: Option<PathBuf> = None;
    let mut script: Option<PathBuf> = None;
    let mut width = 272.0f32;
    let mut ai = true;
    let mut chunk_ms = 25u64;
    let mut fail = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vault" => vault_path = Some(args.next().context("--vault needs a path")?.into()),
            "--script" => script = Some(args.next().context("--script needs a file")?.into()),
            "--width" => {
                width = args
                    .next()
                    .context("--width needs a number")?
                    .parse()
                    .context("--width: number expected")?
            }
            "--no-ai" => ai = false,
            "--chunk-ms" => {
                chunk_ms = args
                    .next()
                    .context("--chunk-ms needs a number")?
                    .parse()
                    .context("--chunk-ms: number expected")?
            }
            "--fail" => fail = true,
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
        std::env::var("FOCUS_CAPTURE_CLASS").unwrap_or_else(|_| "focus-view-search".into());
    let window_size = match std::env::var("FOCUS_CAPTURE").as_deref() {
        Ok("sm") => size(px(1180.), px(760.)),
        _ => size(px(1600.), px(1000.)),
    };
    focus_i18n::init(None);

    Application::new().run(move |cx| {
        gpui_component::init(cx);
        focus_view_reading::bind(cx);
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
            let panel = cx.new(|cx| SearchPanel::new(window, cx));
            let demo = cx.new(|cx| {
                let subscription =
                    cx.subscribe(&panel, |this: &mut Demo, _, event: &SearchEvent, cx| {
                        this.log.push(format!("{event:?}"));
                        cx.notify();
                    });
                Demo {
                    panel: panel.clone(),
                    width,
                    log: Vec::new(),
                    _subscription: subscription,
                }
            });
            load_services(
                &panel,
                vault_path.clone(),
                Fake {
                    enabled: ai,
                    chunk_ms,
                    fail,
                },
                window,
                cx,
            );
            if let Some(steps) = steps {
                let (weak_demo, weak_panel) = (demo.downgrade(), panel.downgrade());
                window
                    .spawn(cx, async move |cx| {
                        run(weak_demo, weak_panel, steps, cx).await
                    })
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

struct Fake {
    enabled: bool,
    chunk_ms: u64,
    fail: bool,
}

fn load_services(
    panel: &Entity<SearchPanel>,
    vault_path: PathBuf,
    fake: Fake,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let (tx, rx) = smol::channel::unbounded::<(usize, usize)>();
    let cache = PathBuf::from(
        std::env::var("FOCUS_DEMO_CACHE").unwrap_or_else(|_| "target/view-search-cache".into()),
    );
    let task = cx.background_spawn(async move {
        let vault = Vault::open(
            &vault_path,
            OpenOptions {
                cache_dir: cache.clone(),
                ignore: Vec::new(),
                watch: false,
            },
        )?;
        let index = SearchIndex::open(vault.clone(), &cache, &|p| {
            let _ = tx.send_blocking((p.done, p.total));
        })?;
        anyhow::Ok((vault, index))
    });
    let weak = panel.downgrade();
    window
        .spawn(cx, async move |cx| {
            while let Ok((done, total)) = rx.recv().await {
                let _ = weak.update(cx, |panel, cx| panel.set_indexing_progress(done, total, cx));
            }
        })
        .detach();
    let weak = panel.downgrade();
    window
        .spawn(cx, async move |cx| match task.await {
            Ok((vault, index)) => {
                let backend = if fake.fail {
                    ScriptedBackend::failing(|| {
                        focus_ai::Error::Network("connection refused".into())
                    })
                } else {
                    ScriptedBackend::demo().with_chunk_delay(Duration::from_millis(fake.chunk_ms))
                };
                let assist: Option<Arc<dyn AssistBackend>> = fake
                    .enabled
                    .then(|| Arc::new(backend) as Arc<dyn AssistBackend>);
                let services = SearchServices {
                    vault,
                    index,
                    assist,
                    redaction: focus_ai::redact::Options {
                        secrets: true,
                        ip_addresses: false,
                        hostnames: false,
                    },
                };
                let _ = cx.update(|window, cx| {
                    weak.update(cx, |panel, cx| {
                        panel.set_services(Some(services), window, cx)
                    })
                });
            }
            Err(error) => eprintln!("demo: opening the vault: {error:#}"),
        })
        .detach();
}
