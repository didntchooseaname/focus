use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_ops::vars::Context as VarContext;
use gpui::{AppContext as _, Context, EventEmitter};

use crate::settings::OpsSettings;
use focus_ops::{
    Credential, Engagement, Finding, Host, JournalEntry, Overview, checklist, findings, hosts,
    journal, loot, vars,
};

#[derive(Clone, Debug)]
pub struct Active {
    pub engagement: Engagement,
    pub hosts: Vec<Host>,
    pub findings: Vec<Finding>,
    pub credentials: Vec<Credential>,
    pub journal: Vec<JournalEntry>,
    pub overview: Overview,
    pub checklist: Vec<checklist::Section>,
    pub context: VarContext,
    pub relations: Vec<focus_ops::relations::Relation>,
}

pub(crate) fn drop_typed_rhost(engagement: &mut Engagement) {
    engagement
        .vars
        .retain(|k, _| focus_ops::vars::canonical_name(k) != "RHOST");
}

pub fn graph_relations(
    relations: &[focus_ops::relations::Relation],
    vault_root: &Path,
) -> Vec<(PathBuf, PathBuf)> {
    relations
        .iter()
        .filter_map(|r| {
            Some((
                r.from.strip_prefix(vault_root).ok()?.to_path_buf(),
                r.to.strip_prefix(vault_root).ok()?.to_path_buf(),
            ))
        })
        .collect()
}

impl Active {
    pub fn graph_relations(&self, vault_root: &Path) -> Vec<(PathBuf, PathBuf)> {
        graph_relations(&self.relations, vault_root)
    }

    pub fn focus_host(&self) -> Option<&Host> {
        let focus = self.engagement.focus.as_deref()?;
        self.hosts.iter().find(|h| {
            h.path
                .file_stem()
                .is_some_and(|s| s.to_string_lossy() == focus)
        })
    }
}

fn context_of(engagement: &Engagement, hosts: &[Host], settings: &OpsSettings) -> VarContext {
    let focus = engagement.focus.as_deref().and_then(|f| {
        hosts
            .iter()
            .find(|h| h.path.file_stem().is_some_and(|s| s.to_string_lossy() == f))
    });
    let mut context = vars::context_for_settings(
        engagement,
        focus,
        &settings.interface,
        &settings.default_variables,
    );
    if let Some(typed) = focus_ops::vars::stored(&engagement.vars, "RHOST")
        .filter(|(_, v)| !v.is_empty())
        .map(|(_, v)| v.clone())
    {
        context.values.insert("RHOST".into(), typed);
    }
    context
}

pub fn load_active(dir: &Path, settings: &OpsSettings) -> focus_ops::Result<Active> {
    let engagement = focus_ops::load(dir)?;
    let hosts = hosts::list(dir);
    let context = context_of(&engagement, &hosts, settings);
    let checklist = std::fs::read_to_string(dir.join("Checklist.md"))
        .map(|t| checklist::progress(&t))
        .unwrap_or_default();
    Ok(Active {
        findings: findings::list(dir),
        credentials: loot::list(dir),
        journal: journal::read(dir),
        overview: focus_ops::overview(dir),
        relations: focus_ops::relations::relations(dir),
        engagement,
        hosts,
        checklist,
        context,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum StateEvent {
    Loaded,
    Failed(String),
    ActiveChanged(PathBuf),
}

pub struct OpsState {
    pub ops_dir: PathBuf,
    pub engagements: Vec<Engagement>,
    pub active: Option<Active>,
    pub loading: bool,
    pub settings: OpsSettings,
    pub trash: Option<EngagementTrash>,
    revision: u64,
    queued: Option<Variables>,
    saving: bool,
}

#[derive(Clone)]
struct Variables {
    dir: PathBuf,
    vars: std::collections::BTreeMap<String, String>,
    var_links: std::collections::BTreeMap<String, String>,
}

pub type Mover = Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct EngagementTrash {
    pub system: bool,
    pub mover: Mover,
}

impl EventEmitter<StateEvent> for OpsState {}

pub(crate) fn error_text(error: &focus_ops::OpsError) -> String {
    use focus_i18n::{Arg, t_args};
    match error {
        focus_ops::OpsError::Io { path, source } => t_args(
            "errors-ops-io",
            &[
                ("path", Arg::from(path.display().to_string().as_str())),
                ("detail", Arg::from(source.to_string().as_str())),
            ],
        ),
        focus_ops::OpsError::AlreadyExists(path) => t_args(
            "errors-ops-already-exists",
            &[("path", Arg::from(path.display().to_string().as_str()))],
        ),
        focus_ops::OpsError::Parse { file, message } => t_args(
            "errors-ops-parse",
            &[
                ("file", Arg::from(file.display().to_string().as_str())),
                ("message", Arg::from(message.as_str())),
            ],
        ),
        focus_ops::OpsError::Conflict(path) => t_args(
            "errors-ops-conflict",
            &[("path", Arg::from(path.display().to_string().as_str()))],
        ),
        focus_ops::OpsError::Invalid(message) => t_args(
            "errors-ops-invalid",
            &[("message", Arg::from(message.as_str()))],
        ),
    }
}

impl OpsState {
    pub fn new(ops_dir: PathBuf, active: Option<PathBuf>, cx: &mut Context<Self>) -> OpsState {
        let mut state = OpsState {
            ops_dir,
            engagements: Vec::new(),
            active: None,
            loading: true,
            settings: OpsSettings::default(),
            trash: None,
            revision: 0,
            queued: None,
            saving: false,
        };
        state.load(active, cx);
        state
    }

    pub fn set_settings(&mut self, settings: OpsSettings, cx: &mut Context<Self>) {
        if self.settings == settings {
            return;
        }
        self.settings = settings;
        if let Some(active) = self.active.as_mut() {
            active.context = context_of(&active.engagement, &active.hosts, &self.settings);
        }
        cx.emit(StateEvent::Loaded);
        cx.notify();
    }

    pub fn set_substitute(&mut self, on: bool, cx: &mut Context<Self>) {
        let settings = OpsSettings {
            substitute: on,
            ..self.settings.clone()
        };
        self.set_settings(settings, cx);
    }

    pub fn trash_active(&mut self, cx: &mut Context<Self>) {
        let (Some(trash), Some(dir)) = (
            self.trash.clone(),
            self.active.as_ref().map(|a| a.engagement.dir.clone()),
        ) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let target = dir.clone();
            let moved = cx
                .background_spawn(async move { (trash.mover)(&target) })
                .await;
            this.update(cx, |this, cx| {
                match moved {
                    Ok(()) => {
                        this.engagements.retain(|e| e.dir != dir);
                        this.active = None;
                        cx.emit(StateEvent::Loaded);
                    }
                    Err(message) => cx.emit(StateEvent::Failed(message)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let keep = self.active.as_ref().map(|a| a.engagement.dir.clone());
        self.load(keep, cx);
    }

    pub fn set_active(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        cx.emit(StateEvent::ActiveChanged(dir.clone()));
        self.load(Some(dir), cx);
    }

    fn load(&mut self, wanted: Option<PathBuf>, cx: &mut Context<Self>) {
        self.loading = true;
        let ops_dir = self.ops_dir.clone();
        let settings = self.settings.clone();
        let revision = self.revision;
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move {
                    let engagements = focus_ops::list(&ops_dir);
                    let dir = wanted
                        .filter(|d| d.is_dir())
                        .or_else(|| engagements.first().map(|e| e.dir.clone()));
                    let active = dir.map(|d| load_active(&d, &settings));
                    (engagements, active)
                })
                .await;
            this.update(cx, |this, cx| {
                let (engagements, active) = loaded;
                this.engagements = engagements;
                this.loading = false;
                match active {
                    Some(Ok(mut active)) => {
                        if this.revision != revision
                            && let Some(shown) = this
                                .active
                                .as_ref()
                                .filter(|a| a.engagement.dir == active.engagement.dir)
                        {
                            active.engagement.vars = shown.engagement.vars.clone();
                            active.engagement.var_links = shown.engagement.var_links.clone();
                            focus_ops::links::follow(&mut active.engagement, &active.credentials);
                            active.context =
                                context_of(&active.engagement, &active.hosts, &this.settings);
                        }
                        this.active = Some(active);
                    }
                    Some(Err(error)) => {
                        this.active = None;
                        cx.emit(StateEvent::Failed(error_text(&error)));
                    }
                    None => this.active = None,
                }
                cx.emit(StateEvent::Loaded);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn set_variable(&mut self, name: &str, value: &str, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let shown = active
            .context
            .values
            .get(&vars::canonical_name(name))
            .cloned()
            .unwrap_or_default();
        let current = vars::stored(&active.engagement.vars, name).map(|(_, v)| v.clone());
        match (&current, value.is_empty()) {
            (None, _) if value == shown => return,
            (None, true) => return,
            (Some(old), false) if old == value => return,
            _ => {}
        }
        let (name, value) = (name.to_string(), value.to_string());
        self.edit_variables(
            move |engagement| {
                focus_ops::links::unlink(engagement, &name);
                vars::store(&mut engagement.vars, &name, &value);
                true
            },
            cx,
        );
    }

    pub fn link_variable(
        &mut self,
        name: &str,
        credential: Option<&Credential>,
        cx: &mut Context<Self>,
    ) {
        self.edit_variables(
            |engagement| match credential {
                Some(credential) => focus_ops::links::link(engagement, name, credential),
                None => focus_ops::links::unlink(engagement, name),
            },
            cx,
        );
    }

    pub fn use_credential(
        &mut self,
        credential: &Credential,
        cx: &mut Context<Self>,
    ) -> Vec<String> {
        let mut names = Vec::new();
        self.edit_variables(
            |engagement| {
                names = focus_ops::links::use_credential(engagement, credential);
                !names.is_empty()
            },
            cx,
        );
        names
    }

    pub fn credential_added(&mut self, credential: Credential, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        active.credentials.push(credential);
        let before = active.engagement.vars.clone();
        focus_ops::links::follow(&mut active.engagement, &active.credentials);
        if active.engagement.vars != before {
            active.context = context_of(&active.engagement, &active.hosts, &self.settings);
            self.revision += 1;
        }
        cx.emit(StateEvent::Loaded);
        cx.notify();
    }

    fn edit_variables(
        &mut self,
        edit: impl FnOnce(&mut Engagement) -> bool,
        cx: &mut Context<Self>,
    ) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        let mut engagement = active.engagement.clone();
        if !edit(&mut engagement) {
            return;
        }
        active.context = context_of(&engagement, &active.hosts, &self.settings);
        let variables = Variables {
            dir: engagement.dir.clone(),
            vars: engagement.vars.clone(),
            var_links: engagement.var_links.clone(),
        };
        active.engagement = engagement;
        self.revision += 1;
        cx.emit(StateEvent::Loaded);
        cx.notify();
        if self.saving {
            self.queued = Some(variables);
        } else {
            self.save_variables(variables, cx);
        }
    }

    fn save_variables(&mut self, variables: Variables, cx: &mut Context<Self>) {
        self.saving = true;
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move {
                    let mut engagement = focus_ops::load(&variables.dir)?;
                    engagement.vars = variables.vars;
                    engagement.var_links = variables.var_links;
                    focus_ops::save(&engagement)
                })
                .await;
            this.update(cx, |this, cx| {
                this.saving = false;
                match saved {
                    Ok(()) => {
                        if let Some(next) = this.queued.take() {
                            this.save_variables(next, cx);
                        }
                    }
                    Err(error) => {
                        this.queued = None;
                        cx.emit(StateEvent::Failed(error_text(&error)));
                        this.revision += 1;
                        this.reload(cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn write<R: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        job: impl FnOnce(&Path) -> focus_ops::Result<R> + Send + 'static,
        done: impl FnOnce(&mut Self, R, &mut Context<Self>) + 'static,
    ) {
        let Some(dir) = self.active.as_ref().map(|a| a.engagement.dir.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let outcome = cx.background_spawn(async move { job(&dir) }).await;
            this.update(cx, |this, cx| match outcome {
                Ok(value) => {
                    done(this, value, cx);
                    this.reload(cx);
                }
                Err(error) => cx.emit(StateEvent::Failed(error_text(&error))),
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_ops::{EngagementKind, NewEngagement};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn relations_become_vault_relative_pairs() {
        use focus_ops::relations::{Relation, RelationKind};
        let relation = |from: &str, to: &str| Relation {
            kind: RelationKind::FindingHost,
            from: PathBuf::from(from),
            label: String::new(),
            to: PathBuf::from(to),
        };
        let pairs = graph_relations(
            &[
                relation("/v/Ops/A/Findings/F-001.md", "/v/Ops/A/Hosts/web.md"),
                relation("/elsewhere/F.md", "/v/Ops/A/Hosts/web.md"),
            ],
            Path::new("/v"),
        );
        assert_eq!(
            pairs,
            [(
                PathBuf::from("Ops/A/Findings/F-001.md"),
                PathBuf::from("Ops/A/Hosts/web.md")
            )]
        );
    }

    #[test]
    fn load_active_reads_everything_written_through_focus_ops() {
        let ops_dir = scratch("ops-state");
        let spec = NewEngagement {
            name: "Lab".into(),
            kind: EngagementKind::Lab,
            client: None,
            scope: vec!["10.0.0.0/24".into()],
            exam: None,
            checklist: None,
        };
        let engagement = focus_ops::create(&ops_dir, &spec).unwrap();
        focus_ops::journal::append(&engagement.dir, Some("10.0.0.5"), "first entry").unwrap();
        focus_ops::loot::add(
            &engagement.dir,
            &focus_ops::Credential {
                user: "u".into(),
                kind: "password".into(),
                secret: "s3cret".into(),
                domain: String::new(),
                source: String::new(),
                valid_on: String::new(),
                notes: String::new(),
            },
        )
        .unwrap();
        let active = load_active(&engagement.dir, &OpsSettings::default()).unwrap();
        assert_eq!(active.engagement.name, "Lab");
        assert_eq!(active.journal.len(), 1);
        assert_eq!(active.credentials.len(), 1);
        assert!(active.checklist.iter().map(|s| s.total).sum::<u32>() > 0);
        assert!(active.focus_host().is_none());
        let _ = std::fs::remove_dir_all(&ops_dir);
    }

    #[test]
    fn a_directory_that_is_not_an_engagement_is_an_error() {
        let dir = scratch("ops-state-missing");
        assert!(load_active(&dir.join("nothing"), &OpsSettings::default()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
