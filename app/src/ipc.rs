use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use focus_mcp::ipc::{Handler, ReferenceServer, Request, Response};
use gpui::{Context, Window};

use crate::shell::Shell;

const ANSWER_WITHIN: Duration = Duration::from_secs(4);

const MAX_QUERY_CHARS: usize = 2_000;
const MAX_INSERT_BYTES: usize = 1_000_000;
const MAX_NOTICE_CHARS: usize = 500;

fn bounded(request: Request) -> Result<Request, String> {
    match request {
        Request::ShowSearch { query } if query.chars().count() > MAX_QUERY_CHARS => Err(format!(
            "the query is longer than {MAX_QUERY_CHARS} characters"
        )),
        Request::InsertAtCursor { text } if text.len() > MAX_INSERT_BYTES => {
            Err(format!("the text is longer than {MAX_INSERT_BYTES} bytes"))
        }
        Request::Notify { text, level } if text.chars().count() > MAX_NOTICE_CHARS => {
            let mut text: String = text.chars().take(MAX_NOTICE_CHARS).collect();
            text.push_str("...");
            Ok(Request::Notify { text, level })
        }
        request => Ok(request),
    }
}

const PRESENT_FOR: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct AgentPresence {
    last: Option<chrono::DateTime<chrono::Local>>,
    _expiry: Option<gpui::Task<()>>,
}

impl AgentPresence {
    pub fn last(&self) -> Option<chrono::DateTime<chrono::Local>> {
        self.last
    }
}

type Exchange = (Request, smol::channel::Sender<Response>);

struct Forward(smol::channel::Sender<Exchange>);

impl Handler for Forward {
    fn handle(&self, request: Request) -> Response {
        let (reply, answer) = smol::channel::bounded(1);
        if self.0.send_blocking((request, reply)).is_err() {
            return Response::Failed {
                message: "the application is closing".into(),
            };
        }
        smol::block_on(async {
            let timeout = async {
                smol::Timer::after(ANSWER_WITHIN).await;
                None
            };
            smol::future::or(async { answer.recv().await.ok() }, timeout).await
        })
        .unwrap_or(Response::Failed {
            message: "the application did not answer in time".into(),
        })
    }
}

pub fn listen(
    root: &Path,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Option<ReferenceServer> {
    let path = focus_mcp::ipc::socket_path(root)?;
    let (sender, requests) = smol::channel::unbounded::<Exchange>();
    let server = match ReferenceServer::start(&path, Arc::new(Forward(sender))) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("focus: agent socket {}: {error}", path.display());
            return None;
        }
    };
    cx.spawn_in(window, async move |shell, cx| {
        while let Ok((request, reply)) = requests.recv().await {
            let response = shell
                .update_in(cx, |shell, window, cx| {
                    shell.answer_agent(request, window, cx)
                })
                .unwrap_or(Response::Failed {
                    message: "the window is closed".into(),
                });
            let _ = reply.send(response).await;
        }
    })
    .detach();
    Some(server)
}

impl Shell {
    pub fn sync_agent_socket(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.agent_server = None;
        if !self.prefs.read(cx).bool("ai.agent_socket") {
            return;
        }
        if let Some(root) = self.vault_root.clone() {
            self.agent_server = listen(&root, window, cx);
        }
    }

    fn agent_seen(&mut self, cx: &mut Context<Self>) {
        self.agent.last = Some(chrono::Local::now());
        self.agent._expiry = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PRESENT_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.agent.last = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub fn answer_agent(
        &mut self,
        request: Request,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Response {
        self.agent_seen(cx);
        let refuse = |message: &str| Response::Failed {
            message: message.to_string(),
        };
        let request = match bounded(request) {
            Ok(request) => request,
            Err(message) => return Response::Failed { message },
        };
        match request {
            Request::CurrentNote => {
                let Some(editor) = self.active_editor().cloned() else {
                    return Response::CurrentNote(focus_mcp::ipc::CurrentNote {
                        path: None,
                        selection: String::new(),
                        line: 1,
                        column: 1,
                    });
                };
                let (path, cursor) = {
                    let editor = editor.read(cx);
                    (
                        editor.meta.path.to_string_lossy().into_owned(),
                        editor.cursor(cx),
                    )
                };
                let selection = editor.update(cx, |editor, cx| editor.selection(window, cx));
                Response::CurrentNote(focus_mcp::ipc::CurrentNote {
                    path: Some(path),
                    selection,
                    line: cursor.line + 1,
                    column: cursor.character + 1,
                })
            }
            Request::OpenNote {
                path,
                line,
                new_tab,
            } => {
                let Some(library) = self.library.clone() else {
                    return refuse("no vault is open");
                };
                let Some(id) = library.note_by_path(Path::new(&path)) else {
                    return refuse(&format!("no note at {path}"));
                };
                self.open_note(id, new_tab, window, cx);
                if let Some(line) = line {
                    self.pending_line = Some((id, line.saturating_sub(1)));
                }
                Response::Done
            }
            Request::ShowSearch { query } => {
                self.show_search(&query, window, cx);
                Response::Done
            }
            Request::ShowGraph { note } => {
                let id = match (&note, &self.library) {
                    (Some(path), Some(library)) => match library.note_by_path(Path::new(path)) {
                        Some(id) => Some(id),
                        None => return refuse(&format!("no note at {path}")),
                    },
                    _ => None,
                };
                self.open_graph(id, window, cx);
                Response::Done
            }
            Request::InsertAtCursor { text } => {
                let Some(editor) = self.active_editor().cloned() else {
                    return refuse("no note is open");
                };
                if editor.read(cx).status_cursor(cx).is_none() {
                    return refuse(
                        "the note is in Reading view: switch it to Edit, or open a block in Live, first",
                    );
                }
                let note = editor.read(cx).meta.title.to_string();
                editor.update(cx, |editor, cx| editor.insert_at_cursor(&text, window, cx));
                let count = text.chars().count();
                self.notify_user(
                    focus_kit::Tone::Warn,
                    focus_i18n::t_args(
                        "ui-agent-inserted",
                        &[("count", count.into()), ("note", note.as_str().into())],
                    ),
                    cx,
                );
                Response::Done
            }
            Request::Notify { text, level } => {
                let tone = match level {
                    focus_mcp::ipc::Level::Info => focus_kit::Tone::Neutral,
                    focus_mcp::ipc::Level::Warning => focus_kit::Tone::Warn,
                    focus_mcp::ipc::Level::Error => focus_kit::Tone::Danger,
                };
                self.notify_user(tone, text, cx);
                Response::Done
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_direct_client_is_bounded() {
        assert!(
            bounded(Request::ShowSearch {
                query: "x".repeat(MAX_QUERY_CHARS + 1)
            })
            .is_err()
        );
        assert!(
            bounded(Request::ShowSearch {
                query: "kerberos".into()
            })
            .is_ok()
        );
        assert!(
            bounded(Request::InsertAtCursor {
                text: "x".repeat(MAX_INSERT_BYTES + 1)
            })
            .is_err()
        );
        match bounded(Request::Notify {
            text: "y".repeat(4_000_000),
            level: focus_mcp::ipc::Level::Info,
        }) {
            Ok(Request::Notify { text, .. }) => {
                assert_eq!(text.chars().count(), MAX_NOTICE_CHARS + 3)
            }
            other => panic!("a long notification is cut, not refused: {other:?}"),
        }
    }
}
