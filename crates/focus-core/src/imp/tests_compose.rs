use super::*;

fn put(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn get(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap()
}

fn open(name: &str, notes: &[(&str, &str)]) -> (PathBuf, PathBuf, Arc<Vault>) {
    let dir = test_dir(name);
    let root = dir.join("vault");
    for (rel, content) in notes {
        put(&root, rel, content);
    }
    let v = Vault::open(
        &root,
        OpenOptions {
            cache_dir: dir.join("cache"),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    v.set_history(&dir.join("history"), HistoryConfig::default())
        .unwrap();
    (dir, root, v)
}

fn id(v: &Vault, p: &str) -> NoteId {
    v.note_by_path(Path::new(p)).unwrap()
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>, SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
                out.push((path.clone(), std::fs::read(&path).unwrap(), modified));
            }
        }
    }
    out.sort();
    out
}

fn kept(history: &Path) -> Vec<(PathBuf, Vec<u8>, SystemTime)> {
    let mut files = snapshot(history);
    files.retain(|(path, _, _)| path.file_name().is_none_or(|name| name != "lock"));
    files
}

const TAGGED: &[(&str, &str)] = &[
    (
        "List.md",
        "---\ntags:\n  - red/ad\n  - blue\n---\n# List\nBody #red and #red/web, not #redder.\n",
    ),
    (
        "Line.md",
        "---\ntags: red, blue\n---\nInline `#red` and https://example.org/#red stay.\n```\n#red\n```\n",
    ),
    ("Flow.md", "---\ntags: [RED/x, green]\n---\n#Red/x/deep\n"),
    ("Merge.md", "---\ntags: [red, blue]\n---\n#red #blue\n"),
    ("None.md", "# Nothing\n#redder #blue\n"),
];

#[test]
fn a_tag_is_renamed_in_bodies_and_properties_with_its_subtags() {
    let (_, root, v) = open("retag", TAGGED);
    let report = v.rename_tag("red", "team/red").unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        get(&root, "List.md"),
        "---\ntags:\n  - team/red/ad\n  - blue\n---\n# List\nBody #team/red and #team/red/web, not #redder.\n"
    );
    assert_eq!(
        get(&root, "Line.md"),
        "---\ntags: team/red, blue\n---\nInline `#red` and https://example.org/#red stay.\n```\n#red\n```\n"
    );
    assert_eq!(
        get(&root, "Flow.md"),
        "---\ntags: [team/red/x, green]\n---\n#team/red/x/deep\n"
    );
    assert_eq!(get(&root, "None.md"), "# Nothing\n#redder #blue\n");
    assert_eq!(report.notes.len(), 4);
    assert_eq!(report.occurrences, 3 + 1 + 2 + 2);
    let note = v.note(id(&v, "Flow.md")).unwrap();
    assert_eq!(note.tags, ["team/red/x", "green", "team/red/x/deep"]);
    assert!(v.notes_with_tag("red").is_empty());
    assert_eq!(v.notes_with_tag("team/red").len(), 4);
}

#[test]
fn renaming_onto_an_existing_tag_merges_them() {
    let (_, root, v) = open("retag-merge", TAGGED);
    v.rename_tag("red", "blue").unwrap();
    assert_eq!(
        get(&root, "Merge.md"),
        "---\ntags: [blue]\n---\n#blue #blue\n"
    );
    assert_eq!(get(&root, "Line.md").lines().nth(1), Some("tags: blue"));
    assert_eq!(
        get(&root, "List.md"),
        "---\ntags:\n  - blue/ad\n  - blue\n---\n# List\nBody #blue and #blue/web, not #redder.\n"
    );
    assert_eq!(v.note(id(&v, "Merge.md")).unwrap().tags, ["blue"]);
}

#[test]
fn an_invalid_name_is_refused_with_nothing_written() {
    let (_, root, v) = open("retag-invalid", TAGGED);
    let before = snapshot(&root);
    for bad in ["two words", "#red", "1984", "a//b", "", "end/"] {
        assert!(v.rename_tag("red", bad).is_err(), "{bad}");
        assert!(v.preview_rename_tag("red", bad).is_err(), "{bad}");
    }
    assert!(v.rename_tag("not a tag", "fine").is_err());
    assert_eq!(snapshot(&root), before);
}

#[test]
fn the_preview_gives_the_figures_of_the_rename_and_writes_nothing() {
    let (dir, root, v) = open("retag-preview", TAGGED);
    let before = snapshot(&root);
    let history = kept(&dir.join("history"));
    let rx = v.subscribe();
    let preview = v.preview_rename_tag("red", "crimson").unwrap();
    assert_eq!(snapshot(&root), before);
    assert_eq!(kept(&dir.join("history")), history);
    assert!(rx.try_recv().is_err());
    let done = v.rename_tag("red", "crimson").unwrap();
    assert_eq!(preview.notes, done.notes);
    assert_eq!(preview.occurrences, done.occurrences);
    assert_eq!(done.notes.len(), 4);
}

#[test]
fn each_note_rewritten_keeps_a_version() {
    let (_, root, v) = open("retag-versions", TAGGED);
    let before: Vec<(&str, String)> = ["List.md", "Line.md", "Flow.md", "Merge.md"]
        .iter()
        .map(|p| (*p, get(&root, p)))
        .collect();
    v.rename_tag("red", "crimson").unwrap();
    let history = v.history().unwrap();
    for (path, text) in before {
        let versions = history.list(Path::new(path)).unwrap();
        assert_eq!(versions.len(), 1, "{path}");
        assert_eq!(versions[0].kind, HistoryKind::Bulk);
        assert_eq!(
            history.read(Path::new(path), &versions[0].id).unwrap(),
            text
        );
    }
    assert!(history.list(Path::new("None.md")).unwrap().is_empty());
}

#[test]
fn a_note_that_cannot_be_read_is_reported_and_the_others_go_through() {
    let (_, root, v) = open("retag-failed", TAGGED);
    std::fs::write(root.join("Flow.md"), b"#red \xff\n").unwrap();
    let report = v.rename_tag("red", "crimson").unwrap();
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, PathBuf::from("Flow.md"));
    assert_eq!(report.notes.len(), 3);
    assert_eq!(std::fs::read(root.join("Flow.md")).unwrap(), b"#red \xff\n");
}

const COMPOSED: &[(&str, &str)] = &[
    ("Dest.md", "---\ntags: [a]\n---\n# Dest\nSee [[Src]].\n"),
    (
        "Notes/Src.md",
        "---\ntags: [b]\naliases: [Source]\n---\n## Part\nSee [up](../Other.md) and [[Src#Part]].\n",
    ),
    (
        "Other.md",
        "[[Src]] and [s](Notes/Src.md) and [[Src#Part|p]] and [[Source]]\n",
    ),
    (
        "Board.canvas",
        r#"{"nodes":[{"id":"1","type":"file","file":"Notes/Src.md","x":0,"y":0,"width":10,"height":10}],"edges":[]}"#,
    ),
];

#[test]
fn a_merge_joins_the_notes_rewrites_links_and_trashes_the_source() {
    let (_, root, v) = open("merge", COMPOSED);
    let src = id(&v, "Notes/Src.md");
    let dest = id(&v, "Dest.md");
    let source_first = get(&root, "Notes/Src.md");
    v.write(src, &source_first.replace("## Part", "## Part\n"), None)
        .unwrap();
    v.write(src, &source_first, None).unwrap();
    let source_before = get(&root, "Notes/Src.md");
    let dest_before = get(&root, "Dest.md");
    let preview = v.preview_merge(src, dest).unwrap();
    let report = v
        .merge(src, dest, Placement::End, TrashKind::Vault)
        .unwrap();
    assert_eq!(preview.links, report.links);
    assert_eq!(report.links, 1 + 1 + 3 + 1);
    assert_eq!(report.trashed.as_deref(), Some(Path::new(".trash/Src.md")));
    assert_eq!(
        get(&root, "Dest.md"),
        "---\ntags:\n  - a\n  - b\naliases: [Source]\n---\n# Dest\nSee [[Dest]].\n\n## Part\nSee [up](Other.md) and [[Dest#Part]].\n"
    );
    assert_eq!(
        get(&root, "Other.md"),
        "[[Dest]] and [s](Dest.md) and [[Dest#Part|p]] and [[Source]]\n"
    );
    assert!(get(&root, "Board.canvas").contains(r#""file":"Dest.md""#));
    assert!(!root.join("Notes/Src.md").exists());
    assert_eq!(get(&root, ".trash/Src.md"), source_before);
    assert!(v.note_by_path(Path::new("Notes/Src.md")).is_none());
    let other = id(&v, "Other.md");
    assert!(v.links_of(other).iter().all(|(_, t)| matches!(
        t,
        LinkTarget::Note { id, .. } if *id == dest
    )));
    let history = v.history().unwrap();
    let kept: Vec<String> = history
        .list(Path::new("Dest.md"))
        .unwrap()
        .iter()
        .map(|version| history.read(Path::new("Dest.md"), &version.id).unwrap())
        .collect();
    assert!(kept.contains(&dest_before) && kept.contains(&source_before));
    assert_eq!(history.list(Path::new("Other.md")).unwrap().len(), 1);
    assert!(history.list(Path::new("Notes/Src.md")).unwrap().is_empty());
    let trashed = history.list(Path::new(".trash/Src.md")).unwrap();
    assert_eq!(trashed.len(), 1);
    assert_eq!(
        history
            .read(Path::new(".trash/Src.md"), &trashed[0].id)
            .unwrap(),
        source_first
    );
}

#[test]
fn a_merge_at_the_start_and_one_into_itself() {
    let (_, root, v) = open("merge-start", COMPOSED);
    let (src, dest) = (id(&v, "Notes/Src.md"), id(&v, "Dest.md"));
    assert!(
        v.merge(dest, dest, Placement::End, TrashKind::Vault)
            .is_err()
    );
    v.merge(src, dest, Placement::Start, TrashKind::Vault)
        .unwrap();
    let text = get(&root, "Dest.md");
    assert!(text.ends_with(
        "---\n## Part\nSee [up](Other.md) and [[Dest#Part]].\n\n# Dest\nSee [[Dest]].\n"
    ));
}

#[test]
fn an_extraction_leaves_a_link_in_place_of_the_text() {
    let (_, root, v) = open("extract", COMPOSED);
    let src = id(&v, "Notes/Src.md");
    let text = get(&root, "Notes/Src.md");
    let selected = "See [up](../Other.md) and [[Src#Part]].";
    let start = text.find(selected).unwrap();
    let moved = v.move_text(src, Path::new("Part.md"), selected);
    assert_eq!(moved, "See [up](Other.md) and [[Src#Part]].");
    let new = v.create_note(Path::new("Part.md"), &moved).unwrap();
    let link = v.link_to(src, new).unwrap();
    assert_eq!(link, "[[Part]]");
    let mut source = text.clone();
    source.replace_range(start..start + selected.len(), &link);
    v.write(src, &source, None).unwrap();
    assert_eq!(
        get(&root, "Notes/Src.md"),
        text.replace(selected, "[[Part]]")
    );
    assert_eq!(get(&root, "Part.md"), moved);
    let dest = id(&v, "Dest.md");
    let before = get(&root, "Dest.md");
    v.insert_text(dest, "Extracted.", Placement::End).unwrap();
    assert_eq!(get(&root, "Dest.md"), format!("{before}\nExtracted.\n"));
    let history = v.history().unwrap();
    let versions = history.list(Path::new("Dest.md")).unwrap();
    assert_eq!(
        history.read(Path::new("Dest.md"), &versions[0].id).unwrap(),
        before
    );
}

#[test]
fn links_follow_the_vault_style() {
    let (_, root, v) = open(
        "link-style",
        &[
            (
                ".obsidian/app.json",
                r#"{"useMarkdownLinks":true,"newLinkFormat":"relative"}"#,
            ),
            ("A/From.md", "x"),
            ("B/To Note.md", "y"),
        ],
    );
    let _ = root;
    assert_eq!(
        v.link_to(id(&v, "A/From.md"), id(&v, "B/To Note.md"))
            .unwrap(),
        "[To Note](../B/To%20Note.md)"
    );
}
