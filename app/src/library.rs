use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use focus_core::{NoteId, NoteMeta, OpenOptions, Vault, VaultError, VaultEvent};
use focus_search::{FuzzyHit, FuzzyScope, SearchIndex};

pub struct Library {
    vault: Arc<Vault>,
    index: Arc<SearchIndex>,
    pub open_ms: f32,
    pub history_error: Option<VaultError>,
}

pub fn attach_ops_history(vault: &Vault) {
    match vault.history() {
        Some(history) => focus_ops::attach_history(history, vault.root()),
        None => focus_ops::detach_history(),
    }
}

impl Library {
    pub fn open(
        root: &Path,
        ignore: Vec<String>,
        external: focus_core::ExternalLinks,
        history: Option<focus_core::HistoryConfig>,
    ) -> anyhow::Result<Library> {
        let cache_dir = focus_core::settings::cache_dir_for(root);
        Library::open_in(root, cache_dir, ignore, external, history)
    }

    pub fn open_in(
        root: &Path,
        cache_dir: PathBuf,
        ignore: Vec<String>,
        external: focus_core::ExternalLinks,
        history: Option<focus_core::HistoryConfig>,
    ) -> anyhow::Result<Library> {
        let started = Instant::now();
        if let Some(base) = cache_dir.parent() {
            crate::private::log_failure(base, crate::private::private_dir(base));
        }
        crate::private::log_failure(&cache_dir, crate::private::private_dir(&cache_dir));
        let vault = Vault::open_with(
            root,
            OpenOptions {
                cache_dir: cache_dir.clone(),
                ignore,
                watch: true,
            },
            external,
        )?;
        let history_error = history.and_then(|config| {
            vault
                .set_history(&focus_core::settings::history_dir_for(vault.root()), config)
                .err()
        });
        attach_ops_history(&vault);
        let index = SearchIndex::open(vault.clone(), &cache_dir, &|_| {})?;
        Ok(Library {
            vault,
            index,
            open_ms: started.elapsed().as_secs_f32() * 1000.,
            history_error,
        })
    }

    pub fn root(&self) -> &Path {
        self.vault.root()
    }
    pub fn name(&self) -> String {
        self.vault.name()
    }
    pub fn note_count(&self) -> usize {
        self.vault.stats().notes
    }
    pub fn note(&self, id: NoteId) -> Option<Arc<NoteMeta>> {
        self.vault.note(id)
    }
    pub fn note_by_path(&self, relative: &Path) -> Option<NoteId> {
        self.vault.note_by_path(relative)
    }
    pub fn absolute(&self, relative: &Path) -> PathBuf {
        self.vault.absolute(relative)
    }
    pub fn fuzzy(
        &self,
        pattern: &str,
        scope: FuzzyScope,
        recent: &[NoteId],
        limit: usize,
    ) -> Vec<FuzzyHit> {
        self.index.fuzzy(pattern, scope, recent, limit)
    }
    pub fn vault(&self) -> &Arc<Vault> {
        &self.vault
    }
    pub fn index(&self) -> &Arc<SearchIndex> {
        &self.index
    }

    pub fn read(&self, id: NoteId) -> Result<String, VaultError> {
        self.vault.read(id)
    }
    pub fn write(
        &self,
        id: NoteId,
        content: &str,
        expected: Option<SystemTime>,
    ) -> Result<SystemTime, VaultError> {
        self.vault.write(id, content, expected)
    }
    pub fn toggle_task(&self, id: NoteId, line: u32) -> Result<bool, VaultError> {
        self.vault.toggle_task(id, line)
    }
    pub fn persist_index(&self) -> anyhow::Result<()> {
        self.index.persist()
    }

    pub fn events(&self) -> smol::channel::Receiver<VaultEvent> {
        let (tx, rx) = smol::channel::unbounded();
        let source = self.vault.subscribe();
        std::thread::Builder::new()
            .name("focus-vault-events".into())
            .spawn(move || {
                while let Ok(event) = source.recv() {
                    if tx.send_blocking(event).is_err() {
                        break;
                    }
                }
            })
            .map(|_| ())
            .unwrap_or_else(|error| eprintln!("focus: vault event thread: {error}"));
        rx
    }

    pub fn apply(&self, event: &VaultEvent) {
        self.index.apply(event);
    }
}
