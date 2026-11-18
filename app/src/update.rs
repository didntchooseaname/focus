use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use focus_kit::{
    ActiveTokens, Button, ButtonKind, Dir, Flow as _, Icon, IconButton, Tone, Type, ellipsis, icon,
    size,
};
use focus_update::{Error, Installation, Progress, Release, Source, Version};
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Task, Window, div, px,
};

use crate::settings::schema::Value;
use crate::shell::{Screen, Shell};

pub const OFFERED: &str = "general.update_check";
pub const CHECK_NOW: &str = "general.update_now";
const SKIPPED: &str = "general.update_skipped";

pub const AT_LAUNCH: Duration = Duration::from_secs(10);
const CHECK_EVERY: Duration = Duration::from_secs(3600);
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

static RELAUNCH: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Offered,
    Downloading(Progress),
    Installed,
    Failed(Failure),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    Network,
    Unsigned,
    Signature,
    Damaged,
    Missing,
    Write { folder: String, detail: String },
}

pub struct Offer {
    pub release: Release,
    pub installation: Option<Installation>,
    pub stage: Stage,
}

#[derive(Default)]
pub struct Updates {
    pub offer: Option<Offer>,
    watch: Option<Task<()>>,
    work: Option<Task<()>>,
    cancel: Arc<AtomicBool>,
}

type Found = Result<Option<(Release, Option<Installation>)>, Error>;

pub fn running() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the package version is a semantic version")
}

pub fn relaunch() {
    let program = RELAUNCH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    let Some(program) = program else { return };
    let mut command = std::process::Command::new(&program);
    if let Some(mount) = std::env::var_os("APPDIR") {
        for name in ["APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
            command.env_remove(name);
        }
        let kept: Vec<PathBuf> = std::env::var_os("LD_LIBRARY_PATH")
            .map(|paths| {
                std::env::split_paths(&paths)
                    .filter(|path| !path.starts_with(&mount))
                    .collect()
            })
            .unwrap_or_default();
        match std::env::join_paths(kept) {
            Ok(paths) if !paths.is_empty() => command.env("LD_LIBRARY_PATH", paths),
            _ => command.env_remove("LD_LIBRARY_PATH"),
        };
    }
    if let Err(error) = command.spawn() {
        eprintln!("focus: restarting {}: {error}", program.display());
    }
}

fn source(scripted: bool) -> Option<Source> {
    #[cfg(debug_assertions)]
    if let (Ok(releases), Ok(key)) = (
        std::env::var("FOCUS_UPDATE_RELEASES"),
        std::env::var("FOCUS_UPDATE_KEY"),
    ) {
        return match std::fs::read_to_string(&key) {
            Ok(key) => Some(Source { releases, key }),
            Err(error) => {
                eprintln!("focus: reading the release key {key}: {error}");
                None
            }
        };
    }
    (!scripted).then(Source::official)
}

impl Failure {
    fn of(error: &Error) -> Option<Failure> {
        Some(match error {
            Error::Network { .. } => Failure::Network,
            Error::Unsigned => Failure::Unsigned,
            Error::Signature => Failure::Signature,
            Error::Malformed(_) | Error::Checksum(_) | Error::Incomplete(_) => Failure::Damaged,
            Error::Missing(_) => Failure::Missing,
            Error::Io { path, source } => Failure::Write {
                folder: path.parent().unwrap_or(path).display().to_string(),
                detail: source.to_string(),
            },
            Error::Cancelled => return None,
        })
    }

    fn text(&self) -> String {
        match self {
            Failure::Network => focus_i18n::t("errors-update-network"),
            Failure::Unsigned => focus_i18n::t("errors-update-unsigned"),
            Failure::Signature => focus_i18n::t("errors-update-signature"),
            Failure::Damaged => focus_i18n::t("errors-update-damaged"),
            Failure::Missing => focus_i18n::t("errors-update-missing"),
            Failure::Write { folder, detail } => focus_i18n::t_args(
                "errors-update-write",
                &[
                    ("folder", folder.as_str().into()),
                    ("detail", detail.as_str().into()),
                ],
            ),
        }
    }
}

fn tag(version: &Version) -> String {
    format!("v{version}")
}

impl Shell {
    pub fn watch_updates(&mut self, first: Duration, cx: &mut Context<Self>) {
        if !self.prefs.read(cx).bool(OFFERED) || source(self.scripted).is_none() {
            self.updates.watch = None;
            return;
        }
        if self.updates.watch.is_some() {
            return;
        }
        self.updates.watch = Some(cx.spawn(async move |this, cx| {
            let mut wait = first;
            loop {
                cx.background_executor().timer(wait).await;
                wait = CHECK_EVERY;
                let Ok(check) = this.update(cx, |this, cx| this.check_updates(false, cx)) else {
                    return;
                };
                check.await;
            }
        }));
    }

    pub fn check_updates(&mut self, asked: bool, cx: &mut Context<Self>) -> Task<()> {
        let busy = matches!(
            self.updates.offer,
            Some(Offer {
                stage: Stage::Downloading(_) | Stage::Installed,
                ..
            })
        );
        let Some(source) = source(self.scripted).filter(|_| !busy) else {
            return Task::ready(());
        };
        let found = cx.background_spawn(async move {
            let installation = Installation::detect();
            if let Some(installation) = &installation {
                installation.tidy();
            }
            let release = focus_update::check(&source, &running())?;
            Ok(release.map(|release| (release, installation)))
        });
        self.pending += 1;
        cx.spawn(async move |this, cx| {
            let found: Found = found.await;
            let _ = this.update(cx, |this, cx| {
                this.pending -= 1;
                this.update_checked(found, asked, cx);
            });
        })
    }

    fn update_checked(&mut self, found: Found, asked: bool, cx: &mut Context<Self>) {
        match found {
            Ok(Some((release, installation))) => {
                let skipped = self.prefs.read(cx).text(SKIPPED) == release.version.to_string();
                if skipped && !asked {
                    return;
                }
                self.updates.offer = Some(Offer {
                    release,
                    installation,
                    stage: Stage::Offered,
                });
                cx.notify();
            }
            Ok(None) if asked => self.notify_user(
                Tone::Ok,
                focus_i18n::t_args(
                    "ui-update-current",
                    &[("version", running().to_string().as_str().into())],
                ),
                cx,
            ),
            Ok(None) => {}
            Err(error) => {
                eprintln!("focus: checking for an update: {error}");
                if asked && let Some(failure) = Failure::of(&error) {
                    self.notify_user(Tone::Danger, failure.text(), cx);
                }
            }
        }
    }

    pub fn download_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(offer) = &mut self.updates.offer else {
            return;
        };
        if matches!(offer.stage, Stage::Downloading(_) | Stage::Installed) {
            return;
        }
        let Some(installation) = offer.installation.clone() else {
            let page = offer.release.page();
            return self.open_address(&page, window, cx);
        };
        offer.stage = Stage::Downloading(Progress {
            received: 0,
            total: None,
        });
        let release = offer.release.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.updates.cancel = cancel.clone();
        let (steps, progress) = smol::channel::unbounded::<Progress>();
        let work = cx.background_spawn(async move {
            let mut reported: Option<Instant> = None;
            focus_update::install(&release, &installation, &cancel, &mut |step| {
                let last = step.total == Some(step.received);
                if last || reported.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
                    reported = Some(Instant::now());
                    let _ = steps.try_send(step);
                }
            })
        });
        self.pending += 1;
        self.updates.work = Some(cx.spawn(async move |this, cx| {
            while let Ok(step) = progress.recv().await {
                let shown = this.update(cx, |this, cx| {
                    if let Some(offer) = &mut this.updates.offer {
                        offer.stage = Stage::Downloading(step);
                        cx.notify();
                    }
                });
                if shown.is_err() {
                    return;
                }
            }
            let outcome = work.await;
            let _ = this.update(cx, |this, cx| {
                this.pending -= 1;
                if let Err(error) = &outcome {
                    eprintln!("focus: installing the update: {error}");
                }
                if let Some(offer) = &mut this.updates.offer {
                    offer.stage = match outcome {
                        Ok(()) => Stage::Installed,
                        Err(error) => Failure::of(&error).map_or(Stage::Offered, Stage::Failed),
                    };
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub fn update_line(&self) -> String {
        let Some(offer) = &self.updates.offer else {
            return "none".into();
        };
        let stage = match &offer.stage {
            Stage::Offered => "offered".to_string(),
            Stage::Downloading(_) => "downloading".to_string(),
            Stage::Installed => "installed".to_string(),
            Stage::Failed(failure) => format!("failed ({})", failure.text()),
        };
        let way = if offer.installation.is_some() {
            "in place"
        } else {
            "from the page"
        };
        format!(
            "{stage} | {} | {} | {way} | {}",
            running(),
            offer.release.version,
            offer.release.page()
        )
    }

    pub fn cancel_update(&mut self) {
        self.updates.cancel.store(true, Ordering::Relaxed);
    }

    pub fn dismiss_update(&mut self, cx: &mut Context<Self>) {
        let Some(offer) = self.updates.offer.take() else {
            return;
        };
        if offer.stage == Stage::Offered {
            let version = offer.release.version.to_string();
            self.set_pref(SKIPPED, Value::Text(version.into()), cx);
        }
        cx.notify();
    }

    pub fn restart_updated(&mut self, cx: &mut Context<Self>) {
        let Some(Offer {
            installation: Some(installation),
            stage: Stage::Installed,
            ..
        }) = &self.updates.offer
        else {
            return;
        };
        *RELAUNCH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some(installation.program().to_path_buf());
        cx.quit();
    }

    pub fn render_update(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let offer = self.updates.offer.as_ref()?;
        if matches!(self.screen, Screen::Welcome { .. } | Screen::Updates { .. }) {
            return None;
        }
        let t = *cx.tokens();
        let (bar, text) = match &offer.stage {
            Stage::Offered => (t.ink, focus_i18n::t("ui-update-available")),
            Stage::Downloading(_) => (t.ink, focus_i18n::t("ui-update-downloading")),
            Stage::Installed => (t.ok, focus_i18n::t("ui-update-installed")),
            Stage::Failed(failure) => (t.danger, failure.text()),
        };
        let versions = div()
            .flow_row()
            .flex_none()
            .items_center()
            .gap(size::S2)
            .mono()
            .child(div().text_color(t.ink_3).child(tag(&running())))
            .child(icon(Dir::current().arrow_forward(), size::ICON_DENSE).text_color(t.ink_3))
            .child(div().text_color(t.ink).child(tag(&offer.release.version)));
        let received = match &offer.stage {
            Stage::Downloading(Progress {
                received,
                total: Some(total),
            }) => Some(focus_i18n::t_args(
                "ui-update-progress",
                &[
                    (
                        "received",
                        focus_i18n::format_bytes(*received).as_str().into(),
                    ),
                    ("total", focus_i18n::format_bytes(*total).as_str().into()),
                ],
            )),
            Stage::Downloading(Progress { received, .. }) => {
                Some(focus_i18n::format_bytes(*received))
            }
            _ => None,
        };
        let page = offer.release.page();
        let notes = div()
            .id("update-notes")
            .flow_row()
            .flex_none()
            .items_center()
            .gap(size::S1)
            .cursor_pointer()
            .ui()
            .text_color(t.ink)
            .child(
                div()
                    .border_b_1()
                    .border_color(t.line_2)
                    .hover(move |style| style.border_color(t.ink))
                    .child(focus_i18n::t("ui-update-notes")),
            )
            .child(icon(Icon::EXTERNAL, size::ICON_SMALL).text_color(t.ink_3))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_address(&page, window, cx);
            }));
        let action = match &offer.stage {
            Stage::Offered => Button::new("update-download", focus_i18n::t("ui-update-download"))
                .kind(ButtonKind::Primary)
                .on_click(cx.listener(|this, _, window, cx| this.download_update(window, cx))),
            Stage::Failed(_) => Button::new("update-retry", focus_i18n::t("ui-update-retry"))
                .kind(ButtonKind::Primary)
                .on_click(cx.listener(|this, _, window, cx| this.download_update(window, cx))),
            Stage::Downloading(_) => {
                Button::new("update-cancel", focus_i18n::t("ui-action-cancel"))
                    .on_click(cx.listener(|this, _, _, _| this.cancel_update()))
            }
            Stage::Installed => Button::new("update-restart", focus_i18n::t("ui-update-restart"))
                .kind(ButtonKind::Primary)
                .on_click(cx.listener(|this, _, _, cx| this.restart_updated(cx))),
        };
        let close = (!matches!(offer.stage, Stage::Downloading(_))).then(|| {
            let label = if offer.stage == Stage::Offered {
                "ui-update-dismiss"
            } else {
                "ui-action-close"
            };
            IconButton::new("update-close", Icon::CLOSE, focus_i18n::t(label))
                .on_click(cx.listener(|this, _, _, cx| this.dismiss_update(cx)))
        });
        Some(
            div()
                .relative()
                .flow_row()
                .flex_none()
                .items_center()
                .gap(size::S3)
                .h(px(40.))
                .px(size::S4)
                .border_b_1()
                .border_color(t.line)
                .child(
                    div()
                        .absolute()
                        .start_at(px(0.))
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(bar),
                )
                .child(
                    div()
                        .flow_row()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(size::S3)
                        .child(
                            div()
                                .flex()
                                .min_w_0()
                                .ui()
                                .text_color(t.ink)
                                .child(ellipsis(text)),
                        )
                        .child(versions)
                        .children(received.map(|received| {
                            div()
                                .flex_none()
                                .mono_sm()
                                .text_color(t.ink_3)
                                .child(received)
                        })),
                )
                .child(notes)
                .child(action)
                .children(close)
                .into_any_element(),
        )
    }
}
