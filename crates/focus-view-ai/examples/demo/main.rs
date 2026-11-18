mod scenarios;
mod script;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, bail};
use focus_core::{OpenOptions, Vault};
use focus_kit::{ActiveTokens, Mode, Type, size as kit_size};
use focus_mcp::VaultTools;
use focus_search::SearchIndex;
use focus_view_ai::fake::FakeBackend;
use focus_view_ai::{
    AiEvent, AiPanel, AiSettings, ChatBackend, DeepSeek, EditorContext, NoteSnapshot, Services,
    Snapshot, WriteGate,
};
use gpui::{
    App, AppContext, Application, Bounds, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, prelude::FluentBuilder, px, size,
};
use gpui_component::Root;

struct Args {
    vault: PathBuf,
    script: Option<PathBuf>,
    width: f32,
    real: bool,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = Args {
        vault: PathBuf::from("fixtures/vault"),
        script: None,
        width: 320.,
        real: false,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--vault" => args.vault = iter.next().context("--vault needs a path")?.into(),
            "--script" => args.script = Some(iter.next().context("--script needs a file")?.into()),
            "--width" => {
                args.width = iter
                    .next()
                    .context("--width needs a number")?
                    .parse()
                    .context("--width needs a number")?
            }
            "--real" => args.real = true,
            other => bail!(
                "unknown argument {other} (expected --vault <path>, --script <file>, --width <px>, --real)"
            ),
        }
    }
    Ok(args)
}

struct DemoEditor(Rc<RefCell<Snapshot>>);

impl EditorContext for DemoEditor {
    fn snapshot(&self, _: &App) -> Snapshot {
        self.0.borrow().clone()
    }
}

pub struct Demo {
    panel: Entity<AiPanel>,
    snapshot: Rc<RefCell<Snapshot>>,
    vault: Arc<Vault>,
    pub width: f32,
    last_event: String,
    _subscription: Subscription,
}

impl Demo {
    fn new(args: &Args, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<Demo> {
        let cache = focus_core::settings::cache_dir_for(&args.vault);
        let vault = Vault::open(
            &args.vault,
            OpenOptions {
                cache_dir: cache.clone(),
                ignore: Vec::new(),
                watch: false,
            },
        )?;
        let index = SearchIndex::open(vault.clone(), &cache, &|_| {})?;
        let gate = WriteGate::new();
        let tools = VaultTools::new(vault.clone(), index.clone())
            .with_extended_tools()
            .with_confirmation(gate.confirm());
        let snapshot = Rc::new(RefCell::new(Snapshot::default()));
        let settings = AiSettings {
            redact_ips: true,
            ..AiSettings::default()
        };
        let backend: Arc<dyn ChatBackend> = if args.real {
            Arc::new(
                DeepSeek::connect(true, &settings.base_url)
                    .context("--real needs DEEPSEEK_API_KEY")?,
            )
        } else {
            Arc::new(
                FakeBackend::new(scenarios::answer("en"), Duration::from_millis(28))
                    .with_models(scenarios::models()),
            )
        };
        let history_dir = std::env::current_dir()?.join("target/ai-demo-history");
        let services = Services {
            vault: vault.clone(),
            index,
            editor: Rc::new(DemoEditor(snapshot.clone())),
            tools: Arc::new(tools),
            gate,
            history_dir: Some(history_dir),
        };
        let panel = cx.new(|cx| AiPanel::new(services, Some(backend), settings, window, cx));
        let subscription = cx.subscribe(&panel, |this: &mut Demo, _, event: &AiEvent, cx| {
            this.last_event = format!("{event:?}");
            cx.notify();
        });
        Ok(Demo {
            panel,
            snapshot,
            vault,
            width: args.width,
            last_event: String::new(),
            _subscription: subscription,
        })
    }

    fn open_note(&mut self, relative: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let id = self
            .vault
            .note_by_path(Path::new(relative))
            .ok_or_else(|| format!("note not found: {relative}"))?;
        let meta = self.vault.note(id).ok_or("note not found")?;
        let text = self.vault.read(id).map_err(|e| e.to_string())?;
        self.snapshot.borrow_mut().note = Some(NoteSnapshot {
            path: meta.path.to_path_buf(),
            title: meta.title.to_string(),
            tags: meta.tags.clone(),
            text: text.into(),
        });
        self.snapshot.borrow_mut().selection = None;
        self.panel.update(cx, |panel, cx| panel.refresh_context(cx));
        Ok(())
    }

    fn select(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        self.snapshot.borrow_mut().selection = text.map(Into::into);
        self.panel.update(cx, |panel, cx| panel.refresh_context(cx));
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let rtl = focus_i18n::direction() == focus_i18n::Direction::Rtl;
        div()
            .id("demo")
            .flex()
            .size_full()
            .bg(t.bg)
            .when(rtl, |this| this.flex_row_reverse())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .p(kit_size::S6)
                    .mono_sm()
                    .text_color(t.ink_3)
                    .child(self.last_event.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(self.width))
                    .h_full()
                    .border_color(t.line)
                    .when(rtl, |this| this.border_r_1())
                    .when(!rtl, |this| this.border_l_1())
                    .child(self.panel.clone()),
            )
    }
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    let steps = match &args.script {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            Some(script::parse(&text).map_err(anyhow::Error::msg)?)
        }
        None => None,
    };
    let scripted = steps.is_some();
    let app_id =
        std::env::var("FOCUS_CAPTURE_CLASS").unwrap_or_else(|_| "focus-view-ai-demo".into());
    let window_size = match std::env::var("FOCUS_CAPTURE").as_deref() {
        Ok("sm") => size(px(1180.), px(760.)),
        _ => size(px(1600.), px(1000.)),
    };
    focus_i18n::init(None);

    Application::new().run(move |cx| {
        gpui_component::init(cx);
        if let Err(error) = focus_kit::init(Mode::Dark, cx) {
            eprintln!("demo: interface initialisation: {error:#}");
            cx.quit();
            return;
        }
        focus_view_ai::bind(cx);
        focus_view_reading::bind(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let options = WindowOptions {
            focus: !scripted,
            show: true,
            titlebar: Some(TitlebarOptions {
                title: Some("Focus AI panel".into()),
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
            window.set_window_title("Focus AI panel");
            let demo = cx.new(|cx| {
                Demo::new(&args, window, cx).unwrap_or_else(|error| {
                    eprintln!("demo: {error:#}");
                    std::process::exit(1)
                })
            });
            if let Some(steps) = steps {
                let weak = demo.downgrade();
                window
                    .spawn(cx, async move |cx| script::run(weak, steps, cx).await)
                    .detach();
            }
            cx.new(|cx| Root::new(demo, window, cx))
        });
        match opened {
            Ok(_) if !scripted => cx.activate(true),
            Ok(_) => {}
            Err(error) => {
                eprintln!("demo: opening the window: {error:#}");
                cx.quit();
            }
        }
    });
    Ok(())
}
