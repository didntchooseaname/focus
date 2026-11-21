use std::path::{Path, PathBuf};

use focus_kit::{ActiveTokens, Flow as _, Tone, TreeRow, Type, ellipsis, size};
use focus_view_media::raster::Kind;
use focus_view_media::{ImageEvent, ImageView, PdfEvent, PdfView, ZoomAction};
use gpui::{
    AnyElement, AppContext, Context, Entity, IntoElement, ParentElement, Styled, Subscription,
    Window, div, px,
};

use crate::shell::{Pane, Shell};

gpui::actions!(
    focus_media,
    [
        GoToPage,
        OpenSystem,
        RotateImage,
        RotateImageBack,
        CopyImage
    ]
);

pub enum FileView {
    Image(Entity<ImageView>),
    Pdf(Entity<PdfView>),
}

pub struct FilePage {
    pub id: u64,
    pub path: PathBuf,
    pub view: FileView,
    _events: Subscription,
}

#[derive(Default)]
pub struct Files {
    pages: Vec<FilePage>,
    next: u64,
    pub link_page: Option<usize>,
}

pub fn opens(path: &Path) -> bool {
    Kind::of(path).is_some()
}

pub fn palette_files(
    library: &crate::library::Library,
    query: &str,
    limit: usize,
) -> Vec<(PathBuf, String, String, Vec<std::ops::Range<usize>>)> {
    let tree = library.vault().tree();
    let needle: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let mut found = Vec::new();
    for node in &tree.nodes {
        if node.kind != focus_core::NodeKind::Attachment || !opens(&node.path) {
            continue;
        }
        let Some(matched) = subsequence(&node.name, &needle) else {
            continue;
        };
        let folder = node
            .path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        found.push((
            node.path.to_path_buf(),
            node.name.to_string(),
            folder,
            matched,
        ));
        if found.len() >= limit {
            break;
        }
    }
    found
}

fn subsequence(name: &str, needle: &[char]) -> Option<Vec<std::ops::Range<usize>>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut wanted = needle.iter().peekable();
    for (byte, ch) in name.char_indices() {
        let Some(next) = wanted.peek() else { break };
        if ch.to_lowercase().eq(next.to_lowercase()) {
            wanted.next();
            let end = byte + ch.len_utf8();
            match ranges.last_mut() {
                Some(last) if last.end == byte => last.end = end,
                _ => ranges.push(byte..end),
            }
        }
    }
    wanted.peek().is_none().then_some(ranges)
}

impl Files {
    pub fn get(&self, id: u64) -> Option<&FilePage> {
        self.pages.iter().find(|p| p.id == id)
    }
}

impl Shell {
    pub fn bind_media_actions(
        &self,
        root: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        use gpui::InteractiveElement as _;
        root.on_action(cx.listener(|this, _: &GoToPage, window, cx| {
            if this.front_pdf().is_some() {
                this.open_palette(":", window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &OpenSystem, _, cx| {
            if let Some(path) = this.front_file().map(|p| p.path.clone()) {
                this.open_file(&path, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &RotateImage, _, cx| {
            if let Some(image) = this.front_image() {
                image.update(cx, |image, cx| image.rotate(true, cx));
            }
        }))
        .on_action(cx.listener(|this, _: &RotateImageBack, _, cx| {
            if let Some(image) = this.front_image() {
                image.update(cx, |image, cx| image.rotate(false, cx));
            }
        }))
        .on_action(cx.listener(|this, _: &CopyImage, _, cx| {
            if let Some(image) = this.front_image() {
                image.update(cx, |image, cx| image.copy(cx));
            }
        }))
    }

    pub fn open_media(
        &mut self,
        path: &Path,
        page: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(existing) = self.files.pages.iter().find(|p| p.path == path) {
            let id = existing.id;
            if let (FileView::Pdf(view), Some(page)) = (&existing.view, page) {
                view.update(cx, |view, cx| view.go_to(page, None, cx));
            }
            self.show_pane(Pane::File(id), window, cx);
            return;
        }
        self.files.next += 1;
        let id = self.files.next;
        let (view, events) = self.file_view(id, path, page, window, cx);
        self.files.pages.push(FilePage {
            id,
            path: path.to_path_buf(),
            view,
            _events: events,
        });
        self.show_pane(Pane::File(id), window, cx);
    }

    fn file_view(
        &mut self,
        id: u64,
        path: &Path,
        page: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (FileView, Subscription) {
        if Kind::of(path) == Some(Kind::Pdf) {
            let path = path.to_path_buf();
            let view = cx.new(|cx| PdfView::new(path, page, cx));
            let events =
                cx.subscribe_in(
                    &view,
                    window,
                    |this, _, event: &PdfEvent, window, cx| match event {
                        PdfEvent::OpenAddress(address) => this.open_address(address, window, cx),
                    },
                );
            return (FileView::Pdf(view), events);
        }
        let viewport = window.viewport_size();
        let longest =
            (f32::from(viewport.width.max(viewport.height)) * window.scale_factor()).ceil() as u32;
        let path = path.to_path_buf();
        let view = cx.new(|cx| ImageView::new(path, longest, cx));
        let events = cx.subscribe_in(
            &view,
            window,
            move |this, _, event: &ImageEvent, window, cx| match event {
                ImageEvent::Neighbor { forward } => this.image_neighbor(id, *forward, window, cx),
                ImageEvent::Copied => {
                    this.notify_user(Tone::Ok, focus_i18n::t("ui-image-copied"), cx)
                }
                ImageEvent::CopyFailed(error) => this.notify_user(
                    Tone::Danger,
                    focus_i18n::t_args("ui-image-copy-failed", &[("error", error.as_str().into())]),
                    cx,
                ),
            },
        );
        (FileView::Image(view), events)
    }

    pub fn forget_file(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(index) = self.files.pages.iter().position(|p| p.id == id) {
            let page = self.files.pages.remove(index);
            release(&page.view, cx);
        }
    }

    pub fn forget_files(&mut self, cx: &mut Context<Self>) {
        for page in std::mem::take(&mut self.files.pages) {
            release(&page.view, cx);
        }
    }

    pub fn front_file(&self) -> Option<&FilePage> {
        match self.active_pane.and_then(|i| self.panes.get(i)) {
            Some(Pane::File(id)) => self.files.get(*id),
            _ => None,
        }
    }

    pub fn front_pdf(&self) -> Option<Entity<PdfView>> {
        match &self.front_file()?.view {
            FileView::Pdf(view) => Some(view.clone()),
            FileView::Image(_) => None,
        }
    }

    pub fn front_image(&self) -> Option<Entity<ImageView>> {
        match &self.front_file()?.view {
            FileView::Image(view) => Some(view.clone()),
            FileView::Pdf(_) => None,
        }
    }

    pub fn file_element(&self, id: u64) -> Option<AnyElement> {
        Some(match &self.files.get(id)?.view {
            FileView::Image(view) => view.clone().into_any_element(),
            FileView::Pdf(view) => view.clone().into_any_element(),
        })
    }

    pub fn file_tab_label(&self, id: u64) -> String {
        self.files
            .get(id)
            .and_then(|p| p.path.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn focus_file(&self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        let Pane::File(id) = pane else { return };
        let handle = match self.files.get(id).map(|p| &p.view) {
            Some(FileView::Image(view)) => gpui::Focusable::focus_handle(view.read(cx), cx),
            Some(FileView::Pdf(view)) => gpui::Focusable::focus_handle(view.read(cx), cx),
            None => return,
        };
        window.focus(&handle);
    }

    pub fn zoom_file(&mut self, action: ZoomAction, cx: &mut Context<Self>) {
        match self.front_file().map(|p| &p.view) {
            Some(FileView::Image(view)) => view.update(cx, |v, cx| v.zoom(action, cx)),
            Some(FileView::Pdf(view)) => view.update(cx, |v, cx| v.zoom(action, cx)),
            None => {}
        }
    }

    fn image_neighbor(
        &mut self,
        id: u64,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(library), Some(page)) = (self.library.clone(), self.files.get(id)) else {
            return;
        };
        let Ok(relative) = page.path.strip_prefix(library.root()) else {
            return;
        };
        let tree = library.vault().tree();
        let images: Vec<&Path> = siblings(&tree.nodes, relative)
            .into_iter()
            .filter(|p| matches!(Kind::of(p), Some(Kind::Image | Kind::Svg)))
            .collect();
        let Some(at) = images.iter().position(|p| *p == relative) else {
            return;
        };
        let next = if forward {
            images.get(at + 1)
        } else {
            at.checked_sub(1).and_then(|i| images.get(i))
        };
        let Some(next) = next else { return };
        let absolute = library.absolute(next);
        let (view, events) = self.file_view(id, &absolute, None, window, cx);
        if let Some(page) = self.files.pages.iter_mut().find(|p| p.id == id) {
            let old = std::mem::replace(&mut page.view, view);
            page.path = absolute;
            page._events = events;
            release(&old, cx);
        }
        self.focus_file(Pane::File(id), window, cx);
        cx.notify();
    }

    pub fn file_status_path(&self) -> Option<String> {
        let page = self.front_file()?;
        let shown = self
            .library
            .as_ref()
            .and_then(|l| page.path.strip_prefix(l.root()).ok())
            .unwrap_or(&page.path);
        Some(shown.display().to_string())
    }

    pub fn file_status(&self, cx: &gpui::App) -> Vec<String> {
        match self.front_file().map(|p| &p.view) {
            Some(FileView::Image(view)) => match view.read(cx).file {
                Some(((width, height), weight)) => vec![
                    focus_i18n::t_args(
                        "ui-status-image-size",
                        &[("width", width.into()), ("height", height.into())],
                    ),
                    focus_i18n::format_bytes(weight),
                ],
                None => Vec::new(),
            },
            Some(FileView::Pdf(view)) => {
                let view = view.read(cx);
                match view.current_page() {
                    Some(page) => vec![focus_i18n::t_args(
                        "ui-status-pdf-page",
                        &[
                            ("page", (page + 1).into()),
                            ("count", view.page_count().into()),
                        ],
                    )],
                    None => Vec::new(),
                }
            }
            None => Vec::new(),
        }
    }

    pub fn render_file_outline(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = *cx.tokens();
        let view = self.front_pdf()?;
        let Some(items) = view.read(cx).outline().filter(|items| !items.is_empty()) else {
            return Some(
                crate::panes::empty_state(&t, focus_i18n::t("ui-outline-pdf-empty"))
                    .into_any_element(),
            );
        };
        let current = view.read(cx).current_page();
        let shown = items
            .iter()
            .rposition(|item| item.page.is_some() && item.page <= current);
        let rows = items.len();
        let list = gpui::uniform_list("file-outline", rows, move |range, _, _| {
            range
                .map(|i| {
                    let item = &items[i];
                    let (page, top) = (item.page, item.top);
                    let view = view.clone();
                    TreeRow::new(("file-heading", i), item.title.clone())
                        .depth(item.depth)
                        .active(shown == Some(i))
                        .on_click(move |_, _, cx| {
                            if let Some(page) = page {
                                view.update(cx, |view, cx| view.go_to(page, top, cx));
                            }
                        })
                })
                .collect::<Vec<_>>()
        })
        .size_full()
        .py(size::S1)
        .track_scroll(self.outline_scroll.clone());
        Some(
            div()
                .relative()
                .size_full()
                .child(list)
                .child(focus_kit::scrollbar(
                    "file-outline-scrollbar",
                    self.outline_scroll.clone(),
                ))
                .into_any_element(),
        )
    }

    pub fn render_file_status(&self, cx: &gpui::App) -> (Option<AnyElement>, Vec<AnyElement>) {
        let t = *cx.tokens();
        let path = self.file_status_path().map(|path| {
            div()
                .flex_shrink()
                .min_w_0()
                .mono_sm()
                .line_height(px(18.))
                .child(ellipsis(path))
                .into_any_element()
        });
        let items = self
            .file_status(cx)
            .into_iter()
            .map(|text| {
                div()
                    .flow_row()
                    .flex_none()
                    .items_center()
                    .mono_sm()
                    .line_height(px(18.))
                    .text_color(t.ink_3)
                    .child(text)
                    .into_any_element()
            })
            .collect();
        (path, items)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewerStep {
    Zoom(ZoomAction),
    Page(usize),
    Rotate,
    Neighbor { forward: bool },
    Drag(f32),
}

impl ViewerStep {
    pub fn parse(rest: &str) -> Result<ViewerStep, String> {
        let words: Vec<&str> = rest.split_whitespace().collect();
        Ok(match words[..] {
            ["zoom", "in"] => ViewerStep::Zoom(ZoomAction::In),
            ["zoom", "out"] => ViewerStep::Zoom(ZoomAction::Out),
            ["zoom", "fit"] => ViewerStep::Zoom(ZoomAction::Fit),
            ["zoom", "width"] => ViewerStep::Zoom(ZoomAction::FitWidth),
            ["zoom", percent] => ViewerStep::Zoom(ZoomAction::Percent(
                percent
                    .parse()
                    .map_err(|_| "expected: zoom in|out|fit|width|<percent>".to_string())?,
            )),
            ["page", page] => ViewerStep::Page(
                page.parse()
                    .ok()
                    .filter(|p| *p > 0)
                    .ok_or("expected: page <number from 1>")?,
            ),
            ["rotate"] => ViewerStep::Rotate,
            ["next"] => ViewerStep::Neighbor { forward: true },
            ["previous"] => ViewerStep::Neighbor { forward: false },
            ["drag", fraction] => ViewerStep::Drag(
                fraction
                    .parse()
                    .ok()
                    .filter(|f| (0. ..=1.).contains(f))
                    .ok_or("expected: drag <fraction from 0 to 1>")?,
            ),
            _ => return Err("expected: viewer zoom|page|rotate|next|previous|drag".into()),
        })
    }
}

impl Shell {
    pub fn run_viewer_step(
        &mut self,
        step: ViewerStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let page = self.front_file().ok_or("no image or PDF tab in front")?;
        let id = page.id;
        match (step, &page.view) {
            (ViewerStep::Zoom(action), _) => self.zoom_file(action, cx),
            (ViewerStep::Page(page), FileView::Pdf(view)) => {
                view.update(cx, |view, cx| view.go_to(page - 1, None, cx))
            }
            (ViewerStep::Rotate, FileView::Image(view)) => {
                view.update(cx, |view, cx| view.rotate(true, cx))
            }
            (ViewerStep::Neighbor { forward }, FileView::Image(_)) => {
                self.image_neighbor(id, forward, window, cx)
            }
            (ViewerStep::Drag(fraction), FileView::Pdf(view)) => {
                let drawn = view.update(cx, |view, cx| view.drag_scrollbar(fraction, cx));
                println!("drag: {drawn} pages asked while the thumb was held");
            }
            _ => return Err("this step does not apply to the tab in front".into()),
        }
        Ok(())
    }

    pub fn status_line(&self, cx: &gpui::App) -> String {
        if let (None, Some(editor), Some(library)) =
            (self.front_file(), self.active_editor(), &self.library)
        {
            let editor = editor.read(cx);
            let broken = library
                .vault()
                .links_of(editor.id)
                .iter()
                .filter(|(_, target)| matches!(target, focus_core::LinkTarget::Unresolved))
                .count();
            return format!("{} | {broken} broken", editor.meta.path.display());
        }
        self.file_status_path()
            .into_iter()
            .chain(self.file_status(cx))
            .chain(self.graph_status(cx))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    pub fn embeds_drawn(&self, cx: &gpui::App) -> Result<Vec<String>, String> {
        let editor = self.active_editor().ok_or("no active note")?;
        Ok(editor.read(cx).reading.read(cx).script_pdf_embeds())
    }

    pub fn files_busy(&self, cx: &gpui::App) -> bool {
        match self.front_file().map(|page| &page.view) {
            Some(FileView::Image(view)) => view.read(cx).busy(),
            Some(FileView::Pdf(view)) => view.read(cx).busy(),
            None => false,
        }
    }
}

fn release(view: &FileView, cx: &mut gpui::App) {
    match view {
        FileView::Image(view) => view.update(cx, |view, cx| view.release(cx)),
        FileView::Pdf(view) => view.update(cx, |view, cx| view.release(cx)),
    }
}

fn siblings<'a>(nodes: &'a [focus_core::TreeNode], path: &Path) -> Vec<&'a Path> {
    let Some(node) = nodes.iter().find(|n| &*n.path == path) else {
        return Vec::new();
    };
    let parent = node.parent;
    nodes
        .iter()
        .filter(|n| n.parent == parent && n.kind != focus_core::NodeKind::Folder)
        .map(|n| &*n.path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn node(
        name: &str,
        path: &str,
        kind: focus_core::NodeKind,
        parent: Option<u32>,
    ) -> focus_core::TreeNode {
        focus_core::TreeNode {
            name: name.into(),
            path: Arc::from(Path::new(path)),
            kind,
            depth: 0,
            parent,
            subtree_end: 0,
            note: None,
            note_count: 0,
            link_out: None,
        }
    }

    #[test]
    fn previous_and_next_follow_the_order_of_the_tree_in_one_folder() {
        use focus_core::NodeKind::{Attachment, Folder, Note};
        let nodes = vec![
            node("assets", "assets", Folder, None),
            node("sub", "assets/sub", Folder, Some(0)),
            node("deep.png", "assets/sub/deep.png", Attachment, Some(1)),
            node("a.png", "assets/a.png", Attachment, Some(0)),
            node("b.md", "assets/b.md", Note, Some(0)),
            node("c10.jpg", "assets/c10.jpg", Attachment, Some(0)),
            node("c2.pdf", "assets/c2.pdf", Attachment, Some(0)),
            node("other.png", "other.png", Attachment, None),
        ];
        let found = siblings(&nodes, Path::new("assets/a.png"));
        assert_eq!(
            found,
            vec![
                Path::new("assets/a.png"),
                Path::new("assets/b.md"),
                Path::new("assets/c10.jpg"),
                Path::new("assets/c2.pdf")
            ]
        );
        let images: Vec<&Path> = found
            .into_iter()
            .filter(|p| matches!(Kind::of(p), Some(Kind::Image | Kind::Svg)))
            .collect();
        assert_eq!(
            images,
            vec![Path::new("assets/a.png"), Path::new("assets/c10.jpg")]
        );
    }

    #[test]
    fn the_palette_matches_file_names_by_their_letters_in_order() {
        assert_eq!(
            subsequence("Report 2026.pdf", &['r', 'p', 'd', 'f']),
            Some(vec![0..1, 2..3, 13..15])
        );
        assert_eq!(subsequence("shot.png", &['x']), None);
        assert_eq!(subsequence("shot.png", &[]), Some(vec![]));
    }

    #[test]
    fn images_and_pdf_files_open_in_the_viewer_and_the_rest_does_not() {
        assert!(opens(Path::new("a/b.PDF")));
        assert!(opens(Path::new("shot.webp")));
        assert!(!opens(Path::new("tool.sh")));
        assert!(!opens(Path::new("table.base")));
    }
}
