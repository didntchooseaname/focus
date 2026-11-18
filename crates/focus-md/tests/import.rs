use std::io::Write;
use std::path::{Path, PathBuf};

use focus_md::import::{Source, detect, html_to_markdown, plan, run};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("import")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (name, data) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn html_converts_to_markdown() {
    let md = html_to_markdown(
        "<html><head><title>x</title><style>p{}</style></head><body><h1>Title</h1><p>Some <b>bold</b>, <em>it</em> and <a href=\"https://e.org/a b\">a link</a>.<br>Next &amp; line <code>x*y</code></p>\
         <ul><li>one<ul><li>nested</li></ul></li><li>two</li></ul><ol start=\"3\"><li>c</li></ol>\
         <pre><code class=\"language-bash\">echo &lt;a&gt;\nls\n</code></pre><blockquote><p>quoted</p></blockquote><hr>\
         <img src=\"p q.png\" alt=\"pic\"><table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>x|y</td></tr></table><script>alert(1)</script></body></html>",
    );
    assert!(md.contains("# Title"));
    assert!(md.contains("Some **bold**, *it* and [a link](https://e.org/a%20b)."));
    assert!(md.contains("Next & line `x*y`"));
    assert!(md.contains("- one\n  - nested\n- two"));
    assert!(md.contains("3. c"));
    assert!(md.contains("```bash\necho <a>\nls\n```"));
    assert!(md.contains("> quoted"));
    assert!(md.contains("\n---\n"));
    assert!(md.contains("![pic](p%20q.png)"));
    assert!(md.contains("| A | B |\n| --- | --- |\n| 1 | x\\|y |"));
    assert!(!md.contains("alert") && !md.contains("p{}"));
}

#[test]
fn html_with_unclosed_tags_does_not_panic() {
    let md =
        html_to_markdown("<p>one<p>two <b>bold <i>both</b> end</i><li>orphan<table><tr><td>a<td>b");
    assert!(md.contains("one") && md.contains("two") && md.contains("orphan"));
}

#[test]
fn detect_guesses_the_kind_of_a_path() {
    let dir = scratch("detect");
    std::fs::write(dir.join("a.md"), "x").unwrap();
    std::fs::write(dir.join("b.html"), "x").unwrap();
    std::fs::write(dir.join("c.bin"), "x").unwrap();
    make_zip(&dir.join("plain.zip"), &[("notes/a.md", b"# a")]);
    make_zip(
        &dir.join("notion.zip"),
        &[("Page 0123456789abcdef0123456789abcdef.md", b"# p")],
    );
    assert_eq!(detect(&dir), Some(Source::Folder));
    assert_eq!(detect(&dir.join("a.md")), Some(Source::Files));
    assert_eq!(detect(&dir.join("b.html")), Some(Source::Files));
    assert_eq!(detect(&dir.join("c.bin")), None);
    assert_eq!(detect(&dir.join("plain.zip")), Some(Source::Zip));
    assert_eq!(detect(&dir.join("notion.zip")), Some(Source::Notion));
    assert_eq!(detect(&dir.join("missing.zip")), None);
}

#[test]
fn folder_import_copies_and_never_overwrites() {
    let root = scratch("folder");
    let source = root.join("vault");
    std::fs::create_dir_all(source.join("sub")).unwrap();
    std::fs::create_dir_all(source.join(".obsidian")).unwrap();
    std::fs::write(source.join("a.md"), "# a").unwrap();
    std::fs::write(source.join("sub/b.md"), "# b").unwrap();
    std::fs::write(source.join("sub/img.png"), [1, 2, 3]).unwrap();
    std::fs::write(source.join(".obsidian/app.json"), "{}").unwrap();
    let destination = root.join("dest");
    let first = run(Source::Folder, std::slice::from_ref(&source), &destination).unwrap();
    assert_eq!(first.notes.len(), 2);
    assert_eq!(first.attachments.len(), 1);
    assert_eq!(first.skipped.len(), 1);
    assert_eq!(
        std::fs::read_to_string(destination.join("vault/sub/b.md")).unwrap(),
        "# b"
    );
    let second = run(Source::Folder, &[source], &destination).unwrap();
    assert!(
        second
            .notes
            .iter()
            .all(|p| p.starts_with(destination.join("vault 2")))
    );
    assert!(destination.join("vault/a.md").exists() && destination.join("vault 2/a.md").exists());
}

#[test]
fn zip_import_blocks_path_traversal() {
    let root = scratch("zip");
    let archive = root.join("in.zip");
    make_zip(
        &archive,
        &[
            ("n/a.md", b"# a"),
            ("n/pic.png", b"png"),
            ("../evil.md", b"x"),
            ("/abs.md", b"x"),
            ("__MACOSX/._a.md", b"x"),
        ],
    );
    let destination = root.join("dest");
    let report = run(Source::Zip, &[archive], &destination).unwrap();
    assert_eq!(report.notes.len(), 1);
    assert_eq!(report.attachments.len(), 1);
    assert_eq!(report.skipped.len(), 3);
    assert!(!root.join("evil.md").exists() && !destination.join("evil.md").exists());
    assert_eq!(
        std::fs::read_to_string(destination.join("in/n/a.md")).unwrap(),
        "# a"
    );
}

#[test]
fn notion_export_gets_clean_names_links_and_tables() {
    let root = scratch("notion");
    let archive = root.join("Export-1.zip");
    let id = "0123456789abcdef0123456789abcdef";
    let id2 = "fedcba9876543210fedcba9876543210";
    let id3 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let page = format!(
        "# Home\n\nSee [Sub page](Home%20{id}/Sub%20page%20{id2}.md#top) and [site](https://e.org) and [db](Tasks%20{id3}.csv).\n"
    );
    let sub = "# Sub\n\n![shot](shot%20one.png)\n".to_string();
    let csv = "Name,Notes\nA,\"x, y\"\nB,\"two\nlines\"\n";
    make_zip(
        &archive,
        &[
            (&format!("Home {id}.md"), page.as_bytes()),
            (&format!("Home {id}/Sub page {id2}.md"), sub.as_bytes()),
            (&format!("Home {id}/shot one.png"), b"png"),
            (&format!("Tasks {id3}.csv"), csv.as_bytes()),
            (&format!("Tasks {id3}_all.csv"), csv.as_bytes()),
        ],
    );
    let destination = root.join("dest");
    let report = run(Source::Notion, &[archive], &destination).unwrap();
    assert_eq!(report.notes.len(), 3, "{report:?}");
    assert_eq!(report.attachments.len(), 1);
    assert_eq!(report.skipped.len(), 1);
    let base = destination.join("Export-1");
    let home = std::fs::read_to_string(base.join("Home.md")).unwrap();
    assert!(
        home.contains("[Sub page](Home/Sub%20page.md#top)"),
        "{home}"
    );
    assert!(home.contains("[site](https://e.org)"));
    assert!(home.contains("[db](Tasks.md)"));
    assert!(base.join("Home/shot one.png").exists());
    let table = std::fs::read_to_string(base.join("Tasks.md")).unwrap();
    assert_eq!(
        table,
        "| Name | Notes |\n| --- | --- |\n| A | x, y |\n| B | two<br>lines |\n"
    );
}

#[test]
fn single_files_are_imported_as_notes() {
    let root = scratch("files");
    std::fs::write(root.join("a.txt"), "plain text").unwrap();
    std::fs::write(root.join("b.html"), "<h2>Hi</h2><p>there</p>").unwrap();
    std::fs::write(root.join("c.md"), "# c").unwrap();
    std::fs::write(root.join("d.bin"), "x").unwrap();
    let destination = root.join("dest");
    let inputs = ["a.txt", "b.html", "c.md", "d.bin"].map(|n| root.join(n));
    let report = run(Source::Files, &inputs, &destination).unwrap();
    assert_eq!(report.notes.len(), 3);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(
        std::fs::read_to_string(destination.join("a.md")).unwrap(),
        "plain text"
    );
    assert_eq!(
        std::fs::read_to_string(destination.join("b.md"))
            .unwrap()
            .trim(),
        "## Hi\n\nthere"
    );
    let again = run(Source::Files, &[root.join("c.md")], &destination).unwrap();
    assert!(again.notes[0].ends_with("c 2.md"));
}

#[test]
fn a_dry_run_gives_the_names_the_import_will_use_and_writes_nothing() {
    let root = scratch("plan");
    let archive = root.join("Export-1.zip");
    let id = "0123456789abcdef0123456789abcdef";
    make_zip(
        &archive,
        &[
            (&format!("Home {id}.md"), b"# h"),
            (&format!("Home {id}/pic.png"), b"png"),
            (&format!("Other {id}.md"), b"# o"),
        ],
    );
    let destination = root.join("dest");
    let planned = plan(Source::Notion, std::slice::from_ref(&archive), &destination).unwrap();
    assert!(!destination.exists(), "nothing is written");
    let mut names: Vec<String> = planned
        .notes
        .iter()
        .chain(&planned.attachments)
        .map(|p| p.strip_prefix(&destination).unwrap().display().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "Export-1/Home.md",
            "Export-1/Home/pic.png",
            "Export-1/Other.md"
        ]
    );
    let done = run(Source::Notion, &[archive], &destination).unwrap();
    assert_eq!(
        (done.notes.len(), done.attachments.len()),
        (planned.notes.len(), planned.attachments.len())
    );
}

#[test]
fn a_dry_run_numbers_taken_names_like_the_import() {
    let root = scratch("plan-names");
    std::fs::write(root.join("a.md"), "x").unwrap();
    let destination = root.join("dest");
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(destination.join("a.md"), "already here").unwrap();
    let planned = plan(
        Source::Files,
        &[root.join("a.md"), root.join("a.md")],
        &destination,
    )
    .unwrap();
    let names: Vec<_> = planned
        .notes
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["a 2.md", "a 3.md"]);
    assert_eq!(
        std::fs::read_to_string(destination.join("a.md")).unwrap(),
        "already here"
    );
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path))
        } else {
            out.push(path)
        }
    }
    out
}

#[test]
fn zip_entries_cannot_leave_the_destination_by_any_spelling() {
    let root = scratch("slip");
    let archive = root.join("in.zip");
    make_zip(
        &archive,
        &[
            ("ok.md", b"# ok"),
            ("../one.md", b"x"),
            ("a/../../two.md", b"x"),
            ("a/b/../../../three.md", b"x"),
            ("/abs/four.md", b"x"),
            ("..\\five.md", b"x"),
            ("a\\..\\..\\six.md", b"x"),
            ("C:\\seven.md", b"x"),
            ("C:/eight.md", b"x"),
            ("\\\\server\\share\\nine.md", b"x"),
            ("././/ten.md", b"x"),
        ],
    );
    let destination = root.join("dest");
    let report = run(Source::Zip, &[archive], &destination).unwrap();
    let written = files_under(&root);
    for file in &written {
        assert!(
            file.starts_with(root.join("dest")) || file.ends_with("in.zip"),
            "{} is outside the destination",
            file.display()
        );
    }
    let names: Vec<_> = report
        .notes
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["ok.md", "ten.md"], "{names:?} {:?}", report.skipped);
    assert_eq!(
        report
            .skipped
            .iter()
            .filter(|(_, why)| why == "path leaves the destination")
            .count(),
        9
    );
}

#[test]
fn symbolic_links_in_an_archive_are_not_extracted() {
    let root = scratch("zip-symlink");
    let archive = root.join("in.zip");
    {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("real.md", options).unwrap();
        writer.write_all(b"# real").unwrap();
        writer
            .add_symlink("link.md", "/etc/passwd", options)
            .unwrap();
        writer.add_symlink("dir", "../../outside", options).unwrap();
        writer.finish().unwrap();
    }
    let destination = root.join("dest");
    let report = run(Source::Zip, &[archive], &destination).unwrap();
    assert_eq!(report.notes.len(), 1);
    assert_eq!(
        report
            .skipped
            .iter()
            .filter(|(_, why)| why == "symbolic link")
            .count(),
        2
    );
    assert!(!destination.join("in/link.md").exists() && !destination.join("in/dir").exists());
}

#[test]
fn nothing_that_exists_is_overwritten() {
    let root = scratch("overwrite");
    let destination = root.join("dest");
    std::fs::create_dir_all(destination.join("in")).unwrap();
    std::fs::write(destination.join("in/a.md"), "mine").unwrap();
    let archive = root.join("in.zip");
    make_zip(&archive, &[("a.md", b"theirs"), ("b.md", b"other")]);
    let report = run(Source::Zip, &[archive], &destination).unwrap();
    assert_eq!(
        std::fs::read_to_string(destination.join("in/a.md")).unwrap(),
        "mine"
    );
    assert_eq!(report.notes.len(), 2);
    let outside = root.join("outside.txt");
    std::os::unix::fs::symlink(&outside, destination.join("dangling.md")).unwrap();
    let single = root.join("dangling.md");
    std::fs::write(&single, "text").unwrap();
    let result = run(Source::Files, &[single], &destination);
    assert!(!outside.exists(), "the dangling link was followed");
    if let Ok(report) = result {
        assert!(
            report
                .notes
                .iter()
                .all(|p| p.file_name().unwrap() != "dangling.md")
        );
    }
}

#[test]
fn a_folder_import_skips_links_and_special_files() {
    let root = scratch("folder-special");
    let source = root.join("src");
    std::fs::create_dir_all(source.join("sub")).unwrap();
    std::fs::write(source.join("a.md"), "# a").unwrap();
    std::fs::write(root.join("secret.txt"), "secret").unwrap();
    std::os::unix::fs::symlink(root.join("secret.txt"), source.join("leak.md")).unwrap();
    std::os::unix::fs::symlink(&root, source.join("up")).unwrap();
    std::os::unix::fs::symlink("nowhere", source.join("sub/broken.md")).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(source.join("pipe.md"))
            .status()
            .unwrap()
            .success()
    );
    let destination = root.join("dest");
    let report = run(Source::Folder, &[source], &destination).unwrap();
    let written: Vec<_> = files_under(&destination)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(written, ["a.md"]);
    let reasons: Vec<_> = report.skipped.iter().map(|(_, why)| why.as_str()).collect();
    assert_eq!(
        reasons.iter().filter(|r| **r == "symbolic link").count(),
        3,
        "{reasons:?}"
    );
    assert!(reasons.contains(&"not a regular file"), "{reasons:?}");
}

#[test]
fn a_folder_cannot_be_imported_into_itself() {
    let root = scratch("self-import");
    let source = root.join("vault");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.md"), "# a").unwrap();
    let error = run(
        Source::Folder,
        std::slice::from_ref(&source),
        &source.join("Imports"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("inside the folder"), "{error}");
    assert!(
        plan(
            Source::Folder,
            std::slice::from_ref(&source),
            &source.join("deeper/still")
        )
        .is_err()
    );
    assert_eq!(files_under(&source).len(), 1, "nothing was copied");
}

#[test]
fn a_special_file_is_not_read_as_a_note() {
    let root = scratch("fifo-file");
    let pipe = root.join("pipe.md");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .unwrap()
            .success()
    );
    let report = run(Source::Files, &[pipe], &root.join("dest")).unwrap();
    assert!(report.notes.is_empty());
    assert_eq!(report.skipped[0].1, "not a regular file");
}

#[test]
fn html_import_keeps_no_active_content() {
    let md = html_to_markdown(
        "<p><a href=\"javascript:alert(1)\">js</a> <a href=\"JaVa\tScRiPt:alert(2)\">js2</a> <a href=\"https://ok.org/\">ok</a> <a href=\"data:text/html,x\">data</a> <a href=\"file:///etc/passwd\">file</a>\
         <img src=\"javascript:alert(3)\" alt=\"i\"><img src=\"https://ok.org/a.png\" alt=\"i2\"></p><iframe src=\"https://evil/\"></iframe><object data=\"x\"></object><script>alert(4)</script><style>p{background:url(https://evil/)}</style>\
         <div onclick=\"alert(5)\">kept text</div><svg onload=\"alert(6)\"><text>svg text</text></svg>",
    );
    let lower = md.to_lowercase();
    assert!(
        !lower.contains("javascript:")
            && !lower.contains("script:")
            && !lower.contains("alert")
            && !lower.contains("data:text")
            && !lower.contains("<"),
        "{md}"
    );
    assert!(
        md.contains("https://ok.org/")
            && md.contains("https://ok.org/a.png")
            && md.contains("kept text"),
        "{md}"
    );
}
