mod script;
mod seed;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, bail};
use focus_core::{OpenOptions, Vault};
use focus_kit::{ActiveTokens, Mode, Type, size as kit_size};
use focus_view_ops::{
    CommandLibrary, CredentialsTable, FindingsList, HostsTable, JournalPanel, NewEngagementDialog,
    OpsDashboard, OpsEvent, OpsIndicator, OpsSidebar, OpsState, ReportView,
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
    empty: bool,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = Args {
        vault: PathBuf::from("fixtures/vault"),
        script: None,
        empty: std::env::var_os("FOCUS_OPS_EMPTY").is_some(),
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--vault" => args.vault = iter.next().context("--vault needs a path")?.into(),
            "--script" => args.script = Some(iter.next().context("--script needs a file")?.into()),
            other => bail!(
                "unknown argument {other} (expected --vault <path>, --script <file>; FOCUS_OPS_EMPTY=1 starts without an engagement)"
            ),
        }
    }
    Ok(args)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Dashboard,
    Hosts,
    Findings,
    Credentials,
    Commands,
    Report,
}

pub struct Demo {
    pub state: Entity<OpsState>,
    pub screen: Screen,
    pub journal_open: bool,
    pub dashboard: Entity<OpsDashboard>,
    pub hosts: Entity<HostsTable>,
    pub findings: Entity<FindingsList>,
    pub credentials: Entity<CredentialsTable>,
    pub commands: Entity<CommandLibrary>,
    pub report: Entity<ReportView>,
    pub indicator: Entity<OpsIndicator>,
    vault: Arc<Vault>,
    pub sidebar: Entity<OpsSidebar>,
    pub journal: Entity<JournalPanel>,
    pub dialog: Option<Entity<NewEngagementDialog>>,
    last_event: String,
    _subscriptions: Vec<Subscription>,
}

impl Demo {
    fn new(
        args: &Args,
        ops_dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Demo> {
        let cache = focus_core::settings::cache_dir_for(&args.vault);
        let vault: Arc<Vault> = Vault::open(
            &args.vault,
            OpenOptions {
                cache_dir: cache,
                ignore: Vec::new(),
                watch: false,
            },
        )?;
        let state = cx.new(|cx| OpsState::new(ops_dir, None, cx));
        let dashboard = cx.new(|cx| OpsDashboard::new(state.clone(), cx));
        let hosts = cx.new(|cx| HostsTable::new(state.clone(), window, cx));
        let findings = cx.new(|cx| FindingsList::new(state.clone(), window, cx));
        let credentials = cx.new(|cx| CredentialsTable::new(state.clone(), window, cx));
        let commands =
            cx.new(|cx| CommandLibrary::new(vault.clone(), state.clone(), Vec::new(), window, cx));
        let report = cx.new(|cx| ReportView::new(state.clone(), window, cx));
        let indicator = cx.new(|cx| OpsIndicator::new(state.clone(), cx));
        let sidebar = cx.new(|cx| OpsSidebar::new(state.clone(), window, cx));
        let journal = cx.new(|cx| JournalPanel::new(state.clone(), window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&dashboard, window, Self::on_event),
            cx.subscribe_in(&hosts, window, Self::on_event),
            cx.subscribe_in(&findings, window, Self::on_event),
            cx.subscribe_in(&credentials, window, Self::on_event),
            cx.subscribe_in(&commands, window, Self::on_event),
            cx.subscribe_in(&sidebar, window, Self::on_event),
            cx.subscribe_in(&journal, window, Self::on_event),
            cx.subscribe_in(&report, window, Self::on_event),
            cx.subscribe_in(&indicator, window, Self::on_event),
        ];
        Ok(Demo {
            state,
            screen: Screen::Dashboard,
            journal_open: false,
            dashboard,
            hosts,
            findings,
            credentials,
            commands,
            report,
            indicator,
            vault,
            sidebar,
            journal,
            dialog: None,
            last_event: String::new(),
            _subscriptions: subscriptions,
        })
    }

    fn on_event<E: gpui::EventEmitter<OpsEvent>>(
        &mut self,
        _: &Entity<E>,
        event: &OpsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            OpsEvent::NewEngagementRequested => self.open_dialog(window, cx),
            OpsEvent::DialogClosed => self.dialog = None,
            OpsEvent::DashboardRequested => self.screen = Screen::Dashboard,
            _ => {}
        }
        self.last_event = format!("{event:?}");
        cx.notify();
    }

    pub fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.clone();
        let vault = self.vault.clone();
        let dialog =
            cx.new(|cx| NewEngagementDialog::new(state, window, cx).with_note_templates(vault));
        self._subscriptions
            .push(cx.subscribe_in(&dialog, window, Self::on_event));
        self.dialog = Some(dialog);
        cx.notify();
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let rtl = focus_i18n::direction() == focus_i18n::Direction::Rtl;
        let center: gpui::AnyElement = match self.screen {
            Screen::Dashboard => self.dashboard.clone().into_any_element(),
            Screen::Hosts => self.hosts.clone().into_any_element(),
            Screen::Findings => self.findings.clone().into_any_element(),
            Screen::Credentials => self.credentials.clone().into_any_element(),
            Screen::Commands => self.commands.clone().into_any_element(),
            Screen::Report => self.report.clone().into_any_element(),
        };
        div()
            .id("demo")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(t.bg)
            .child(
                div()
                    .flex_none()
                    .h(kit_size::TITLE_BAR)
                    .flex()
                    .items_center()
                    .px(kit_size::S3)
                    .border_b_1()
                    .border_color(t.line)
                    .child(self.indicator.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(rtl, |this| this.flex_row_reverse())
                    .child(div().flex_1().min_w_0().h_full().child(center))
                    .child(
                        div()
                            .flex_none()
                            .w(px(320.))
                            .h_full()
                            .when(rtl, |d| d.border_r_1())
                            .when(!rtl, |d| d.border_l_1())
                            .border_color(t.line)
                            .child(self.sidebar.clone()),
                    ),
            )
            .when(self.journal_open, |this| {
                this.child(
                    div()
                        .flex_none()
                        .h(px(220.))
                        .border_t_1()
                        .border_color(t.line)
                        .child(self.journal.clone()),
                )
            })
            .when(!self.last_event.is_empty(), |this| {
                this.child(
                    div()
                        .flex_none()
                        .px(kit_size::S3)
                        .h(kit_size::STATUS_BAR)
                        .flex()
                        .items_center()
                        .border_t_1()
                        .border_color(t.line)
                        .mono_sm()
                        .text_color(t.ink_3)
                        .child(self.last_event.clone()),
                )
            })
            .children(self.dialog.clone().map(|dialog| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(t.veil)
                    .child(dialog)
            }))
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
    let ops_dir = std::env::current_dir()?.join(if args.empty {
        "target/ops-demo-empty"
    } else {
        "target/ops-demo"
    });
    let _ = std::fs::remove_dir_all(&ops_dir);
    std::fs::create_dir_all(&ops_dir)?;
    if !args.empty {
        seed::seed_pentest(&ops_dir)?;
        seed::seed_exam(&ops_dir)?;
    }
    let app_id =
        std::env::var("FOCUS_CAPTURE_CLASS").unwrap_or_else(|_| "focus-view-ops-demo".into());
    let window_size = match std::env::var("FOCUS_CAPTURE").as_deref() {
        Ok("sm") => size(px(1180.), px(760.)),
        _ => size(px(1600.), px(1000.)),
    };
    focus_i18n::init(None);

    Application::new().run(move |cx: &mut App| {
        gpui_component::init(cx);
        if let Err(error) = focus_kit::init(Mode::Dark, cx) {
            eprintln!("demo: interface initialisation: {error:#}");
            cx.quit();
            return;
        }
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
                title: Some("Focus Ops".into()),
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
            window.set_window_title("Focus Ops");
            let demo = cx.new(|cx| {
                Demo::new(&args, ops_dir, window, cx).unwrap_or_else(|error| {
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
