use std::time::Duration;

use focus_kit::Mode;
use gpui::{AppContext as _, AsyncWindowContext, Keystroke, WeakEntity, Window};

use crate::{Demo, Screen, seed};

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Screen(Screen),
    Journal(bool),
    Dialog(bool),
    Engagement(String),
    PasteScan,
    ImportRun,
    Sort(String),
    Host(usize),
    ReportBuild,
    ReportWrite,
    Evidence,
    Variable(String, String),
    Calculator,
    EditFinding,
    ExamStart,
    ExamItem(usize),
    Finding(usize),
    Reveal(usize),
    NewFinding,
    Focus(String),
    Theme(bool),
    Lang(String),
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
            "screen" => Step::Screen(match rest {
                "dashboard" => Screen::Dashboard,
                "hosts" => Screen::Hosts,
                "findings" => Screen::Findings,
                "credentials" => Screen::Credentials,
                "commands" => Screen::Commands,
                "report" => Screen::Report,
                _ => {
                    return Err(fail(
                        "expected dashboard, hosts, findings, credentials or commands",
                    ));
                }
            }),
            "journal" => Step::Journal(rest == "on"),
            "dialog" => Step::Dialog(rest == "open"),
            "engagement" => Step::Engagement(need("start of the name")?),
            "paste-scan" => Step::PasteScan,
            "import-run" => Step::ImportRun,
            "sort" => Step::Sort(need("column")?),
            "host" => Step::Host(rest.parse().map_err(|_| fail("expected a position"))?),
            "report-build" => Step::ReportBuild,
            "report-write" => Step::ReportWrite,
            "evidence" => Step::Evidence,
            "var" => {
                let (name, value) = rest
                    .split_once(char::is_whitespace)
                    .ok_or_else(|| fail("expected: var <NAME> <value>"))?;
                Step::Variable(name.to_string(), value.trim().to_string())
            }
            "finding" => Step::Finding(rest.parse().map_err(|_| fail("expected a position"))?),
            "reveal" => Step::Reveal(rest.parse().map_err(|_| fail("expected a position"))?),
            "calculator" => Step::Calculator,
            "edit-finding" => Step::EditFinding,
            "exam-start" => Step::ExamStart,
            "exam-item" => Step::ExamItem(rest.parse().map_err(|_| fail("expected a position"))?),
            "new-finding" => Step::NewFinding,
            "focus" => Step::Focus(need("search, journal or entry")?),
            "theme" => Step::Theme(match rest {
                "dark" => true,
                "light" => false,
                _ => return Err(fail("expected dark or light")),
            }),
            "lang" => Step::Lang(need("language code")?),
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
            other => {
                settle(&demo, cx).await;
                match demo.update_in(cx, |demo, window, cx| apply(demo, other, window, cx)) {
                    Ok(result) => result,
                    Err(error) => Err(error.to_string()),
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

async fn settle(demo: &WeakEntity<Demo>, cx: &mut AsyncWindowContext) {
    for _ in 0..1500 {
        let busy = demo
            .read_with(cx, |demo, cx| {
                demo.state.read(cx).loading || demo.commands.read(cx).is_building()
            })
            .unwrap_or(false);
        if !busy {
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
    settle(demo, cx).await;
    cx.update(|window, _| window.refresh())
        .map_err(|e| e.to_string())?;
    for _ in 0..4 {
        cx.background_executor().timer(FRAME * 2).await;
    }
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
    match step {
        Step::Screen(screen) => demo.screen = *screen,
        Step::Journal(on) => demo.journal_open = *on,
        Step::Dialog(open) => {
            if *open {
                demo.open_dialog(window, cx);
            } else {
                demo.dialog = None;
            }
        }
        Step::Engagement(prefix) => {
            let found = demo
                .state
                .read(cx)
                .engagements
                .iter()
                .find(|e| e.name.starts_with(prefix.as_str()))
                .map(|e| e.dir.clone());
            let dir = found.ok_or_else(|| format!("no engagement starts with {prefix}"))?;
            demo.state.update(cx, |state, cx| state.set_active(dir, cx));
        }
        Step::PasteScan => demo.hosts.update(cx, |hosts, cx| {
            hosts.show_import(Some(seed::NMAP.to_string()), window, cx)
        }),
        Step::ImportRun => demo.hosts.update(cx, |hosts, cx| hosts.import_pasted(cx)),
        Step::Sort(column) => demo.hosts.update(cx, |hosts, cx| hosts.sort_by(column, cx)),
        Step::Host(n) => demo.hosts.update(cx, |hosts, cx| hosts.select_nth(*n, cx)),
        Step::ReportBuild => demo.report.update(cx, |report, cx| report.build(cx)),
        Step::ReportWrite => demo.report.update(cx, |report, cx| report.write(cx)),
        Step::Evidence => demo
            .sidebar
            .update(cx, |sidebar, cx| sidebar.request_evidence(cx)),
        Step::Variable(name, value) => demo
            .sidebar
            .update(cx, |sidebar, cx| sidebar.set_variable(name, value, cx)),
        Step::Finding(n) => demo.findings.update(cx, |list, cx| list.select_nth(*n, cx)),
        Step::Reveal(n) => demo
            .credentials
            .update(cx, |table, cx| table.toggle_reveal(*n, cx)),
        Step::Calculator => demo
            .findings
            .update(cx, |list, cx| list.show_calculator(cx)),
        Step::EditFinding => demo
            .findings
            .update(cx, |list, cx| list.set_editing(true, cx)),
        Step::ExamStart => demo
            .sidebar
            .update(cx, |sidebar, cx| sidebar.start_exam(cx)),
        Step::ExamItem(n) => demo
            .sidebar
            .update(cx, |sidebar, cx| sidebar.toggle_exam_item_at(*n, cx)),
        Step::NewFinding => demo
            .findings
            .update(cx, |list, cx| list.start_creating(window, cx)),
        Step::Focus(which) => match which.as_str() {
            "search" => demo
                .commands
                .update(cx, |library, cx| library.focus_search(window, cx)),
            "journal" => demo
                .journal
                .update(cx, |panel, cx| panel.focus_filter(window, cx)),
            "entry" => demo
                .sidebar
                .update(cx, |sidebar, cx| sidebar.focus_entry(window, cx)),
            other => return Err(format!("unknown focus: {other}")),
        },
        Step::Theme(dark) => focus_kit::theme::apply(
            if *dark { Mode::Dark } else { Mode::Light },
            Some(window),
            cx,
        )
        .map_err(|e| e.to_string())?,
        Step::Lang(code) => focus_i18n::set_language(code).map_err(|e| e.to_string())?,
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
