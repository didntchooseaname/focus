use std::time::Duration;

use focus_kit::Mode;
use focus_view_ai::AiPanel;
use focus_view_ai::fake::FakeBackend;
use gpui::{AppContext as _, AsyncWindowContext, Keystroke, WeakEntity, Window};

use crate::{Demo, scenarios};

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Open(String),
    Select(Option<String>),
    Scenario(String),
    NoKey,
    Send(String),
    Action(String),
    Toggle(usize, usize),
    Menu(String),
    History,
    Resume(usize),
    Resolve(bool),
    Stop,
    NewConversation,
    Theme(bool),
    Lang(String),
    Width(f32),
    Focus,
    Key(String),
    Type(String),
    Wait(u64),
    Settle,
    Shot(String),
    Quit,
}

pub fn parse(text: &str) -> Result<Vec<(usize, Step)>, String> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(w, r)| (w, r.trim()));
        let fail = |message: &str| format!("line {number}: {message}");
        let need = |what: &str| {
            if rest.is_empty() {
                Err(fail(&format!("missing argument: {what}")))
            } else {
                Ok(rest.to_string())
            }
        };
        let step = match word {
            "open" => Step::Open(need("relative path")?),
            "select" => Step::Select(Some(need("text")?)),
            "deselect" => Step::Select(None),
            "scenario" => Step::Scenario(need("name")?),
            "nokey" => Step::NoKey,
            "send" => Step::Send(need("text")?),
            "action" => Step::Action(need("identifier")?),
            "toggle" => {
                let (a, b) = rest
                    .split_once(' ')
                    .ok_or_else(|| fail("expected: toggle <entry> <part>"))?;
                Step::Toggle(
                    a.parse().map_err(|_| fail("entry must be a number"))?,
                    b.trim()
                        .parse()
                        .map_err(|_| fail("part must be a number"))?,
                )
            }
            "menu" => Step::Menu(need("actions, models or none")?),
            "history" => Step::History,
            "resume" => Step::Resume(rest.parse().map_err(|_| fail("expected a position"))?),
            "apply" => Step::Resolve(true),
            "reject" => Step::Resolve(false),
            "stop" => Step::Stop,
            "newconv" => Step::NewConversation,
            "theme" => Step::Theme(match rest {
                "dark" => true,
                "light" => false,
                _ => return Err(fail("expected dark or light")),
            }),
            "lang" => Step::Lang(need("language code")?),
            "width" => Step::Width(
                rest.parse()
                    .map_err(|_| fail("expected a width in pixels"))?,
            ),
            "focus" => Step::Focus,
            "key" => {
                let keys = need("key")?;
                Keystroke::parse(&keys).map_err(|e| fail(&format!("unreadable key: {e}")))?;
                Step::Key(keys)
            }
            "type" => Step::Type(need("text")?),
            "wait" => Step::Wait(rest.parse().map_err(|_| fail("expected milliseconds"))?),
            "settle" => Step::Settle,
            "shot" => Step::Shot(need("name")?),
            "quit" => Step::Quit,
            other => return Err(fail(&format!("unknown instruction: {other}"))),
        };
        steps.push((number, step));
    }
    Ok(steps)
}

const FRAME: Duration = Duration::from_millis(20);

pub async fn run(demo: WeakEntity<Demo>, steps: Vec<(usize, Step)>, cx: &mut AsyncWindowContext) {
    for (line, step) in steps {
        let outcome = match &step {
            Step::Quit => break,
            Step::Wait(ms) => {
                cx.background_executor()
                    .timer(Duration::from_millis(*ms))
                    .await;
                Ok(())
            }
            Step::Settle => {
                settle(&demo, cx).await;
                Ok(())
            }
            Step::Key(keys) => match Keystroke::parse(keys) {
                Ok(keystroke) => cx
                    .update(|window, cx| {
                        window.dispatch_keystroke(keystroke, cx);
                    })
                    .map_err(|e| e.to_string()),
                Err(error) => Err(error.to_string()),
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
            other => match demo.update_in(cx, |demo, window, cx| apply(demo, other, window, cx)) {
                Ok(result) => result,
                Err(error) => Err(error.to_string()),
            },
        };
        if let Err(message) = outcome {
            eprintln!("demo: line {line}: {message}");
        }
        cx.background_executor().timer(FRAME).await;
    }
    let _ = cx.update(|_, cx| cx.quit());
}

async fn settle(demo: &WeakEntity<Demo>, cx: &mut AsyncWindowContext) {
    for _ in 0..1500 {
        let running = demo
            .read_with(cx, |demo, cx| demo.panel.read(cx).is_running())
            .unwrap_or(false);
        if !running {
            break;
        }
        cx.background_executor().timer(FRAME).await;
    }
}

async fn shot(
    demo: &WeakEntity<Demo>,
    name: &str,
    cx: &mut AsyncWindowContext,
) -> Result<(), String> {
    let command =
        std::env::var("FOCUS_SHOT_CMD").map_err(|_| "FOCUS_SHOT_CMD is not set".to_string())?;
    cx.update(|window, _| window.refresh())
        .map_err(|e| e.to_string())?;
    for _ in 0..4 {
        cx.background_executor().timer(FRAME * 2).await;
    }
    let _ = demo;
    let name = name.to_string();
    let status = cx
        .background_spawn(async move { std::process::Command::new(&command).arg(&name).status() })
        .await
        .map_err(|e| format!("running FOCUS_SHOT_CMD: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("FOCUS_SHOT_CMD failed ({status})"))
    }
}

fn apply(
    demo: &mut Demo,
    step: &Step,
    window: &mut Window,
    cx: &mut gpui::Context<Demo>,
) -> Result<(), String> {
    let panel = demo.panel.clone();
    match step {
        Step::Open(path) => demo.open_note(path, cx)?,
        Step::Select(text) => demo.select(text.clone(), cx),
        Step::Scenario(name) => {
            let scripts = scenarios::named(name, &focus_i18n::language())
                .ok_or_else(|| format!("unknown scenario: {name}"))?;
            let backend = std::sync::Arc::new(
                FakeBackend::new(scripts, Duration::from_millis(28))
                    .with_models(scenarios::models()),
            );
            panel.update(cx, |panel: &mut AiPanel, cx| {
                panel.set_backend(Some(backend), cx)
            });
        }
        Step::NoKey => panel.update(cx, |panel, cx| panel.set_backend(None, cx)),
        Step::Send(text) => panel.update(cx, |panel, cx| panel.ask(text, window, cx)),
        Step::Action(id) => panel.update(cx, |panel, cx| panel.run_action(id, cx)),
        Step::Toggle(entry, part) => {
            panel.update(cx, |panel, cx| panel.toggle_part(*entry, *part, cx))
        }
        Step::Menu(which) => match which.as_str() {
            "actions" => panel.update(cx, |panel, cx| panel.show_actions(cx)),
            "models" => panel.update(cx, |panel, cx| panel.show_models(cx)),
            "none" => panel.update(cx, |panel, cx| {
                panel.dismiss(cx);
            }),
            other => return Err(format!("unknown menu: {other}")),
        },
        Step::History => panel.update(cx, |panel, cx| panel.toggle_history(cx)),
        Step::Resume(n) => panel.update(cx, |panel, cx| panel.resume_nth(*n, cx)),
        Step::Resolve(apply) => {
            panel.update(cx, |panel, cx| panel.answer_pending_write(*apply, cx))
        }
        Step::Stop => panel.update(cx, |panel, cx| panel.stop(cx)),
        Step::NewConversation => panel.update(cx, |panel, cx| panel.new_conversation(cx)),
        Step::Theme(dark) => focus_kit::theme::apply(
            if *dark { Mode::Dark } else { Mode::Light },
            Some(window),
            cx,
        )
        .map_err(|e| e.to_string())?,
        Step::Lang(code) => {
            focus_i18n::set_language(code).map_err(|e| e.to_string())?;
            panel.update(cx, |panel, cx| panel.refresh_context(cx));
        }
        Step::Width(px) => demo.width = *px,
        Step::Focus => panel.update(cx, |panel, cx| panel.focus_input(window, cx)),
        Step::Key(_)
        | Step::Type(_)
        | Step::Wait(_)
        | Step::Settle
        | Step::Shot(_)
        | Step::Quit => {}
    }
    cx.notify();
    Ok(())
}
