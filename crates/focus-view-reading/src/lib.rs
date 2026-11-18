pub mod blockmove;
pub mod media;
pub mod model;
pub mod preview;
pub mod query;
pub mod remote;
pub mod srcmap;
pub mod text;
pub mod view;

use std::path::{Path, PathBuf};

use std::sync::Arc;

use focus_core::{LinkTarget, NoteId, NoteMeta, PropValue, PropertyKind, Vault};

pub use model::{Built, Decor, Prop, build, build_base_page, build_with_links};
pub use preview::LinkPreview;
pub use remote::{RemoteImages, remote_images, set_remote_images};
pub use view::query::QueryHitClicked;
pub use view::{
    CONTEXT, CopySelection, CrumbClicked, FoldState, FoldsChanged, LanguageChosen, LiveBlock,
    LiveKind, LiveOpen, PropertyToggled, ReadingEvent, ReadingView, ScriptMouse, SelectAllNote,
    Typesetting, VariableChosen, VariableMenu, property_checkbox_edit, set_property_type_host,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeVariables {
    pub list: Vec<CodeVariable>,
    pub engaged: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeVariable {
    pub name: String,
    pub value: String,
    pub description: String,
    pub secret: bool,
}

pub trait CodeDecoration: Send + Sync {
    fn decorate(&self, lang: &str, code: &str) -> Option<Decorated>;
}

pub struct Decorated {
    pub spans: Vec<(std::ops::Range<usize>, Decor)>,
    pub resolved: String,
    pub resolved_spans: Vec<(std::ops::Range<usize>, Decor)>,
    pub revealed: Option<(String, DecorSpans)>,
}

pub type DecorSpans = Vec<(std::ops::Range<usize>, Decor)>;

pub trait HostLinks: Send + Sync {
    fn find(&self, text: &str) -> Vec<(std::ops::Range<usize>, model::LinkGoal)>;
}

pub trait Resolver: Send + Sync {
    fn resolve_link(&self, from: NoteId, target: &str) -> LinkTarget;
    fn absolute(&self, relative: &Path) -> PathBuf;

    fn resolve_from_root(&self, _target: &str) -> LinkTarget {
        LinkTarget::Unresolved
    }

    fn known_file(&self, _target: &str) -> Option<PathBuf> {
        None
    }

    fn note_title(&self, _note: NoteId) -> Option<String> {
        None
    }

    fn note_text(&self, _note: NoteId) -> Option<Result<(String, Arc<NoteMeta>), String>> {
        None
    }

    fn base_table(
        &self,
        _path: &Path,
        _view: Option<&str>,
        _this: Option<&Path>,
    ) -> Option<Result<focus_bases::Table, focus_bases::BasesError>> {
        None
    }

    fn base_views(
        &self,
        _path: &Path,
        _this: Option<&Path>,
    ) -> Option<Result<BaseViews, focus_bases::BasesError>> {
        None
    }

    fn query_results(&self, source: &str, note: Option<NoteId>) -> query::QueryState {
        query::search_state(source, note)
    }

    fn property_type(&self, name: &str, value: &PropValue) -> PropertyKind {
        focus_core::property_type(name, value, None, None)
    }
}

pub type BaseViews = Vec<(
    Option<String>,
    Result<focus_bases::Table, focus_bases::BasesError>,
)>;

impl Resolver for Vault {
    fn resolve_link(&self, from: NoteId, target: &str) -> LinkTarget {
        Vault::resolve_link(self, from, target)
    }
    fn absolute(&self, relative: &Path) -> PathBuf {
        Vault::absolute(self, relative)
    }
    fn resolve_from_root(&self, target: &str) -> LinkTarget {
        Vault::resolve_link_from_root(self, target)
    }
    fn note_title(&self, note: NoteId) -> Option<String> {
        self.note(note).map(|meta| meta.title.to_string())
    }
    fn base_table(
        &self,
        path: &Path,
        view: Option<&str>,
        this: Option<&Path>,
    ) -> Option<Result<focus_bases::Table, focus_bases::BasesError>> {
        Some(focus_bases::load(self, path).and_then(|base| base.table(self, view, this)))
    }
    fn base_views(
        &self,
        path: &Path,
        this: Option<&Path>,
    ) -> Option<Result<BaseViews, focus_bases::BasesError>> {
        let base = match focus_bases::load(self, path) {
            Ok(base) => base,
            Err(error) => return Some(Err(error)),
        };
        Some(Ok((0..base.views.len())
            .map(|i| (base.views[i].name.clone(), base.table_at(self, i, this)))
            .collect()))
    }
    fn property_type(&self, name: &str, value: &PropValue) -> PropertyKind {
        Vault::property_type(self, name, value)
    }
    fn note_text(&self, note: NoteId) -> Option<Result<(String, Arc<NoteMeta>), String>> {
        let meta = self.note(note)?;
        match self.read(note) {
            Ok(text) => Some(Ok((text, meta))),
            Err(_) => Some(Err(focus_i18n::t_args(
                "reading-embed-unreadable",
                &[("note", meta.path.to_string_lossy().as_ref().into())],
            ))),
        }
    }
}

pub fn bind(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("ctrl-c", CopySelection, Some(CONTEXT)),
        gpui::KeyBinding::new("ctrl-a", SelectAllNote, Some(CONTEXT)),
        gpui::KeyBinding::new("pageup", view::PageUp, Some(CONTEXT)),
        gpui::KeyBinding::new("pagedown", view::PageDown, Some(CONTEXT)),
        gpui::KeyBinding::new("home", view::NoteStart, Some(CONTEXT)),
        gpui::KeyBinding::new("end", view::NoteEnd, Some(CONTEXT)),
        gpui::KeyBinding::new("ctrl-home", view::NoteStart, Some(CONTEXT)),
        gpui::KeyBinding::new("ctrl-end", view::NoteEnd, Some(CONTEXT)),
    ]);
    cx.bind_keys(view::in_place_bindings());
}
