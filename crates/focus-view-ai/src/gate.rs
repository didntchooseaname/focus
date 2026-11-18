use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use focus_ai::Cancel;
use focus_mcp::WriteProposal;
use parking_lot::Mutex;

pub(crate) struct Proposal {
    pub tool: String,
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: String,
    pub reply: mpsc::Sender<bool>,
}

pub struct WriteGate {
    sender: smol::channel::Sender<Proposal>,
    receiver: Mutex<Option<smol::channel::Receiver<Proposal>>>,
}

impl WriteGate {
    pub fn new() -> Arc<WriteGate> {
        let (sender, receiver) = smol::channel::unbounded();
        Arc::new(WriteGate {
            sender,
            receiver: Mutex::new(Some(receiver)),
        })
    }

    pub fn confirm(
        self: &Arc<Self>,
    ) -> impl Fn(&WriteProposal, &Cancel) -> bool + Send + Sync + 'static {
        let gate = self.clone();
        move |proposal, cancel| {
            let (reply, answer) = mpsc::channel();
            let sent = gate.sender.send_blocking(Proposal {
                tool: proposal.tool.to_string(),
                path: proposal.path.to_path_buf(),
                before: proposal.before.map(str::to_string),
                after: proposal.after.to_string(),
                reply,
            });
            if sent.is_err() {
                return false;
            }
            loop {
                match answer.recv_timeout(Duration::from_millis(50)) {
                    Ok(decision) => return decision,
                    Err(mpsc::RecvTimeoutError::Timeout) if !cancel.is_cancelled() => {}
                    Err(_) => return false,
                }
            }
        }
    }

    pub(crate) fn take_receiver(&self) -> Option<smol::channel::Receiver<Proposal>> {
        self.receiver.lock().take()
    }
}
