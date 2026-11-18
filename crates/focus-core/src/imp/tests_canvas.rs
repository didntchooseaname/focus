use super::*;
use std::time::Duration;

fn put(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn get(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap()
}

fn open(dir: &Path, root: &Path) -> Arc<Vault> {
    Vault::open(root, options(dir, false)).unwrap()
}

const BOARD: &str = "{\n\t\"nodes\":[\n\t\t{\"id\":\"a1\",\"type\":\"file\",\"file\":\"Notes/Target.md\",\"x\":0,\"y\":0,\"width\":400,\"height\":400}\n\t],\n\t\"edges\":[]\n}";

#[test]
fn renaming_a_note_rewrites_the_canvas_that_shows_it() {
    let dir = test_dir("canvas-rename-note");
    let root = dir.join("vault");
    put(&root, "Notes/Target.md", "# Target\n");
    put(&root, "Board.canvas", BOARD);
    let v = open(&dir, &root);
    v.rename(
        Path::new("Notes/Target.md"),
        Path::new("Notes/Renamed.md"),
        true,
    )
    .unwrap();
    assert_eq!(
        get(&root, "Board.canvas"),
        BOARD.replace("Notes/Target.md", "Notes/Renamed.md")
    );
}

fn options(dir: &Path, watch: bool) -> OpenOptions {
    OpenOptions {
        cache_dir: dir.join("cache"),
        ignore: Vec::new(),
        watch,
    }
}

fn note(v: &Vault, p: &str) -> NoteId {
    v.note_by_path(Path::new(p))
        .unwrap_or_else(|| panic!("{p} missing"))
}

fn canvas(v: &Vault, p: &str) -> NoteId {
    v.canvas_by_path(Path::new(p))
        .unwrap_or_else(|| panic!("canvas {p} missing"))
}

const MIXED: &str = r##"{"nodes":[
{"id":"t","type":"text","text":"[[Notes/Target]] [t](Notes/Target.md)","x":0,"y":0,"width":1,"height":1},
{"id":"f","type":"file","file":"Notes/Target.md","subpath":"#Part","x":0,"y":0,"width":1,"height":1},
{"id":"i","type":"file","file":"img/pic.png","x":0,"y":0,"width":1,"height":1},
{"id":"m","type":"file","file":"Notes/Missing.md","x":0,"y":0,"width":1,"height":1},
{"id":"n","type":"file","file":"Target.md","x":0,"y":0,"width":1,"height":1}
],"edges":[]}"##;

fn mixed(name: &str) -> (PathBuf, PathBuf) {
    let dir = test_dir(name);
    let root = dir.join("vault");
    put(&root, "Notes/Target.md", "# Target\n\n## Part\n");
    put(&root, "Notes/Other.md", "# Other\n[[Target]]\n");
    put(&root, "img/pic.png", "PNG");
    put(&root, "Boards/Mixed.canvas", MIXED);
    (dir, root)
}

#[test]
fn a_canvas_is_a_source_of_links() {
    let (dir, root) = mixed("canvas-index");
    let v = open(&dir, &root);
    let target = note(&v, "Notes/Target.md");
    let board = canvas(&v, "Boards/Mixed.canvas");
    assert!(v.note(board).is_none());
    assert!(v.note_by_path(Path::new("Boards/Mixed.canvas")).is_none());
    assert_eq!(
        v.source_path(board).as_deref(),
        Some(Path::new("Boards/Mixed.canvas"))
    );
    assert_eq!(v.canvases().len(), 1);
    assert_eq!(v.stats().notes, 2);

    let backlinks = v.backlinks(target);
    let from_board: Vec<_> = backlinks.iter().filter(|b| b.source == board).collect();
    assert_eq!(
        from_board.len(),
        1,
        "text cards do not count: {backlinks:?}"
    );
    assert_eq!(from_board[0].line, 2);
    assert_eq!(from_board[0].context, "Notes/Target.md#Part");
    assert_eq!(
        &from_board[0].context[from_board[0].range.clone()],
        "Notes/Target.md"
    );
    let mut sources = v.linked_from(target);
    sources.sort_unstable();
    let mut expected = [note(&v, "Notes/Other.md"), board];
    expected.sort_unstable();
    assert_eq!(sources, expected);

    let links = v.links_of(board);
    assert_eq!(links.len(), 4);
    assert_eq!(links[0].0.kind, LinkKind::Embed);
    assert_eq!(
        links[0].1,
        LinkTarget::Note {
            id: target,
            anchor: Some("Part".into())
        }
    );
    assert_eq!(links[1].1, LinkTarget::Attachment("img/pic.png".into()));
    let broken: Vec<String> = v.unresolved(board).into_iter().map(|l| l.target).collect();
    assert_eq!(broken, ["Notes/Missing.md", "Target.md"]);
    assert_eq!(
        v.all_unresolved()
            .iter()
            .filter(|(id, _)| *id == board)
            .count(),
        2
    );
    assert!(
        !v.orphan_attachments()
            .contains(&PathBuf::from("img/pic.png")),
        "an image on a canvas is used"
    );
}

#[test]
fn invalid_canvases_have_no_links_and_are_never_written() {
    let dir = test_dir("canvas-invalid");
    let root = dir.join("vault");
    put(&root, "Notes/Target.md", "# Target\n");
    let broken = "{\"nodes\":[{\"type\":\"file\",\"file\":\"Notes/Target.md\"}]";
    let not_canvas = "[{\"type\":\"file\",\"file\":\"Notes/Target.md\"}]";
    put(&root, "Broken.canvas", broken);
    put(&root, "List.canvas", not_canvas);
    std::fs::write(root.join("Binary.canvas"), [0xff, 0xfe, 0x00, b'{']).unwrap();
    let v = open(&dir, &root);
    let target = note(&v, "Notes/Target.md");
    assert!(v.backlinks(target).is_empty());
    assert!(v.linked_from(target).is_empty());
    for p in ["Broken.canvas", "List.canvas", "Binary.canvas"] {
        assert!(v.links_of(canvas(&v, p)).is_empty());
    }
    let report = v
        .rename(
            Path::new("Notes/Target.md"),
            Path::new("Notes/New.md"),
            true,
        )
        .unwrap();
    assert_eq!(report.updated_links, 0);
    assert_eq!(get(&root, "Broken.canvas"), broken);
    assert_eq!(get(&root, "List.canvas"), not_canvas);
}

#[test]
fn rename_keeps_every_other_byte_and_escapes_the_new_path() {
    let dir = test_dir("canvas-rewrite-bytes");
    let root = dir.join("vault");
    put(&root, "Notes/Target.md", "# Target\n");
    put(&root, "img/pic.png", "PNG");
    let text = "{ \"edges\" : [ ],\r\n  \"nodes\": [\r\n    { \"type\" : \"text\", \"text\" : \"caf\\u00e9 \\/ Notes/Target.md\" },\r\n    {\"subpath\":\"#^blk\",\"file\":\"Notes/Target.md\",\"type\":\"file\"},\r\n    {\"type\":\"file\",\"file\":\"img/pic.png\",\"color\":\"1\"}\r\n  ]\r\n}\r\n";
    put(&root, "Board.canvas", text);
    let v = open(&dir, &root);
    let renamed = "Notes/Said \"hi\" \\ é.md";
    let report = v
        .rename(Path::new("Notes/Target.md"), Path::new(renamed), true)
        .unwrap();
    assert_eq!(report.updated_links, 1);
    assert!(report.updated_notes.is_empty(), "a canvas is no note");
    let expected = text.replacen(
        "\"file\":\"Notes/Target.md\"",
        "\"file\":\"Notes/Said \\\"hi\\\" \\\\ é.md\"",
        1,
    );
    assert_eq!(get(&root, "Board.canvas"), expected);
    let board = canvas(&v, "Board.canvas");
    let target = note(&v, renamed);
    assert_eq!(
        v.links_of(board)[0].1,
        LinkTarget::Note {
            id: target,
            anchor: Some("^blk".into())
        },
        "the subpath is kept and the new path resolves"
    );

    v.rename(Path::new("img/pic.png"), Path::new("img/photo.png"), true)
        .unwrap();
    assert_eq!(
        get(&root, "Board.canvas"),
        expected.replacen("img/pic.png", "img/photo.png", 1)
    );
}

#[test]
fn a_folder_move_rewrites_paths_and_moves_the_canvas_inside() {
    let dir = test_dir("canvas-folder");
    let root = dir.join("vault");
    put(&root, "Project/Note.md", "# Note\n");
    put(&root, "Project/pic.png", "PNG");
    put(&root, "Elsewhere.md", "# Elsewhere\n");
    let inner = r#"{"nodes":[{"type":"file","file":"Project/Note.md"},{"type":"file","file":"Project/pic.png"},{"type":"file","file":"Elsewhere.md"}]}"#;
    put(&root, "Project/Plan.canvas", inner);
    put(
        &root,
        "Top.canvas",
        r#"{"nodes":[{"type":"file","file":"Project/Plan.canvas"},{"type":"file","file":"Project/Note.md"}]}"#,
    );
    let v = open(&dir, &root);
    let plan = canvas(&v, "Project/Plan.canvas");
    v.rename(Path::new("Project"), Path::new("Archive/Project"), true)
        .unwrap();
    assert!(!root.join("Project").exists());
    assert_eq!(
        get(&root, "Archive/Project/Plan.canvas"),
        inner.replace("\"Project/", "\"Archive/Project/")
    );
    assert_eq!(
        get(&root, "Top.canvas"),
        r#"{"nodes":[{"type":"file","file":"Archive/Project/Plan.canvas"},{"type":"file","file":"Archive/Project/Note.md"}]}"#
    );
    assert_eq!(
        v.canvas_by_path(Path::new("Archive/Project/Plan.canvas")),
        Some(plan)
    );
    assert!(v.canvas_by_path(Path::new("Project/Plan.canvas")).is_none());
    assert!(v.unresolved(plan).is_empty());
    let moved = note(&v, "Archive/Project/Note.md");
    assert_eq!(v.backlinks(moved).len(), 2);

    v.rename(Path::new("Elsewhere.md"), Path::new("Gone.md"), false)
        .unwrap();
    assert_eq!(v.unresolved(plan).len(), 1);
}

#[test]
fn the_cache_keeps_canvases_and_updates_them_by_difference() {
    let (dir, root) = mixed("canvas-cache");
    let (board, links) = {
        let v = open(&dir, &root);
        let board = canvas(&v, "Boards/Mixed.canvas");
        (board, v.links_of(board))
    };
    let v = open(&dir, &root);
    assert_eq!(v.stats().reparsed, 0);
    assert_eq!(canvas(&v, "Boards/Mixed.canvas"), board, "the id is kept");
    assert_eq!(v.links_of(board), links);
    drop(v);

    put(
        &root,
        "Boards/Mixed.canvas",
        r#"{"nodes":[{"type":"file","file":"Notes/Other.md"}]}"#,
    );
    let v = open(&dir, &root);
    assert_eq!(v.links_of(board).len(), 1);
    assert!(
        v.backlinks(note(&v, "Notes/Target.md"))
            .iter()
            .all(|b| b.source != board)
    );
    assert_eq!(v.backlinks(note(&v, "Notes/Other.md")).len(), 1);
}

#[test]
fn a_rewritten_canvas_keeps_a_version() {
    let dir = test_dir("canvas-history");
    let root = dir.join("vault");
    put(&root, "Notes/Target.md", "# Target\n");
    put(&root, "Board.canvas", BOARD);
    let v = open(&dir, &root);
    v.set_history(&dir.join("history"), HistoryConfig::default())
        .unwrap();
    v.rename(
        Path::new("Notes/Target.md"),
        Path::new("Notes/Renamed.md"),
        true,
    )
    .unwrap();
    let history = v.history().unwrap();
    let versions = history.list(Path::new("Board.canvas")).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(
        history
            .read(Path::new("Board.canvas"), &versions[0].id)
            .unwrap(),
        BOARD
    );
    assert!(v.take_history_error().is_none());
}

#[test]
fn the_watcher_reads_a_canvas_changed_on_disk() {
    let dir = test_dir("canvas-watch");
    let root = dir.join("vault");
    put(&root, "Notes/Target.md", "# Target\n");
    let v = Vault::open(&root, options(&dir, true)).unwrap();
    let rx = v.subscribe();
    assert!(
        v.inner
            .watcher
            .lock()
            .as_ref()
            .is_some_and(|w| w.wait_ready(Duration::from_secs(10)))
    );
    let target = note(&v, "Notes/Target.md");
    put(&root, "Board.canvas", BOARD);
    let deadline = Instant::now() + Duration::from_secs(10);
    while v.backlinks(target).is_empty() {
        assert!(Instant::now() < deadline, "canvas not indexed");
        let _ = rx.recv_timeout(Duration::from_millis(50));
    }
    let board = canvas(&v, "Board.canvas");
    put(&root, "Board.canvas", "{\"nodes\":[]}");
    while !v.backlinks(target).is_empty() {
        assert!(Instant::now() < deadline, "canvas change not seen");
        let _ = rx.recv_timeout(Duration::from_millis(50));
    }
    assert_eq!(canvas(&v, "Board.canvas"), board);
    std::fs::remove_file(root.join("Board.canvas")).unwrap();
    while v.canvas_by_path(Path::new("Board.canvas")).is_some() {
        assert!(Instant::now() < deadline, "canvas removal not seen");
        let _ = rx.recv_timeout(Duration::from_millis(50));
    }
}

fn snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, (Option<Vec<u8>>, SystemTime)> {
    fn walk_into(
        dir: &Path,
        out: &mut std::collections::BTreeMap<PathBuf, (Option<Vec<u8>>, SystemTime)>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::metadata(&path).unwrap();
            let bytes = meta.is_file().then(|| std::fs::read(&path).unwrap());
            out.insert(path.clone(), (bytes, meta.modified().unwrap()));
            if meta.is_dir() {
                walk_into(&path, out);
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk_into(root, &mut out);
    out
}

fn rewrite_vault(name: &str) -> (PathBuf, PathBuf, Arc<Vault>) {
    let dir = test_dir(name);
    let root = dir.join("vault");
    put(&root, "Notes/Target.md", "# Target\n");
    put(
        &root,
        "Notes/Relative.md",
        "See [x](Target.md) and itself.\n",
    );
    put(&root, "ByName.md", "See [[Target]].\n");
    put(
        &root,
        "ByPath.md",
        "See [[Notes/Target]] twice [[Notes/Target|again]].\n",
    );
    put(&root, "Board.canvas", BOARD);
    let v = open(&dir, &root);
    (dir, root, v)
}

#[test]
fn preview_names_what_the_rename_then_rewrites() {
    let (dir, root, v) = rewrite_vault("canvas-preview-equals-rename");
    let (from, to) = (Path::new("Notes/Target.md"), Path::new("Archive/Target.md"));
    let before = snapshot(&root);
    let rx = v.subscribe();
    let preview = v.preview_rename(from, to).unwrap();
    assert_eq!(snapshot(&root), before, "the preview touched the vault");
    assert!(rx.try_recv().is_err(), "the preview emitted an event");
    assert!(!root.join("Archive").exists());
    assert!(
        preview
            .rewritten
            .iter()
            .all(|r| r.path != Path::new("ByName.md")),
        "a bare name that still resolves is not rewritten"
    );
    assert!(
        preview
            .rewritten
            .iter()
            .any(|r| r.canvas && r.path == Path::new("Board.canvas"))
    );

    let real = v.rename(from, to, true).unwrap();
    let figures = |r: &MoveReport| {
        r.rewritten
            .iter()
            .map(|w| (w.id, w.path.clone(), w.links, w.canvas))
            .collect::<Vec<_>>()
    };
    assert_eq!(figures(&preview), figures(&real));
    assert_eq!(preview.updated_links, real.updated_links);
    assert_eq!(preview.updated_notes, real.updated_notes);
    assert_eq!(
        real.updated_links, 4,
        "two paths, one relative, one canvas card"
    );
    assert_eq!(get(&root, "ByName.md"), "See [[Target]].\n");
    drop(dir);
}

#[test]
fn preview_refuses_as_rename_does() {
    let (_dir, root, v) = rewrite_vault("canvas-preview-refusals");
    put(&root, "Taken.md", "taken\n");
    let v = {
        drop(v);
        open(root.parent().unwrap(), &root)
    };
    let cases = [
        ("Notes/Target.md", "Taken.md"),
        ("Notes", "Notes/Inside"),
        ("Missing.md", "Other.md"),
        ("Notes/Target.md", "../escape.md"),
    ];
    for (from, to) in cases {
        let preview = v.preview_rename(Path::new(from), Path::new(to));
        let real = v.rename(Path::new(from), Path::new(to), true);
        assert_eq!(
            preview.as_ref().map(|_| ()).map_err(|e| e.to_string()),
            real.as_ref().map(|_| ()).map_err(|e| e.to_string()),
            "{from} to {to}"
        );
        assert!(real.is_err(), "{from} to {to}");
    }
    let same = v
        .preview_rename(Path::new("Taken.md"), Path::new("Taken.md"))
        .unwrap();
    assert!(same.rewritten.is_empty() && same.updated_links == 0);
}
