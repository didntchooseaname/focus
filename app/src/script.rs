use std::path::Path;
use std::time::Duration;

use focus_kit::Forced;
use gpui::{AppContext as _, AsyncWindowContext, Keystroke, WeakEntity, Window};

use crate::editor::ViewMode;
use crate::kit::Gallery;
use crate::settings::schema::{Section, Value};
use crate::shell::{BottomTab, LeftTab, Overlay, RightTab, Screen, Shell};

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Open(String),
    View(ViewMode),
    Left(Option<LeftTab>),
    Right(Option<RightTab>),
    Mentions,
    Bottom(Option<BottomTab>),
    Palette(String),
    Search(String),
    Expand(String),
    Scroll(f32),
    ScrollBy {
        pixels: f32,
        ms: u64,
    },
    Stamp(Option<String>),
    Goto(String),
    Theme(bool),
    Fullscreen,
    Engagement(String),
    AgentInsert(String),
    Bookmark(String),
    OpenPath(std::path::PathBuf),
    Listen,
    Tabs,
    Front,
    Split,
    Group(bool),
    MoveTab {
        second: bool,
        rank: usize,
    },
    Divider(f32),
    Import(Option<std::path::PathBuf>),
    SelectWord(String),
    AiAction(String),
    Substitute(bool),
    Crumb(Option<usize>),
    Evidence,
    Host(String),
    Variables(u32),
    OpsVar(String, String),
    Credential(focus_ops::Credential),
    UseCredential(String),
    OpsLink(String, String),
    OpsScreen(crate::ops::OpsScreen),
    Codes(String),
    TrashEngagement,
    Target(String),
    Restore,
    DeleteCopy,
    Disk {
        note: String,
        source: std::path::PathBuf,
    },
    Banner(BannerAction),
    Click {
        target: String,
        alt: bool,
    },
    Press {
        target: String,
        right: bool,
    },
    Block(Option<u32>),
    Caret {
        line: u32,
        column: u32,
    },
    Dump(String),
    Table(String),
    Address(String),
    Task(usize),
    CreateBroken(usize),
    Action(String),
    Set(String, String),
    Graph,
    AiFake(Option<String>),
    Point(String),
    Bar(crate::selbar::Press),
    Pointer {
        phase: PointerPhase,
        target: String,
    },
    Handle(HandleStep),
    Drop(String),
    Lang(String),
    Ops(bool),
    Settings(Section),
    Screen(String),
    Hover(String),
    Focus(String),
    Key(String),
    Type(String),
    Wait(u64),
    Mark(String),
    Shot(String),
    Snap(String),
    Zoom(usize),
    Viewer(crate::media::ViewerStep),
    Status,
    Embeds,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleStep {
    Point(String),
    Press(String),
    Move(String, Option<bool>),
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerAction {
    Reload,
    Keep,
    Both,
    Merge,
}

fn script_error(line: usize, message: &str) -> String {
    format!("script, line {line}: {message}")
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
        let fail = |message: &str| script_error(number, message);
        let need = |what: &str| {
            if rest.is_empty() {
                Err(fail(&format!("missing argument: {what}")))
            } else {
                Ok(rest.to_string())
            }
        };
        let step = match word {
            "open" => Step::Open(need("relative path")?),
            "view" => Step::View(match rest {
                "source" => ViewMode::Source,
                "live" => ViewMode::Live,
                "reading" => ViewMode::Reading,
                _ => return Err(fail("expected: source, live or reading")),
            }),
            "left" => Step::Left(match rest {
                "files" => Some(LeftTab::Files),
                "search" => Some(LeftTab::Search),
                "tags" => Some(LeftTab::Tags),
                "bookmarks" => Some(LeftTab::Bookmarks),
                "hidden" => None,
                _ => return Err(fail("expected: files, search, tags, bookmarks or hidden")),
            }),
            "right" => Step::Right(match rest {
                "outline" => Some(RightTab::Outline),
                "links" => Some(RightTab::Links),
                "info" => Some(RightTab::Info),
                "ai" => Some(RightTab::Ai),
                "ops" => Some(RightTab::Ops),
                "hidden" => None,
                _ => return Err(fail("expected: outline, links, info, ai, ops or hidden")),
            }),
            "mentions" => Step::Mentions,
            "bottom" => Step::Bottom(match rest {
                "journal" => Some(BottomTab::Journal),
                "tasks" => Some(BottomTab::Tasks),
                "broken" => Some(BottomTab::Broken),
                "hidden" => None,
                _ => return Err(fail("expected: journal, tasks, broken or hidden")),
            }),
            "palette" => Step::Palette(rest.to_string()),
            "search" => Step::Search(need("query")?),
            "expand" => Step::Expand(need("folder")?),
            "scroll" => match rest.split_whitespace().collect::<Vec<_>>()[..] {
                [fraction] => Step::Scroll(match fraction.parse::<f32>() {
                    Ok(f) if (0.0..=1.0).contains(&f) => f,
                    _ => {
                        return Err(fail(
                            "expected: a fraction between 0 and 1, or <pixels> <ms>",
                        ));
                    }
                }),
                [pixels, ms] => match (pixels.parse::<f32>(), ms.parse::<u64>()) {
                    (Ok(pixels), Ok(ms)) if ms > 0 => Step::ScrollBy { pixels, ms },
                    _ => return Err(fail("expected: scroll <pixels> <duration ms>")),
                },
                _ => return Err(fail("expected: scroll <fraction> or scroll <pixels> <ms>")),
            },
            "stamp" => Step::Stamp((!rest.is_empty()).then(|| rest.to_string())),
            "goto" => Step::Goto(need("heading")?),
            "fullscreen" => Step::Fullscreen,
            "theme" => Step::Theme(match rest {
                "dark" => true,
                "light" => false,
                _ => return Err(fail("expected: dark or light")),
            }),
            "ai" => match rest.split_once(' ').unwrap_or((rest, "")) {
                ("fake", answer) => {
                    Step::AiFake(Some(answer.trim().to_string()).filter(|path| !path.is_empty()))
                }
                _ => return Err(fail("expected: fake [answer file]")),
            },
            "drop" => Step::Drop(need("relative path")?),
            "action" => Step::Action(need("setting key")?),
            "engagement" => Step::Engagement(need("name")?),
            "evidence" => Step::Evidence,
            "host" => Step::Host(need("host name or address")?),
            "variables" => Step::Variables(match rest.parse::<u32>() {
                Ok(n) if n >= 1 => n - 1,
                _ => return Err(fail("expected: the line of the opening fence, from 1")),
            }),
            "opsvar" => {
                let (name, value) = rest.split_once(' ').unwrap_or((rest, ""));
                if name.is_empty() {
                    return Err(fail("expected: a variable name, then its value or nothing"));
                }
                Step::OpsVar(name.to_string(), value.trim().to_string())
            }
            "credential" => match rest.split_whitespace().collect::<Vec<_>>()[..] {
                [user, kind, secret] | [user, kind, secret, _] => Step::Credential(
                    focus_ops::Credential {
                        user: user.into(),
                        kind: kind.into(),
                        secret: secret.into(),
                        domain: rest.split_whitespace().nth(3).unwrap_or_default().into(),
                        source: String::new(),
                        valid_on: String::new(),
                        notes: String::new(),
                    },
                ),
                _ => return Err(fail("expected: user, type, secret, and a domain or nothing")),
            },
            "usecredential" => Step::UseCredential(need("user")?),
            "opslink" => match rest.split_once(' ') {
                Some((name, user)) if !user.trim().is_empty() => {
                    Step::OpsLink(name.to_string(), user.trim().to_string())
                }
                _ => return Err(fail("expected: a variable name, then a user")),
            },
            "opsscreen" => Step::OpsScreen(match rest {
                "dashboard" => crate::ops::OpsScreen::Dashboard,
                "hosts" => crate::ops::OpsScreen::Hosts,
                "findings" => crate::ops::OpsScreen::Findings,
                "credentials" => crate::ops::OpsScreen::Credentials,
                "commands" => crate::ops::OpsScreen::Commands,
                "report" => crate::ops::OpsScreen::Report,
                _ => {
                    return Err(fail(
                        "expected: dashboard, hosts, findings, credentials, commands or report",
                    ));
                }
            }),
            "codes" => Step::Codes(need("file path")?),
            "trash-engagement" => Step::TrashEngagement,
            "target" => Step::Target(need("host note stem")?),
            "restore" => Step::Restore,
            "delete-copy" => Step::DeleteCopy,
            "disk" => match rest.rsplit_once(char::is_whitespace) {
                Some((note, source)) if !note.trim().is_empty() => Step::Disk {
                    note: note.trim().to_string(),
                    source: source.into(),
                },
                _ => return Err(fail("expected: disk <relative path> <file>")),
            },
            "banner" => Step::Banner(match rest {
                "reload" => BannerAction::Reload,
                "keep" => BannerAction::Keep,
                "both" => BannerAction::Both,
                "merge" => BannerAction::Merge,
                _ => return Err(fail("expected: banner reload|keep|both|merge")),
            }),
            "bookmark" => Step::Bookmark(need("note path")?),
            "openpath" => Step::OpenPath(
                std::path::absolute(need("path")?).map_err(|e| fail(&e.to_string()))?,
            ),
            "listen" => Step::Listen,
            "tabs" => Step::Tabs,
            "front" => Step::Front,
            "split" => Step::Split,
            "group" => Step::Group(match rest {
                "1" => false,
                "2" => true,
                _ => return Err(fail("expected: 1 or 2")),
            }),
            "movetab" => match rest.split_whitespace().collect::<Vec<_>>()[..] {
                [group @ ("1" | "2"), rank] => Step::MoveTab {
                    second: group == "2",
                    rank: rank
                        .parse()
                        .map_err(|_| fail("expected: the rank of the tab, from 0"))?,
                },
                _ => return Err(fail("expected: movetab <1|2> <rank>")),
            },
            "divider" => Step::Divider(match rest.parse::<f32>() {
                Ok(share) if (0.0..=1.0).contains(&share) => share,
                _ => return Err(fail("expected: a fraction between 0 and 1")),
            }),
            "import" => Step::Import(match rest {
                "cancel" => None,
                _ => Some(
                    std::path::absolute(need("path or cancel")?)
                        .map_err(|e| fail(&e.to_string()))?,
                ),
            }),
            "select" => Step::SelectWord(need("word")?),
            "aiaction" => Step::AiAction(need("action id")?),
            "substitute" => Step::Substitute(match rest {
                "on" => true,
                "off" => false,
                _ => return Err(fail("expected: on or off")),
            }),
            "crumb" => Step::Crumb(match rest {
                "note" => None,
                _ => Some(
                    rest.parse::<usize>()
                        .map_err(|_| fail("expected: crumb <rank from 0> or crumb note"))?,
                ),
            }),
            "block" => Step::Block(match rest {
                "end" => None,
                _ => match rest.parse::<u32>() {
                    Ok(n) if n >= 1 => Some(n - 1),
                    _ => return Err(fail("expected: a source line from 1, or end")),
                },
            }),
            "caret" => match rest
                .split_once(':')
                .map(|(l, c)| (l.parse::<u32>(), c.parse::<u32>()))
            {
                Some((Ok(line), Ok(column))) if line >= 1 => Step::Caret {
                    line: line - 1,
                    column,
                },
                _ => return Err(fail("expected: caret <line from 1>:<column from 0>")),
            },
            "dump" => Step::Dump(need("file path")?),
            "table" => Step::Table(need("menu, or a table menu item")?),
            "address" => Step::Address(need("address")?),
            "task" => Step::Task(match rest.parse::<usize>() {
                Ok(n) if n >= 1 => n,
                _ => return Err(fail("expected: the rank of a task shown, from 1")),
            }),
            "create-broken" => Step::CreateBroken(match rest.parse::<usize>() {
                Ok(n) if n >= 1 => n,
                _ => return Err(fail("expected: the rank of a broken link shown, from 1")),
            }),
            "agent" => match rest.split_once(char::is_whitespace) {
                Some(("insert", text)) if !text.trim().is_empty() => {
                    Step::AgentInsert(text.trim().to_string())
                }
                _ => return Err(fail("expected: insert <text>")),
            },
            "set" => {
                let (key, value) = rest
                    .split_once(char::is_whitespace)
                    .map_or((rest, ""), |(k, v)| (k, v.trim()));
                if crate::settings::schema::setting(key).is_none() {
                    return Err(fail(&format!("unknown setting: {key}")));
                }
                Step::Set(key.to_string(), value.to_string())
            }
            "point" => Step::Point(need("text")?),
            "bar" => Step::Bar(crate::selbar::Press::parse(rest).ok_or_else(|| {
                fail(
                    "expected: type, type:<paragraph|h1..h6|bullets|numbers|tasks|quote|callout|codeblock>, bold, italic, strike, highlight, code, link, ai or extract",
                )
            })?),
            "pointer" => {
                let (phase, target) = rest
                    .split_once(char::is_whitespace)
                    .map(|(p, t)| (p, t.trim()))
                    .ok_or_else(|| fail("expected: down|move|up <target>"))?;
                let phase = match phase {
                    "down" => PointerPhase::Down,
                    "move" => PointerPhase::Move,
                    "up" => PointerPhase::Up,
                    _ => return Err(fail("expected: down|move|up <target>")),
                };
                Step::Pointer {
                    phase,
                    target: target.to_string(),
                }
            }
            "handle" => Step::Handle(match rest.split_once(char::is_whitespace) {
                Some(("move", target)) => {
                    let target = target.trim();
                    let (nudge, target) = match target.split_once(char::is_whitespace) {
                        Some(("above", t)) => (Some(false), t.trim()),
                        Some(("below", t)) => (Some(true), t.trim()),
                        _ => (None, target),
                    };
                    HandleStep::Move(target.to_string(), nudge)
                }
                Some(("point", target)) => HandleStep::Point(target.trim().to_string()),
                _ if rest == "drop" => HandleStep::Drop,
                _ if !rest.is_empty() => HandleStep::Press(rest.to_string()),
                _ => {
                    return Err(fail(
                        "expected: <target>, point <target>, move [above|below] <target>, or drop",
                    ));
                }
            }),
            "click" => Step::Click {
                target: need("text")?,
                alt: false,
            },
            "altclick" => Step::Click {
                target: need("text")?,
                alt: true,
            },
            "press" | "rpress" => {
                let target = need("target")?;
                let compose = target.starts_with("tag:")
                    || target == "editor"
                    || ["menu:rename-tag", "menu:merge", "menu:extract", "menu:ask-ai"]
                        .contains(&target.as_str());
                if !(target.starts_with("tree:")
                    || target.starts_with("recent:")
                    || target == "menu:paste"
                    || target == "menu:open-system"
                    || compose)
                {
                    return Err(fail(
                        "expected: tree:<relative path>, tree:, tag:<tag>, editor, recent:<rank>, menu:paste, menu:open-system, menu:rename-tag, menu:merge, menu:extract or menu:ask-ai",
                    ));
                }
                Step::Press {
                    target,
                    right: word == "rpress",
                }
            }
            "graph" => Step::Graph,
            "lang" => match rest {
                "en" | "fr" | "ar" | "zh-CN" => Step::Lang(rest.to_string()),
                _ => return Err(fail("expected: en, fr, ar or zh-CN")),
            },
            "ops" => Step::Ops(match rest {
                "on" => true,
                "off" => false,
                _ => return Err(fail("expected: on or off")),
            }),
            "settings" => Step::Settings(if rest.is_empty() {
                Section::General
            } else {
                Section::from_name(rest).ok_or_else(|| fail(&format!("unknown section: {rest}")))?
            }),
            "screen" => Step::Screen(need("screen name")?),
            "hover" => Step::Hover(need("element name")?),
            "focus" => Step::Focus(need("element name")?),
            "key" => {
                let keys = need("key")?;
                Keystroke::parse(&keys).map_err(|e| fail(&format!("unreadable key: {e}")))?;
                Step::Key(keys)
            }
            "type" => Step::Type(need("text")?),
            "mark" => Step::Mark(need("label")?),
            "wait" => Step::Wait(
                rest.parse()
                    .map_err(|_| fail("expected: a duration in milliseconds"))?,
            ),
            "shot" => Step::Shot(need("name")?),
            "snap" => Step::Snap(need("name")?),
            "quit" => Step::Quit,
            "viewer" => Step::Viewer(crate::media::ViewerStep::parse(rest).map_err(|e| fail(&e))?),
            "status" => Step::Status,
            "embeds" => Step::Embeds,
            "zoom" => Step::Zoom(
                rest.parse()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| fail("expected: zoom <rank from 1>"))?,
            ),
            other => return Err(fail(&format!("unknown instruction: {other}"))),
        };
        steps.push((number, step));
    }
    Ok(steps)
}

const FRAME: Duration = Duration::from_millis(20);

pub async fn run(shell: WeakEntity<Shell>, steps: Vec<(usize, Step)>, cx: &mut AsyncWindowContext) {
    let mut steps = steps.into_iter().peekable();
    while let Some((line, step)) = steps.next() {
        let outcome = match &step {
            Step::Quit => break,
            Step::Wait(ms) => {
                cx.background_executor()
                    .timer(Duration::from_millis(*ms))
                    .await;
                Ok(())
            }
            Step::Mark(label) => {
                crate::profile::mark(label);
                Ok(())
            }
            Step::Stamp(label) => {
                stamp(label.as_deref());
                Ok(())
            }
            Step::ScrollBy { pixels, ms } => {
                let (done, finished) = smol::channel::bounded::<()>(1);
                let (pixels, duration) = (*pixels, Duration::from_millis(*ms));
                let started = shell.update_in(cx, |shell, window, _| {
                    let target = match (shell.front_pdf(), shell.active_editor()) {
                        (Some(pdf), _) => Scrolled::Pdf(pdf.downgrade()),
                        (None, Some(editor)) => Scrolled::Note(editor.downgrade()),
                        (None, None) => return Err("no active note or PDF".to_string()),
                    };
                    scroll_frames(
                        target,
                        std::time::Instant::now(),
                        duration,
                        pixels,
                        0.,
                        done,
                        window,
                    );
                    Ok::<_, String>(())
                });
                match started.map_err(|e| e.to_string()).and_then(|r| r) {
                    Ok(()) => finished
                        .recv()
                        .await
                        .map_err(|_| "the scroll stopped: the window went away".to_string()),
                    Err(error) => Err(error),
                }
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
            Step::Shot(name) => shot(&shell, name, cx).await,
            Step::Snap(name) => snap(name, cx),
            Step::Codes(path) => codes(&shell, path, cx).await,
            _ => {
                settle(&shell, cx).await;
                match shell.update_in(cx, |shell, window, cx| shell.apply(&step, window, cx)) {
                    Ok(result) => result,
                    Err(error) => Err(error.to_string()),
                }
            }
        };
        if let Err(message) = outcome {
            eprintln!("focus: {}", script_error(line, &message));
        }
        if !matches!(step, Step::ScrollBy { .. } | Step::Stamp(_) | Step::Snap(_))
            && !matches!(steps.peek(), Some((_, Step::Codes(_))))
        {
            cx.background_executor().timer(FRAME).await;
        }
    }
    let _ = cx.update(|_, cx| cx.quit());
}

async fn codes(
    shell: &WeakEntity<Shell>,
    path: &str,
    cx: &mut AsyncWindowContext,
) -> Result<(), String> {
    let (drawn, done) = smol::channel::bounded::<()>(1);
    cx.update(|window, _| {
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |_, _| {
                let _ = drawn.try_send(());
            })
        })
    })
    .map_err(|e| e.to_string())?;
    done.recv()
        .await
        .map_err(|_| "the window went away".to_string())?;
    let json = shell
        .read_with(cx, |shell, cx| shell.script_codes(cx))
        .map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("writing {path}: {e}"))
}

async fn settle(shell: &WeakEntity<Shell>, cx: &mut AsyncWindowContext) {
    let mut calm = 0;
    for _ in 0..1500 {
        let settled = shell
            .read_with(cx, |shell, cx| shell.settled(cx))
            .unwrap_or(true);
        calm = if settled { calm + 1 } else { 0 };
        if calm >= 3 {
            return;
        }
        let _ = cx.update(|window, _| window.refresh());
        cx.background_executor().timer(FRAME).await;
    }
    eprintln!("focus: work in flight did not settle within 30 s, taking the shot as is");
}

enum Scrolled {
    Note(WeakEntity<crate::editor::NoteEditor>),
    Pdf(WeakEntity<focus_view_media::PdfView>),
}

fn scroll_frames(
    target: Scrolled,
    start: std::time::Instant,
    duration: Duration,
    pixels: f32,
    applied: f32,
    done: smol::channel::Sender<()>,
    window: &mut Window,
) {
    window.on_next_frame(move |window, cx| {
        let elapsed = start.elapsed().min(duration);
        let target_px = pixels * elapsed.as_secs_f32() / duration.as_secs_f32();
        let step = target_px - applied;
        let drawn = match &target {
            Scrolled::Note(editor) => editor
                .update(cx, |editor, cx| editor.scroll_by(gpui::px(step), cx))
                .is_ok(),
            Scrolled::Pdf(pdf) => pdf
                .update(cx, |pdf, cx| pdf.scroll_pixels(step, cx))
                .is_ok(),
        };
        if !drawn || elapsed >= duration {
            let _ = done.try_send(());
            return;
        }
        scroll_frames(target, start, duration, pixels, target_px, done, window);
    });
}

#[cfg(not(unix))]
fn stamp(label: Option<&str>) {
    let label = label.map(|l| format!(" {l}")).unwrap_or_default();
    eprintln!(
        "[{}] stamp{label}",
        chrono::Local::now().format("%H:%M:%S%.6f")
    );
}

#[cfg(unix)]
fn stamp(label: Option<&str>) {
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
    let label = label.map(|l| format!(" {l}")).unwrap_or_default();
    eprintln!(
        "[{:02}:{:02}:{:02}.{:06}] stamp{label}",
        local.tm_hour,
        local.tm_min,
        local.tm_sec,
        now.tv_nsec / 1000
    );
}

fn snap(name: &str, cx: &mut AsyncWindowContext) -> Result<(), String> {
    let command = std::env::var("FOCUS_SHOT_CMD")
        .map_err(|_| "FOCUS_SHOT_CMD is not set: nothing to take the shot with".to_string())?;
    let mut child = std::process::Command::new(&command)
        .arg(name)
        .spawn()
        .map_err(|e| format!("launching FOCUS_SHOT_CMD: {e}"))?;
    let name = name.to_string();
    cx.background_spawn(async move {
        match child.wait() {
            Ok(status) if status.success() => {}
            Ok(status) => eprintln!("focus: snap {name}: FOCUS_SHOT_CMD failed ({status})"),
            Err(error) => eprintln!("focus: snap {name}: {error}"),
        }
    })
    .detach();
    Ok(())
}

async fn shot(
    shell: &WeakEntity<Shell>,
    name: &str,
    cx: &mut AsyncWindowContext,
) -> Result<(), String> {
    let command = std::env::var("FOCUS_SHOT_CMD")
        .map_err(|_| "FOCUS_SHOT_CMD is not set: nothing to take the shot with".to_string())?;
    settle(shell, cx).await;
    cx.update(|window, _| window.refresh())
        .map_err(|e| e.to_string())?;
    for _ in 0..3 {
        cx.background_executor().timer(FRAME * 2).await;
    }
    let name = name.to_string();
    let status = cx
        .background_spawn(async move { std::process::Command::new(&command).arg(&name).status() })
        .await
        .map_err(|e| format!("launching FOCUS_SHOT_CMD: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("FOCUS_SHOT_CMD failed ({status})"))
    }
}

impl Shell {
    fn reading_point(
        &self,
        target: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Result<
        (
            gpui::Entity<focus_view_reading::ReadingView>,
            gpui::Point<gpui::Pixels>,
        ),
        String,
    > {
        let reading = match self.active_editor() {
            Some(editor) => editor.read(cx).reading.clone(),
            None => self.active_base_view().ok_or("no active note")?,
        };
        let zone = target
            .strip_prefix('#')
            .and_then(|zone| zone.split_once(' '))
            .and_then(|(kind, nth)| Some((kind.to_string(), nth.trim().parse::<usize>().ok()?)));
        let coordinates = target
            .strip_prefix('@')
            .and_then(|xy| xy.split_once(','))
            .and_then(|(x, y)| {
                Some(gpui::point(
                    gpui::px(x.trim().parse().ok()?),
                    gpui::px(y.trim().parse().ok()?),
                ))
            });
        let at = match (zone, coordinates) {
            (_, Some(at)) => at,
            (Some((kind, dx)), None) if kind == "block" => {
                self.editor_bounds.0.get().origin + gpui::point(gpui::px(dx as f32), gpui::px(8.))
            }
            (Some((kind, nth)), None) => reading
                .read(cx)
                .script_zone_point(&kind, nth)
                .ok_or_else(|| format!("no {kind} {nth} drawn"))?,
            (None, None) => reading
                .read(cx)
                .point_of(target)
                .ok_or_else(|| format!("text not shown: {target}"))?,
        };
        Ok((reading, at))
    }

    pub fn apply(
        &mut self,
        step: &Step,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        match step {
            Step::Open(path) => {
                let library = self.library.clone().ok_or("no vault open")?;
                let id = library
                    .note_by_path(Path::new(path))
                    .ok_or_else(|| format!("note not found: {path}"))?;
                self.open_note(id, true, window, cx);
            }
            Step::View(view) => self.set_view(*view, window, cx),
            Step::Left(tab) => {
                self.layout.left_open = tab.is_some();
                if let Some(tab) = tab {
                    self.layout.left_tab = *tab;
                }
            }
            Step::Right(tab) => {
                self.layout.right_open = tab.is_some();
                if let Some(tab) = tab {
                    self.layout.right_tab = *tab;
                }
            }
            Step::Mentions => crate::panes::open_unlinked_mentions(cx),
            Step::Bottom(tab) => {
                self.layout.bottom_open = tab.is_some();
                if let Some(tab) = tab {
                    self.layout.bottom_tab = *tab;
                }
            }
            Step::Palette(text) => self.open_palette(text, window, cx),
            Step::Search(query) => {
                self.layout.left_open = true;
                self.layout.left_tab = LeftTab::Search;
                let query = query.clone();
                self.search
                    .update(cx, |panel, cx| panel.set_query(&query, window, cx));
            }
            Step::Expand(folder) => {
                let views = self.vault_views.as_ref().ok_or("no vault open")?;
                let mut folders = views.tree.read(cx).expanded();
                let mut path = Path::new(folder);
                loop {
                    folders.push(path.to_path_buf());
                    match path.parent() {
                        Some(parent) if !parent.as_os_str().is_empty() => path = parent,
                        _ => break,
                    }
                }
                views
                    .tree
                    .update(cx, |tree, cx| tree.set_expanded(folders, cx));
            }
            Step::Scroll(fraction) => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let drawn = match editor.read(cx).view {
                    crate::editor::ViewMode::Reading => true,
                    crate::editor::ViewMode::Live => editor.read(cx).edit_in_place(),
                    _ => false,
                };
                if drawn {
                    let reading = editor.read(cx).reading.clone();
                    reading.update(cx, |reading, cx| {
                        reading.scroll_to_fraction(*fraction);
                        cx.notify();
                    });
                } else {
                    let lines = editor.read(cx).line_count(cx);
                    let line = ((lines.saturating_sub(1)) as f32 * fraction).round() as u32;
                    editor.update(cx, |editor, cx| editor.go_to_line(line, window, cx));
                }
            }
            Step::Goto(title) => {
                let heading = |built: Option<&std::sync::Arc<focus_view_reading::Built>>| {
                    built
                        .and_then(|b| {
                            b.headings
                                .iter()
                                .find(|h| h.text.trim().eq_ignore_ascii_case(title.trim()))
                                .map(|h| (h.line, h.item))
                        })
                        .ok_or_else(|| format!("heading not found: {title}"))
                };
                match (self.active_editor().cloned(), self.active_base_view()) {
                    (None, Some(page)) => {
                        let (_, item) = heading(page.read(cx).built())?;
                        page.update(cx, |page, cx| {
                            page.scroll_to_item(item);
                            cx.notify();
                        });
                    }
                    (editor, _) => {
                        let editor = editor.ok_or("no active note")?;
                        let reading = editor.read(cx).reading.clone();
                        let (line, _) = heading(reading.read(cx).built())?;
                        reading.update(cx, |reading, cx| {
                            reading.scroll_to_line(line);
                            cx.notify();
                        });
                        if editor.read(cx).view != crate::editor::ViewMode::Reading {
                            editor.update(cx, |editor, cx| editor.go_to_line(line, window, cx));
                        }
                    }
                }
            }
            Step::Zoom(rank) => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let source = editor.read(cx).reading.read(cx).built().and_then(|b| {
                    b.items
                        .iter()
                        .filter_map(|i| match &i.node {
                            focus_view_reading::model::Node::Mermaid { source } => {
                                Some(source.clone())
                            }
                            _ => None,
                        })
                        .nth(rank - 1)
                });
                self.overlay = crate::shell::Overlay::Zoom(
                    source.ok_or_else(|| format!("no diagram of rank {rank}"))?,
                );
            }
            Step::Theme(dark) => self.set_pref(
                "appearance.theme",
                Value::Choice(if *dark { 0 } else { 1 }),
                cx,
            ),
            Step::Fullscreen => window.toggle_fullscreen(),
            Step::AiFake(answer) => {
                let panel = self.ai.clone().ok_or("no AI panel: open a vault first")?;
                let answer = answer
                    .as_ref()
                    .map(|path| {
                        std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))
                    })
                    .transpose()?;
                let note = self
                    .active_editor()
                    .map(|editor| editor.read(cx).meta.path.clone());
                let backend: std::sync::Arc<dyn focus_view_ai::ChatBackend> = std::sync::Arc::new(
                    crate::ai::scripted_backend(note.as_deref(), answer.as_deref()),
                );
                panel.update(cx, |panel, cx| panel.set_backend(Some(backend), cx));
                self.search_assist = Some(std::sync::Arc::new(
                    focus_view_search::assist::ScriptedBackend::demo(),
                ));
                self.refresh_search_services(window, cx);
            }
            Step::Click { target, alt } => {
                use focus_view_reading::ScriptMouse;
                let (reading, at) = self.reading_point(target, cx)?;
                let modifiers = gpui::Modifiers {
                    alt: *alt,
                    ..Default::default()
                };
                reading.update(cx, |reading, cx| {
                    reading.script_mouse(
                        ScriptMouse::Press {
                            at,
                            count: 1,
                            modifiers,
                        },
                        window,
                        cx,
                    );
                    reading.script_mouse(ScriptMouse::Up(at), window, cx);
                });
            }
            Step::Press { target, .. } if target.starts_with("recent:") => {
                let rank: usize = target["recent:".len()..]
                    .parse()
                    .map_err(|_| format!("{target}: expected recent:<rank from 1>"))?;
                let Overlay::Settings { view, .. } = &self.overlay else {
                    return Err("the settings are not open".into());
                };
                let view = view.clone();
                if !view.update(cx, |view, cx| {
                    view.activate_recent(rank.wrapping_sub(1), cx)
                }) {
                    return Err(format!(
                        "{target}: the open vault, a folder gone, or no such row"
                    ));
                }
            }
            Step::Pointer { phase, target } => {
                use focus_view_reading::ScriptMouse;
                let (reading, at) = self.reading_point(target, cx)?;
                let input = match phase {
                    PointerPhase::Down => ScriptMouse::Down {
                        at,
                        count: 1,
                        shift: false,
                    },
                    PointerPhase::Move => ScriptMouse::Drag(at),
                    PointerPhase::Up => ScriptMouse::Up(at),
                };
                reading.update(cx, |reading, cx| reading.script_mouse(input, window, cx));
            }
            Step::Handle(step) => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let reading = editor.read(cx).reading.clone();
                match step {
                    HandleStep::Point(target) => {
                        let (_, at) = self.reading_point(target, cx)?;
                        reading.update(cx, |reading, cx| reading.script_point_handle(at, cx))?;
                    }
                    HandleStep::Press(target) => {
                        let (_, at) = self.reading_point(target, cx)?;
                        reading.update(cx, |reading, cx| {
                            reading.script_press_handle(at, window, cx)
                        })?;
                    }
                    HandleStep::Move(target, nudge) => {
                        let (_, at) = self.reading_point(target, cx)?;
                        let dy = match nudge {
                            Some(true) => 10.,
                            Some(false) => -10.,
                            None => 0.,
                        };
                        let at = at + gpui::point(gpui::px(0.), gpui::px(dy));
                        reading.update(cx, |reading, cx| reading.script_drag_handle(at, cx))?;
                    }
                    HandleStep::Drop => {
                        reading.update(cx, |reading, cx| reading.script_drop_handle(window, cx))?;
                    }
                }
            }
            Step::Bar(press) => self.script_bar(*press, window, cx)?,
            Step::Press { target, .. }
                if !(target.starts_with("tree:")
                    || target == "menu:paste"
                    || target == "menu:open-system") =>
            {
                self.compose_press(target, window, cx)?;
            }
            Step::Press { target, right } => {
                let views = self.vault_views.as_ref().ok_or("no vault open")?;
                let pressed =
                    views
                        .tree
                        .update(cx, |tree, cx| match target.strip_prefix("tree:") {
                            Some(path) => tree.script_press(Path::new(path), *right, window, cx),
                            None if target == "menu:open-system" => tree.script_open_system(cx),
                            None => tree.script_paste(cx),
                        });
                if !pressed {
                    return Err(format!("{target}: not drawn, or disabled"));
                }
            }
            Step::Point(text) => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let reading = editor.read(cx).reading.clone();
                let at = reading
                    .read(cx)
                    .point_of(text)
                    .ok_or_else(|| format!("text not shown: {text}"))?;
                reading.update(cx, |reading, cx| {
                    reading.script_mouse(focus_view_reading::ScriptMouse::Move(at), window, cx)
                });
            }
            Step::Graph => self.open_graph(None, window, cx),
            Step::Viewer(step) => self.run_viewer_step(*step, window, cx)?,
            Step::Status => println!("status: {}", self.status_line(cx)),
            Step::Embeds => {
                for embed in self.embeds_drawn(cx)? {
                    println!("embed: {embed}");
                }
            }
            Step::Task(rank) => {
                let views = self
                    .vault_views
                    .as_ref()
                    .ok_or("no vault views: open a vault first")?;
                if !views
                    .tasks
                    .update(cx, |tasks, cx| tasks.script_toggle(rank - 1, cx))
                {
                    return Err(format!("no task of rank {rank} shown"));
                }
            }
            Step::CreateBroken(rank) => {
                let views = self
                    .vault_views
                    .as_ref()
                    .ok_or("no vault views: open a vault first")?;
                if !views
                    .broken
                    .update(cx, |broken, cx| broken.script_create(rank - 1, cx))
                {
                    return Err(format!("no broken link of rank {rank} naming a note"));
                }
            }
            Step::Address(target) => self.open_address(target, window, cx),
            Step::Block(line) => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                if editor.read(cx).view != ViewMode::Live {
                    return Err("the note is not in Live view".into());
                }
                let reading = editor.read(cx).reading.clone();
                let found = match line {
                    Some(line) => reading
                        .read(cx)
                        .live_block_at(*line)
                        .map(|(kind, lines)| (kind, lines, *line)),
                    None => {
                        let count = editor
                            .read(cx)
                            .input
                            .read(cx)
                            .value()
                            .split_inclusive('\n')
                            .count() as u32;
                        Some((focus_view_reading::LiveKind::End, count..count, count))
                    }
                };
                let (kind, lines, line) = found.ok_or("no block at that line")?;
                editor.update(cx, |editor, cx| {
                    editor.open_block(kind, lines, line, true, window, cx)
                });
            }
            Step::Caret { line, column } => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                if editor.read(cx).view != ViewMode::Live {
                    return Err("the note is not in Live view".into());
                }
                editor.update(cx, |editor, cx| editor.set_edit_in_place(true, window, cx));
                let reading = editor.read(cx).reading.clone();
                reading.update(cx, |reading, cx| {
                    reading.place_caret_at(*line, *column, window, cx)
                });
            }
            Step::Dump(path) => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let dump = editor.read(cx).reading.read(cx).script_dump();
                std::fs::write(path, dump).map_err(|e| format!("writing {path}: {e}"))?;
            }
            Step::Table(item) => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let reading = editor.read(cx).reading.clone();
                reading.update(cx, |reading, cx| reading.script_table(item, window, cx))?;
            }
            Step::Crumb(None) => self.reveal_note_in_files(cx),
            Step::Crumb(Some(rank)) => {
                let path = self
                    .active_editor()
                    .map(|editor| editor.read(cx).meta.path.to_path_buf())
                    .unwrap_or_default();
                let folder: std::path::PathBuf = path
                    .parent()
                    .map(|parent| parent.iter().take(*rank).collect())
                    .unwrap_or_default();
                if *rank > path.parent().map_or(0, |p| p.iter().count()) {
                    return Err(format!("no breadcrumb segment of rank {rank}"));
                }
                self.open_crumb_menu(folder, gpui::px(140. + 90. * *rank as f32), cx);
            }
            Step::Substitute(on) => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                ops.state
                    .update(cx, |state, cx| state.set_substitute(*on, cx));
            }
            Step::SelectWord(word) => {
                use focus_view_reading::ScriptMouse;
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let reading = editor.read(cx).reading.clone();
                let at = reading
                    .read(cx)
                    .point_of(word)
                    .ok_or_else(|| format!("text not shown: {word}"))?
                    + gpui::point(gpui::px(4.), gpui::px(0.));
                reading.update(cx, |reading, cx| {
                    reading.script_mouse(
                        ScriptMouse::Press {
                            at,
                            count: 2,
                            modifiers: gpui::Modifiers {
                                alt: true,
                                ..Default::default()
                            },
                        },
                        window,
                        cx,
                    );
                    reading.script_mouse(ScriptMouse::Up(at), window, cx);
                });
            }
            Step::AiAction(id) => self.run_ai_action(id, window, cx),
            Step::OpenPath(path) => {
                self.open_paths.push(path.clone());
                self.open_given_paths(window, cx);
            }
            Step::Listen => {
                let socket = crate::instance::socket_path().ok_or("no XDG_RUNTIME_DIR")?;
                let listener = crate::instance::bind(&socket)
                    .map_err(|e| format!("{}: {e}", socket.display()))?;
                crate::instance::serve(self, listener, socket.clone(), window, cx);
                println!("listening {}", socket.display());
            }
            Step::Front => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let editor = editor.read(cx);
                let line = if editor.view == ViewMode::Reading {
                    let reading = editor.reading.read(cx);
                    reading
                        .top_item()
                        .and_then(|item| Some(reading.built()?.items.get(item)?.lines.start))
                } else {
                    Some(editor.input.read(cx).cursor_position().line)
                };
                println!(
                    "front: {} {:?} line {}",
                    editor.meta.path.display(),
                    editor.view,
                    line.map_or(0, |l| l + 1)
                );
            }
            Step::Tabs => {
                let titles = |tabs: &[gpui::Entity<crate::editor::NoteEditor>]| {
                    tabs.iter()
                        .map(|tab| tab.read(cx).meta.title.to_string())
                        .collect::<Vec<_>>()
                        .join(" | ")
                };
                let focused = titles(&self.tabs);
                match &self.split {
                    None => println!("tabs: {focused}"),
                    Some(split) => {
                        let other = titles(&split.other.tabs);
                        let (first, second) = if split.focused_second {
                            (other, focused)
                        } else {
                            (focused, other)
                        };
                        println!("tabs: {first} || {second}");
                    }
                }
            }
            Step::Split => self.split_right(None, window, cx),
            Step::Group(second) => {
                self.split.as_ref().ok_or("the central area is not split")?;
                self.focus_group(*second, window, cx);
            }
            Step::MoveTab { second, rank } => {
                self.split.as_ref().ok_or("the central area is not split")?;
                self.focus_group(*second, window, cx);
                let editor = self
                    .tabs
                    .get(*rank)
                    .cloned()
                    .ok_or(format!("no tab of rank {rank}"))?;
                let dragged = crate::groups::DraggedTab {
                    second: *second,
                    item: crate::groups::TabItem::Note(editor.entity_id()),
                    title: editor.read(cx).meta.title.to_string().into(),
                };
                self.move_tab(&dragged, !*second, window, cx);
            }
            Step::Divider(share) => {
                let split = self.split.as_mut().ok_or("the central area is not split")?;
                split.share = crate::groups::clamp_share(*share);
            }
            Step::Import(source) => {
                let views = self
                    .vault_views
                    .as_ref()
                    .ok_or("no vault views: open a vault first")?;
                match source {
                    Some(path) => views.tree.update(cx, |tree, cx| {
                        tree.import(
                            vec![path.clone()],
                            std::path::PathBuf::new(),
                            focus_core::ImportMode::Copy,
                            cx,
                        )
                    }),
                    None => views.tree.update(cx, |tree, _| tree.cancel_import()),
                }
            }
            Step::Bookmark(path) => {
                let views = self
                    .vault_views
                    .as_ref()
                    .ok_or("no vault views: open a vault first")?;
                views.bookmarks.update(cx, |bookmarks, cx| {
                    bookmarks.toggle(std::path::Path::new(path), cx)
                });
            }
            Step::AgentInsert(text) => {
                if let focus_mcp::ipc::Response::Failed { message } = self.answer_agent(
                    focus_mcp::ipc::Request::InsertAtCursor { text: text.clone() },
                    window,
                    cx,
                ) {
                    return Err(message);
                }
            }
            Step::OpsVar(name, value) => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                ops.sidebar
                    .update(cx, |sidebar, cx| sidebar.set_variable(name, value, cx));
            }
            Step::Credential(credential) => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                let dir = ops
                    .state
                    .read(cx)
                    .active
                    .as_ref()
                    .map(|a| a.engagement.dir.clone())
                    .ok_or("no active engagement")?;
                let credential = credential.clone();
                ops.credentials
                    .update(cx, |table, cx| table.save(dir, credential, cx));
            }
            Step::UseCredential(user) => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                let credential = ops
                    .state
                    .read(cx)
                    .active
                    .as_ref()
                    .and_then(|a| {
                        a.credentials
                            .iter()
                            .rev()
                            .find(|c| c.user == *user)
                            .cloned()
                    })
                    .ok_or_else(|| format!("no credential of {user}"))?;
                ops.credentials
                    .update(cx, |table, cx| table.use_credential(credential, cx));
            }
            Step::OpsLink(name, user) => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                if !ops
                    .sidebar
                    .update(cx, |sidebar, cx| sidebar.script_link(name, user, cx))
                {
                    return Err(format!("no credential of {user} suits {name}"));
                }
            }
            Step::OpsScreen(screen) => self.show_ops(*screen, window, cx),
            Step::TrashEngagement => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                ops.state.update(cx, |state, cx| state.trash_active(cx));
            }
            Step::Target(stem) => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                let stem = stem.clone();
                ops.state.update(cx, |state, cx| {
                    state.write(
                        cx,
                        move |dir| {
                            let mut engagement = focus_ops::load(dir)?;
                            engagement.focus = Some(stem);
                            focus_ops::save(&engagement)
                        },
                        |_, (), _| {},
                    )
                });
            }
            Step::DeleteCopy => {
                let dialog = self
                    .versions_dialog()
                    .ok_or("the Versions dialog is closed")?;
                dialog.update(cx, |dialog, cx| dialog.delete_copy(cx));
            }
            Step::Disk { note, source } => {
                let root = self.vault_root.clone().ok_or("no vault open")?;
                let target = root.join(note);
                let text =
                    std::fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
                let temporary = target.with_file_name(".focus-script-disk.tmp");
                std::fs::write(&temporary, text)
                    .and_then(|()| std::fs::rename(&temporary, &target))
                    .map_err(|e| format!("{}: {e}", target.display()))?;
            }
            Step::Banner(action) => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let (conflict, can_merge) = {
                    let editor = editor.read(cx);
                    (editor.conflict, editor.can_merge())
                };
                if !conflict {
                    return Err("the active note shows no conflict banner".into());
                }
                if *action == BannerAction::Merge && !can_merge {
                    return Err("the banner offers no merge".into());
                }
                editor.update(cx, |editor, cx| match action {
                    BannerAction::Reload => editor.load(window, cx).detach(),
                    BannerAction::Keep => drop(editor.save(true, cx)),
                    BannerAction::Both => editor.keep_both(window, cx),
                    BannerAction::Merge => editor.merge(window, cx),
                });
            }
            Step::Restore => {
                let dialog = self
                    .versions_dialog()
                    .ok_or("the Versions dialog is closed")?;
                dialog.update(cx, |dialog, cx| dialog.restore(cx));
            }
            Step::Variables(line) => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let reading = editor.read(cx).reading.clone();
                let at =
                    self.editor_bounds.0.get().origin + gpui::point(gpui::px(400.), gpui::px(200.));
                reading.update(cx, |reading, cx| {
                    reading.script_open_variables(*line, at, window, cx)
                });
            }
            Step::Host(name) => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                let (hosts, table) = (
                    ops.state
                        .read(cx)
                        .active
                        .as_ref()
                        .map(|a| a.hosts.clone())
                        .unwrap_or_default(),
                    ops.hosts.clone(),
                );
                let key =
                    |h: &focus_ops::Host| h.hostname.clone().unwrap_or_default().to_lowercase();
                let mut sorted: Vec<_> = hosts.iter().collect();
                sorted.sort_by(|a, b| {
                    key(a)
                        .is_empty()
                        .cmp(&key(b).is_empty())
                        .then(key(a).cmp(&key(b)))
                });
                let rank = sorted
                    .iter()
                    .position(|h| {
                        h.hostname
                            .as_deref()
                            .is_some_and(|n| n.eq_ignore_ascii_case(name))
                            || h.ip == *name
                    })
                    .ok_or_else(|| format!("no host {name} in the active engagement"))?;
                self.show_ops(crate::ops::OpsScreen::Hosts, window, cx);
                table.update(cx, |table, cx| {
                    table.sort_by("name", cx);
                    table.select_nth(rank, cx);
                });
            }
            Step::Evidence => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                ops.sidebar
                    .update(cx, |sidebar, cx| sidebar.request_evidence(cx));
            }
            Step::Engagement(name) => {
                let ops = self.ops.as_ref().ok_or("Ops mode is off")?;
                let ops_dir = ops.state.read(cx).ops_dir.clone();
                let spec = focus_ops::NewEngagement {
                    name: name.clone(),
                    kind: focus_ops::EngagementKind::Lab,
                    client: None,
                    scope: Vec::new(),
                    exam: None,
                    checklist: None,
                };
                let engagement = focus_ops::create(&ops_dir, &spec).map_err(|e| e.to_string())?;
                ops.state
                    .update(cx, |state, cx| state.set_active(engagement.dir, cx));
            }
            Step::Action(key) => match key.as_str() {
                "search.rebuild" => self.rebuild_index(window, cx),
                "ops.new-engagement" => {
                    window.dispatch_action(Box::new(crate::actions::OpsNewEngagement), cx)
                }
                "capture" => window.dispatch_action(Box::new(crate::actions::QuickCapture), cx),
                other => return Err(format!("no scripted action for {other}")),
            },
            Step::Set(key, text) => {
                use crate::settings::schema::{Control, Value, setting};
                let setting = setting(key).ok_or("unknown setting")?;
                let value = match setting.control {
                    Control::Toggle => Value::Bool(match text.as_str() {
                        "on" | "true" => true,
                        "off" | "false" => false,
                        _ => return Err(format!("expected on or off: {text}")),
                    }),
                    Control::Number { .. } => {
                        Value::Number(text.parse().map_err(|_| format!("not a number: {text}"))?)
                    }
                    Control::Choice(_) => Value::Choice(
                        text.parse()
                            .map_err(|_| format!("not an option index: {text}"))?,
                    ),
                    _ => Value::Text(text.clone().into()),
                };
                self.set_pref(setting.key, value, cx);
            }
            Step::Drop(path) => {
                let root = self.vault_root.clone().ok_or("no vault open")?;
                self.drop_files(&[root.join(path)], window, cx);
            }
            Step::Lang(code) => {
                if let Screen::Welcome { .. } = self.screen {
                    let index = crate::welcome::languages()
                        .iter()
                        .position(|c| c == code)
                        .unwrap_or(0);
                    self.welcome_select(index, window, cx);
                } else {
                    let index = crate::settings::schema::LANGUAGE_CODES
                        .iter()
                        .position(|c| *c == Some(code.as_str()))
                        .unwrap_or(0);
                    self.set_pref("general.language", Value::Choice(index), cx);
                }
            }
            Step::Ops(on) => self.set_pref("ops.enabled", Value::Bool(*on), cx),
            Step::Settings(section) => self.open_settings(*section, window, cx),
            Step::Screen(name) => match name.as_str() {
                "kit" => {
                    let gallery = cx.new(|cx| Gallery::new(window, cx));
                    self.screen = Screen::Kit(gallery);
                }
                "workspace" => self.screen = Screen::Workspace,
                other => {
                    return Err(format!(
                        "unknown screen: {other} (expected: kit or workspace)"
                    ));
                }
            },
            Step::Hover(name) => {
                if let Some(path) = name.strip_prefix("tree:") {
                    let views = self.vault_views.as_ref().ok_or("no vault open")?;
                    if !views
                        .tree
                        .update(cx, |tree, cx| tree.select(Path::new(path), cx))
                    {
                        return Err(format!("not in the tree: {path}"));
                    }
                } else if name == "notice" {
                    if !self.hover_newest_notice(cx) {
                        return Err("no notification shown".into());
                    }
                } else {
                    self.forced.insert(name.clone(), Forced::Hover);
                }
            }
            Step::Focus(name) => match name.as_str() {
                "ai" => self.show_ai(window, cx),
                "tree" => {
                    let views = self.vault_views.as_ref().ok_or("no vault open")?;
                    let handle = gpui::Focusable::focus_handle(views.tree.read(cx), cx);
                    window.focus(&handle);
                }
                "editor" => {
                    let editor = self.active_editor().cloned().ok_or("no active note")?;
                    let input = editor.read(cx).typing_input();
                    input.update(cx, |input, cx| input.focus(window, cx));
                }
                "search" => {
                    self.search
                        .update(cx, |panel, cx| panel.focus_field(window, cx));
                }
                "palette" => match &self.overlay {
                    Overlay::Palette { view, .. } => {
                        let input = view.read(cx).input.clone();
                        input.update(cx, |input, cx| input.focus(window, cx));
                    }
                    _ => return Err("the palette is closed".into()),
                },
                "settings-search" => match &self.overlay {
                    Overlay::Settings { view, .. } => {
                        let input = view.read(cx).search_input().clone();
                        input.update(cx, |input, cx| input.focus(window, cx));
                    }
                    _ => return Err("the settings are closed".into()),
                },
                _ => {
                    self.forced.insert(name.clone(), Forced::Focus);
                }
            },
            Step::Quit
            | Step::Wait(_)
            | Step::Mark(_)
            | Step::Key(_)
            | Step::Type(_)
            | Step::Shot(_)
            | Step::Snap(_)
            | Step::Codes(_)
            | Step::Stamp(_)
            | Step::ScrollBy { .. } => {}
        }
        cx.notify();
        Ok(())
    }

    fn script_codes(&self, cx: &gpui::App) -> String {
        let block = |(line, shown, copied): (u32, String, String)| serde_json::json!({ "line": line + 1, "shown": shown, "copied": copied });
        let mut views = Vec::new();
        let second = self.split.as_ref().is_some_and(|s| s.focused_second);
        let fronts = [
            (second, self.active_editor().cloned()),
            (
                !second,
                self.split.as_ref().and_then(|s| {
                    s.other
                        .active_pane
                        .is_none()
                        .then(|| s.other.active.and_then(|i| s.other.tabs.get(i).cloned()))
                        .flatten()
                }),
            ),
        ];
        for (second, editor) in fronts {
            let Some(editor) = editor else { continue };
            let editor = editor.read(cx);
            views.push(serde_json::json!({
                "view": if second { "group 2" } else { "group 1" },
                "note": editor.meta.path.to_string_lossy(),
                "mode": format!("{:?}", editor.view),
                "blocks": editor.reading.read(cx).script_code().into_iter().map(block).collect::<Vec<_>>(),
            }));
        }
        if self.preview.is_some()
            && let Some(preview) = &self.link_preview
        {
            views.push(serde_json::json!({
                "view": "preview",
                "blocks": preview.read(cx).script_code(cx).into_iter().map(block).collect::<Vec<_>>(),
            }));
        }
        if let Some(ops) = &self.ops {
            let drawn = ops.commands.read(cx).script_drawn();
            if !drawn.is_empty() {
                views.push(serde_json::json!({
                    "view": "commands",
                    "blocks": drawn
                        .into_iter()
                        .map(|(shown, copied)| serde_json::json!({ "shown": shown, "copied": copied }))
                        .collect::<Vec<_>>(),
                }));
            }
        }
        serde_json::json!({ "views": views }).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{Step, parse};

    #[test]
    fn parses_the_documented_instructions() {
        let steps = parse("# comment\nopen 00_Home.md\nview source\nleft hidden\nscroll 0.5\nshot shell-dark\nsnap typing\ncaret 9:4\ndump out.json\nquit\n").expect("valid script");
        assert_eq!(steps.len(), 9);
        assert_eq!(steps[0], (2, Step::Open("00_Home.md".into())));
        assert_eq!(steps[2].1, Step::Left(None));
        assert_eq!(steps[5].1, Step::Snap("typing".into()));
        assert_eq!(steps[6].1, Step::Caret { line: 8, column: 4 });
        assert_eq!(steps[7].1, Step::Dump("out.json".into()));
    }

    #[test]
    fn reports_the_line_of_an_error() {
        let error = parse("open a.md\nview sideways\n").unwrap_err();
        assert!(error.contains("line 2"), "{error}");
    }
}
