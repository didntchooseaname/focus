#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod actions;
mod ai;
mod attach;
mod bases;
mod capture;
mod center;
mod chrome;
mod compose;
mod crumbs;
mod editing;
mod editor;
mod errors;
mod find;
mod fonts;
mod format;
mod graph;
mod groups;
mod inplace;
mod instance;
mod ipc;
mod kit;
mod library;
mod live;
mod loose;
mod media;
mod moving;
mod notices;
mod obsidian;
mod ops;
mod outside;
mod palette;
mod panes;
mod private;
mod profile;
mod props;
mod script;
mod selbar;
mod session;
mod settings;
mod shell;
mod slash;
mod templates;
mod transfer;
mod update;
mod vault_views;
mod versions;
mod welcome;

use std::path::PathBuf;

use anyhow::{Context as _, bail};
use focus_kit::Mode;
use gpui::{
    AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use gpui_component::Root;

use shell::Shell;

#[global_allocator]
static ALLOCATOR: profile::Counting = profile::Counting;

struct Args {
    vault: Option<PathBuf>,
    script: Option<PathBuf>,
    paths: Vec<PathBuf>,
    addresses: Vec<String>,
    capture: Option<String>,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = Args {
        vault: None,
        script: None,
        paths: Vec::new(),
        addresses: Vec::new(),
        capture: None,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--vault" => args.vault = Some(iter.next().context("--vault expects a path")?.into()),
            "--script" => {
                args.script = Some(iter.next().context("--script expects a file")?.into())
            }
            "--capture" => {
                args.capture = Some(iter.next().context("--capture expects a text, or -")?)
            }
            other if other.starts_with("--") => bail!(
                "unknown argument: {other} (expected: --vault <path>, --script <file>, \
                 --capture <text|->, --version, [PATH|obsidian://...]...)"
            ),
            address if obsidian::is_obsidian(address) => args.addresses.push(address.to_string()),
            path => args
                .paths
                .push(std::path::absolute(path).with_context(|| format!("the path {path}"))?),
        }
    }
    Ok(args)
}

fn main() -> anyhow::Result<()> {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == focus_view_media::pdf::worker::WORKER_ARG)
    {
        std::process::exit(focus_view_media::pdf::worker::serve_stdio());
    }
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == "--version" || a == "-V")
    {
        println!("focus {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    profile::start();
    for diagnostic in focus_i18n::init(None) {
        eprintln!("focus: translation file: {diagnostic:?}");
    }
    let args = parse_args()?;
    if let Some(text) = &args.capture {
        std::process::exit(capture::run_command(text));
    }
    let steps = match &args.script {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading the script {}", path.display()))?;
            Some(script::parse(&text).map_err(anyhow::Error::msg)?)
        }
        None => None,
    };
    let scripted = steps.is_some();
    let socket = if scripted {
        None
    } else {
        instance::socket_path()
    };
    if let Some(socket) = &socket
        && instance::hand_over(socket, &args.paths, &args.addresses)
    {
        return Ok(());
    }
    let listener = socket
        .as_deref()
        .and_then(|socket| match instance::bind(socket) {
            Ok(listener) => Some((listener, socket.to_path_buf())),
            Err(error) => {
                eprintln!("focus: instance socket {}: {error}", socket.display());
                None
            }
        });
    let (store, settings_error) = session::Store::load(scripted);
    let language = match store.settings.language.as_deref() {
        Some("system") | None => focus_i18n::detect_system(),
        Some(code) => code.to_string(),
    };
    if let Err(error) = focus_i18n::set_language(&language) {
        eprintln!("focus: language: {error}");
    }
    let app_id = std::env::var("FOCUS_CAPTURE_CLASS").unwrap_or_else(|_| "focus".into());
    let window_size = match std::env::var("FOCUS_CAPTURE").as_deref() {
        Ok("sm") => size(px(1180.), px(760.)),
        _ => size(px(1600.), px(1000.)),
    };
    let (vault, open) = loose::route(args.vault, args.paths, &store.settings.recent_vaults);
    let addresses = args.addresses;

    Application::new().run(move |cx| {
        gpui_component::init(cx);
        gpui_component::set_locale(&focus_i18n::language());
        if let Err(error) = focus_kit::init(Mode::Dark, cx) {
            eprintln!("focus: interface initialization: {error:#}");
            cx.quit();
            return;
        }
        focus_md::mermaid::init_fonts(&focus_kit::fonts::text_faces());
        actions::bind(cx);
        focus_view_reading::bind(cx);
        focus_view_ai::bind(cx);
        focus_view_vault::bind(cx);
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
                title: Some(focus_i18n::t("ui-app-name").into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                window_size,
                cx,
            ))),
            window_min_size: Some(size(px(720.), px(480.))),
            app_id: Some(app_id),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            window.set_window_title(&focus_i18n::t("ui-app-name"));
            let shell = cx.new(|cx| Shell::new(vault, open, scripted, store, window, cx));
            shell.update(cx, |shell, cx| {
                for address in &addresses {
                    shell.follow_obsidian(address, window, cx);
                }
            });
            if let Some(detail) = settings_error {
                let message = focus_i18n::t_args(
                    "errors-settings-read",
                    &[("detail", detail.as_str().into())],
                );
                shell.update(cx, |shell, cx| {
                    shell.notify_user(focus_kit::Tone::Danger, message, cx)
                });
            }
            if let Some((listener, socket)) = listener {
                shell.update(cx, |shell, cx| {
                    instance::serve(shell, listener, socket, window, cx)
                });
            }
            profile::mark("window created");
            if let Some(steps) = steps {
                let weak = shell.downgrade();
                window
                    .spawn(cx, async move |cx| script::run(weak, steps, cx).await)
                    .detach();
            }
            cx.new(|cx| Root::new(shell, window, cx))
        });
        match opened {
            Ok(_) if !scripted => cx.activate(true),
            Ok(_) => {}
            Err(error) => {
                eprintln!("focus: opening the window: {error:#}");
                cx.quit();
            }
        }
    });
    update::relaunch();
    Ok(())
}
