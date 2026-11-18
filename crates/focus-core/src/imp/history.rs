use super::*;
use focus_history::{History, Kind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const PRUNE_EVERY: Duration = Duration::from_secs(3600);

#[derive(Default)]
pub(crate) struct Slot {
    history: RwLock<Option<Arc<History>>>,
    failure: Arc<Mutex<Option<VaultError>>>,
    pruning: Arc<AtomicBool>,
    last_prune: Mutex<Option<Instant>>,
}

pub(crate) fn from_history(e: focus_history::Error) -> VaultError {
    match e {
        focus_history::Error::Io { path, source } => VaultError::Io { path, source },
        other => VaultError::Other(other.to_string()),
    }
}

impl Inner {
    pub(crate) fn set_history(&self, dir: &Path, config: HistoryConfig) -> Result<()> {
        let history = {
            let mut slot = self.history.history.write();
            match slot.as_ref() {
                Some(h) if h.dir() == dir => h.set_config(config),
                _ => *slot = Some(Arc::new(History::open(dir, config).map_err(from_history)?)),
            }
            slot.clone().expect("set just above")
        };
        self.prune_in_background(history);
        Ok(())
    }

    pub(crate) fn history(&self) -> Option<Arc<History>> {
        self.history.history.read().clone()
    }

    pub(crate) fn take_history_error(&self) -> Option<VaultError> {
        self.history.failure.lock().take()
    }

    fn record_history_failure(&self, error: VaultError) {
        *self.history.failure.lock() = Some(error);
    }

    pub(crate) fn keep_version(&self, rel: &Path, kind: Kind) -> Result<()> {
        let Some(history) = self.history() else {
            return Ok(());
        };
        let abs = self.root.join(rel);
        match history.keep(rel, kind, || std::fs::read(&abs)) {
            Ok(Some(_)) => {
                self.prune_if_due(history);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(e) if kind == Kind::Edit => {
                self.record_history_failure(from_history(e));
                Ok(())
            }
            Err(e) => Err(from_history(e)),
        }
    }

    pub(crate) fn follow_move(&self, from: &Path, to: &Path) {
        let Some(history) = self.history() else {
            return;
        };
        let moved = if self.root.join(to).is_dir() {
            history.move_folder(from, to)
        } else {
            history.move_note(from, to)
        };
        if let Err(e) = moved {
            self.record_history_failure(from_history(e));
        }
    }

    fn prune_if_due(&self, history: Arc<History>) {
        let due = self
            .history
            .last_prune
            .lock()
            .is_none_or(|at| at.elapsed() >= PRUNE_EVERY);
        if due {
            self.prune_in_background(history);
        }
    }

    fn prune_in_background(&self, history: Arc<History>) {
        if self.history.pruning.swap(true, Ordering::AcqRel) {
            return;
        }
        *self.history.last_prune.lock() = Some(Instant::now());
        let (pruning, failure) = (self.history.pruning.clone(), self.history.failure.clone());
        let spawned = std::thread::Builder::new()
            .name("history-prune".into())
            .spawn(move || {
                if let Err(e) = history.prune() {
                    *failure.lock() = Some(from_history(e));
                }
                pruning.store(false, Ordering::Release);
            });
        if let Err(e) = spawned {
            self.history.pruning.store(false, Ordering::Release);
            self.record_history_failure(io_err(Path::new("history-prune"), e));
        }
    }
}
