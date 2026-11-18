use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_bases::{
    Base, BasesError, Cell, Drawn, EvalError, Feature, QueryError, TableRow, Typed, Value,
    evaluate, load, table,
};
use focus_core::{OpenOptions, Vault};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("bases-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn fixture(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn vault(name: &str) -> (Arc<Vault>, PathBuf) {
    let root = scratch(name);
    write(
        &root,
        "Red/Alpha.md",
        "---\nstatus: draft\ntype: note\ndomain: ad\n---\n# Alpha\n#attack/ad\n[[Beta]]\n",
    );
    write(
        &root,
        "Red/Beta.md",
        "---\nstatus: ready\ntype: note\ndomain: web\nlast_verified: 2026-01-02\n---\nBeta\n",
    );
    write(
        &root,
        "Red/Gamma.md",
        "---\nstatus: historical\ntype: index\n---\nGamma\n",
    );
    write(
        &root,
        "Labs/Network/Delta.md",
        "---\nstatus: draft\ntype: note\nhostname: dc01\nip_address: 10.0.0.5\n---\nDelta\n",
    );
    write(
        &root,
        "Labs/Network/Epsilon.md",
        "---\nhostname: web01\n---\nEpsilon\n",
    );
    write(&root, "Blue/Zeta.md", "---\nstatus: reference\n---\nZeta\n");
    write(&root, "Other/Eta.md", "---\nstatus: draft\n---\nEta\n");
    write(&root, "Operations.base", &fixture("operations.base"));
    write(&root, "Targets.base", &fixture("targets.base"));
    let options = OpenOptions {
        cache_dir: scratch(&format!("{name}-cache")),
        ignore: Vec::new(),
        watch: false,
    };
    (Vault::open(&root, options).unwrap(), root)
}

fn rows(table: &focus_bases::Table) -> Vec<&TableRow> {
    table.groups.iter().flat_map(|g| &g.rows).collect()
}

fn name(row: &TableRow) -> String {
    match &row.cells[0] {
        Ok(Value::Text(s)) => s.clone(),
        other => panic!("{other:?}"),
    }
}

fn names(table: &focus_bases::Table) -> Vec<String> {
    rows(table).into_iter().map(name).collect()
}

fn text(s: &str) -> Drawn {
    Ok(Value::Text(s.into()))
}

#[test]
fn the_view_filter_and_the_base_filter_both_apply() {
    let (vault, _) = vault("filters");
    let todo = table(&vault, Path::new("Operations.base"), Some("À traiter")).unwrap();
    assert_eq!(names(&todo), ["Alpha", "Delta"]);
    assert_eq!(todo.matched, 2);
    let ready = table(&vault, Path::new("Operations.base"), Some("Prêtes")).unwrap();
    assert_eq!(names(&ready), ["Beta"]);
    let reference = table(&vault, Path::new("Operations.base"), Some("Références")).unwrap();
    assert_eq!(names(&reference), ["Zeta"]);
}

#[test]
fn columns_follow_order_with_their_display_names() {
    let (vault, _) = vault("columns");
    let todo = table(&vault, Path::new("Operations.base"), Some("À traiter")).unwrap();
    let columns: Vec<_> = todo
        .columns
        .iter()
        .map(|c| (c.id.as_str(), c.label.as_str(), c.named, c.link))
        .collect();
    assert_eq!(
        columns,
        [
            ("file.name", "name", false, true),
            ("note.status", "Statut", true, false),
            ("note.type", "Type", true, false),
            ("note.domain", "Domaine", true, false),
            ("file.mtime", "Modifiée le", true, false),
        ]
    );
    let alpha = rows(&todo)[0];
    assert_eq!(alpha.cells[1], text("draft"));
    assert_eq!(alpha.cells[3], text("ad"));
    assert!(matches!(alpha.cells[4], Ok(Value::Date(_))));
    assert_eq!(
        vault.note(alpha.note).unwrap().path.to_string_lossy(),
        "Red/Alpha.md"
    );
}

#[test]
fn a_view_without_filter_lists_what_the_base_filter_keeps() {
    let (vault, _) = vault("targets");
    let targets = table(&vault, Path::new("Targets.base"), None).unwrap();
    assert_eq!(names(&targets), ["Delta", "Epsilon"]);
    assert_eq!(rows(&targets)[0].cells[2], text("10.0.0.5"));
    assert_eq!(rows(&targets)[1].cells[2], Ok(Value::Null));
    assert_eq!(targets.groups.len(), 1);
    assert_eq!(targets.groups[0].key, None);
    assert!(targets.groups[0].summaries.is_empty());
    assert_eq!(targets.columns[2].label, "Adresse IP");
}

#[test]
fn sort_not_tags_links_and_limit() {
    let (vault, root) = vault("sort");
    write(
        &root,
        "Sorted.base",
        "filters:\n  not:\n    - 'file.inFolder(\"Other\")'\n    - 'status.isEmpty()'\nviews:\n  - type: table\n    name: A\n    order: [file.name, status]\n    sort:\n      - property: status\n        direction: DESC\n      - property: file.name\n        direction: ASC\n    limit: 3\n  - type: table\n    name: B\n    filters: 'file.hasTag(\"attack\") || file.hasLink(\"Beta\")'\n  - type: table\n    name: C\n    filters: 'file.hasTag(\"attack\")'\n",
    );
    let sorted = table(&vault, Path::new("Sorted.base"), Some("A")).unwrap();
    assert_eq!(sorted.matched, 5);
    assert_eq!(names(&sorted), ["Zeta", "Beta", "Gamma"]);
    let linking = table(&vault, Path::new("Sorted.base"), Some("B")).unwrap();
    assert_eq!(names(&linking), ["Alpha"]);
    let tagged = table(&vault, Path::new("Sorted.base"), Some("C")).unwrap();
    assert_eq!(names(&tagged), ["Alpha"]);
}

#[test]
fn what_the_table_cannot_draw_is_named_and_gives_no_table() {
    let (vault, root) = vault("unsupported");
    write(
        &root,
        "Bad.base",
        "formulas:\n  age: file.mtime\n  loop: formula.loop + 1\n  wrong: status * 2\n  mark: 'html(\"<b>x</b>\")'\n  mine: this.file.name\nsummaries:\n  Mark: 'html(\"<b>x</b>\")'\nviews:\n  - type: cards\n    name: Cards\n  - type: list\n    name: List\n  - type: kanban\n    name: Kanban\n    groupBy:\n      property: note.status\n  - type: map\n    name: Map\n  - type: table\n    name: This\n    filters: 'file.hasLink(this.file)'\n  - type: table\n    name: ThisFormula\n    order: [formula.mine]\n  - type: table\n    name: Loop\n    order: [formula.loop]\n  - type: table\n    name: Missing formula\n    filters: 'formula.nope > 1'\n  - type: table\n    name: Wrong\n    order: [file.name, formula.wrong]\n  - type: table\n    name: Filter\n    filters: 'formula.wrong > 1'\n  - type: table\n    name: Grouped wrong\n    groupBy:\n      property: formula.wrong\n  - type: table\n    name: Html\n    order: [formula.mark]\n  - type: table\n    name: Html group\n    groupBy:\n      property: formula.mark\n  - type: table\n    name: Html summary\n    summaries:\n      file.name: Mark\n  - type: table\n    name: No summary\n    summaries:\n      file.name: Nope\n  - type: table\n    name: Fine\n    order: [formula.age]\n",
    );
    let path = Path::new("Bad.base");
    let unsupported = |view: &str| match table(&vault, path, Some(view)) {
        Err(BasesError::Unsupported {
            expression,
            feature,
            subject,
        }) => (expression, feature, subject),
        other => panic!("{view}: {other:?}"),
    };
    let named = |expression: &str, feature: Feature, subject: &str| {
        (expression.to_string(), feature, subject.to_string())
    };
    for (view, kind) in [
        ("Cards", "cards"),
        ("List", "list"),
        ("Kanban", "kanban"),
        ("Map", "map"),
    ] {
        assert!(
            matches!(table(&vault, path, Some(view)), Err(BasesError::UnsupportedViewType(t)) if t == kind),
            "{view}"
        );
    }
    assert_eq!(
        unsupported("This"),
        named("file.hasLink(this.file)", Feature::This, "this")
    );
    assert_eq!(
        unsupported("ThisFormula"),
        named("this.file.name", Feature::This, "this")
    );
    assert_eq!(
        unsupported("Missing formula"),
        named("formula.nope > 1", Feature::Formula, "formula.nope")
    );
    assert_eq!(
        unsupported("Html"),
        named("formula.mark", Feature::Function, "html")
    );
    assert_eq!(
        unsupported("Html group"),
        named("formula.mark", Feature::Function, "html")
    );
    assert_eq!(
        unsupported("Html summary"),
        named("Mark", Feature::Function, "html")
    );
    let invalid = |view: &str| match table(&vault, path, Some(view)) {
        Err(BasesError::Invalid(message)) => message,
        other => panic!("{view}: {other:?}"),
    };
    assert_eq!(invalid("Loop"), "formulas refer to each other: loop, loop");
    assert_eq!(
        invalid("No summary"),
        "no summary named Nope (for file.name)"
    );
    let wrong = "* does not apply to string and number";
    let filter = invalid("Filter");
    assert!(
        filter.starts_with("filter formula.wrong > 1 fails on ") && filter.ends_with(wrong),
        "{filter}"
    );
    let group = invalid("Grouped wrong");
    assert!(
        group.starts_with("group formula.wrong fails on ") && group.ends_with(wrong),
        "{group}"
    );
    let fine = table(&vault, path, Some("Fine")).unwrap();
    assert_eq!(rows(&fine).len(), 7);
    assert!(
        rows(&fine)
            .iter()
            .all(|row| matches!(row.cells[0], Ok(Value::Date(_))))
    );
    assert!(
        matches!(table(&vault, path, Some("Missing")), Err(BasesError::UnknownView(n)) if n == "Missing")
    );
}

#[test]
fn a_cell_that_fails_is_its_error_and_the_other_cells_are_drawn() {
    let (vault, root) = vault("cell-errors");
    write(
        &root,
        "Wrong.base",
        "formulas:\n  wrong: status * 2\nviews:\n  - type: table\n    order: [file.name, formula.wrong]\n    summaries:\n      formula.wrong: Sum\n",
    );
    let wrong = table(&vault, Path::new("Wrong.base"), None).unwrap();
    let failed: Drawn = Err(EvalError::Operator {
        operator: "*",
        left: "string",
        right: "number",
    });
    let cells: Vec<_> = rows(&wrong)
        .into_iter()
        .map(|row| (name(row), row.cells[1].clone()))
        .collect();
    assert_eq!(cells.len(), 7);
    for (name, cell) in &cells {
        let expected = if name == "Epsilon" {
            Ok(Value::Null)
        } else {
            failed.clone()
        };
        assert_eq!(*cell, expected, "{name}");
    }
    let summary = wrong.groups[0].summaries[1].as_ref().unwrap();
    assert_eq!((summary.name.as_str(), summary.built_in), ("Sum", true));
    assert_eq!(summary.value, failed);
    assert!(wrong.groups[0].summaries[0].is_none());
}

#[test]
fn a_grouped_view_gives_a_table_per_group_in_the_order_of_the_view() {
    let (vault, root) = vault("groups");
    write(
        &root,
        "Groups.base",
        "summaries:\n  Count: values.length\n  Sum: '\"mine\"'\nviews:\n  - type: table\n    name: Down\n    order: [file.name, note.status]\n    groupBy:\n      property: note.status\n      direction: DESC\n  - type: table\n    name: Up\n    order: [file.name]\n    groupBy:\n      property: status\n    summaries:\n      file.name: Count\n      note.status: Unique\n      file.size: Sum\n  - type: table\n    name: Listed\n    order: [file.name, note.status, file.size]\n    groupBy:\n      property: note.status\n    groupOrder: [ready, null, waiting, draft]\n    limit: 6\n    summaries:\n      file.name: Filled\n      file.size: Sum\n  - type: table\n    name: Hidden\n    groupBy:\n      property: note.status\n    groupOrder: []\n",
    );
    let path = Path::new("Groups.base");
    let groups = |table: &focus_bases::Table| -> Vec<(Option<Value>, Vec<String>)> {
        table
            .groups
            .iter()
            .map(|group| (group.key.clone(), group.rows.iter().map(name).collect()))
            .collect()
    };
    let key = |s: &str| Some(Value::Text(s.into()));
    let list = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();

    let down = table(&vault, path, Some("Down")).unwrap();
    assert_eq!(
        groups(&down),
        [
            (key("reference"), list(&["Zeta"])),
            (key("ready"), list(&["Beta"])),
            (key("historical"), list(&["Gamma"])),
            (key("draft"), list(&["Alpha", "Delta", "Eta"])),
            (Some(Value::Null), list(&["Epsilon"])),
        ]
    );
    assert!(down.groups.iter().all(|group| group.summaries.is_empty()));
    assert_eq!(down.groups[3].rows[1].cells[1], text("draft"));
    assert_eq!(down.matched, 7);

    let up = table(&vault, path, Some("Up")).unwrap();
    assert_eq!(
        groups(&up),
        [
            (Some(Value::Null), list(&["Epsilon"])),
            (key("draft"), list(&["Alpha", "Delta", "Eta"])),
            (key("historical"), list(&["Gamma"])),
            (key("ready"), list(&["Beta"])),
            (key("reference"), list(&["Zeta"])),
        ]
    );
    let counts: Vec<_> = up
        .groups
        .iter()
        .map(|group| {
            assert_eq!(group.summaries.len(), 1);
            let summary = group.summaries[0].as_ref().unwrap();
            assert_eq!((summary.name.as_str(), summary.built_in), ("Count", false));
            summary.value.clone()
        })
        .collect();
    assert_eq!(
        counts,
        [1.0, 3.0, 1.0, 1.0, 1.0].map(|n| Ok(Value::Number(n)))
    );

    let listed = table(&vault, path, Some("Listed")).unwrap();
    assert_eq!(
        groups(&listed)
            .into_iter()
            .map(|(key, rows)| (key, rows.len()))
            .collect::<Vec<_>>(),
        [
            (key("ready"), 1),
            (Some(Value::Null), 1),
            (key("waiting"), 0),
            (key("draft"), 3),
        ]
    );
    assert_eq!(listed.matched, 7);
    let summaries: Vec<_> = listed
        .groups
        .iter()
        .map(|group| {
            assert!(group.summaries[1].is_none());
            let filled = group.summaries[0].as_ref().unwrap();
            let sum = group.summaries[2].as_ref().unwrap();
            assert_eq!((sum.name.as_str(), sum.built_in), ("Sum", false));
            assert_eq!(sum.value, text("mine"));
            assert!(filled.built_in);
            filled.value.clone()
        })
        .collect();
    assert_eq!(
        summaries,
        [1.0, 1.0, 0.0, 3.0].map(|n| Ok(Value::Number(n)))
    );

    let hidden = table(&vault, path, Some("Hidden")).unwrap();
    assert!(hidden.groups.is_empty());
    assert_eq!(hidden.matched, 7);
}

#[test]
fn every_documented_summary_stands_under_its_column() {
    let root = scratch("summaries");
    write(&root, "A.md", "---\nn: 1\nd: 2026-01-01\nok: true\n---\n");
    write(&root, "B.md", "---\nn: 2\nd: 2026-01-03\nok: false\n---\n");
    write(&root, "C.md", "---\nn: 6\nd: 2026-01-10\nok: true\n---\n");
    write(&root, "D.md", "---\nother: x\n---\n");
    let numbers = ["Average", "Min", "Max", "Sum", "Range", "Median", "Stddev"];
    let others = [
        ("Earliest", "d"),
        ("Latest", "d"),
        ("Span", "d"),
        ("Checked", "ok"),
        ("Unchecked", "ok"),
        ("Empty", "n"),
        ("Filled", "n"),
        ("Unique", "ok"),
    ];
    let columns: Vec<(String, &str, &str)> = numbers
        .iter()
        .map(|name| (name.to_lowercase(), "n", *name))
        .chain(others.iter().map(|(name, property)| {
            (
                name.to_lowercase(),
                *property,
                if *name == "Span" { "Range" } else { name },
            )
        }))
        .collect();
    let mut base = String::from("formulas:\n");
    for (formula, property, _) in &columns {
        base += &format!("  {formula}: {property}\n");
    }
    base += "views:\n  - type: table\n    order:\n";
    for (formula, _, _) in &columns {
        base += &format!("      - formula.{formula}\n");
    }
    base += "    summaries:\n";
    for (formula, _, summary) in &columns {
        base += &format!("      formula.{formula}: {summary}\n");
    }
    write(&root, "Sums.base", &base);
    let options = OpenOptions {
        cache_dir: scratch("summaries-cache"),
        ignore: Vec::new(),
        watch: false,
    };
    let vault = Vault::open(&root, options).unwrap();
    let sums = table(&vault, Path::new("Sums.base"), None).unwrap();
    assert_eq!(sums.groups.len(), 1);
    assert_eq!(sums.groups[0].rows.len(), 4);
    let found: Vec<_> = sums.groups[0]
        .summaries
        .iter()
        .map(|summary| {
            let summary = summary.as_ref().unwrap();
            assert!(summary.built_in, "{}", summary.name);
            (summary.name.as_str(), summary.value.clone())
        })
        .collect();
    let number = |n: f64| Ok(Value::Number(n));
    assert_eq!(
        found,
        [
            ("Average", number(3.0)),
            ("Min", number(1.0)),
            ("Max", number(6.0)),
            ("Sum", number(9.0)),
            ("Range", number(5.0)),
            ("Median", number(2.0)),
            ("Stddev", number((14.0f64 / 3.0).sqrt())),
            ("Earliest", text("2026-01-01")),
            ("Latest", text("2026-01-10")),
            ("Range", text("9d")),
            ("Checked", number(2.0)),
            ("Unchecked", number(1.0)),
            ("Empty", number(1.0)),
            ("Filled", number(3.0)),
            ("Unique", number(2.0)),
        ]
    );
}

#[test]
fn the_table_takes_formulas_and_the_functions_of_the_language() {
    let (vault, root) = vault("formulas");
    write(
        &root,
        "Work.base",
        "filters: 'file.ext == \"md\"'\nformulas:\n  label: 'file.basename.lower() + if(status, \" (\" + status.title() + \")\", \"\")'\n  verified: 'if(last_verified, last_verified + \"1M\")'\n  host: 'if(hostname, hostname.replace(/\\d+$/, \"\"))'\n  linked: file.links.map(value.asFile().basename)\n  from: file.backlinks.map(value.asFile().basename)\n  in_red: file.inFolder(\"Red\")\nviews:\n  - type: table\n    name: All\n    filters: 'formula.label.contains(\"(\") && !status.containsAny(\"historical\", \"reference\")'\n    order: [file.name, formula.label, formula.verified, formula.host, formula.linked, formula.from, formula.in_red, last_verified, file.tags, file.folder]\n    sort:\n      - property: formula.in_red\n        direction: DESC\n      - property: formula.label\n",
    );
    let all = table(&vault, Path::new("Work.base"), None).unwrap();
    assert_eq!(names(&all), ["Alpha", "Beta", "Delta", "Eta"]);
    let cells = |row: usize| -> Vec<Value> {
        rows(&all)[row].cells[1..]
            .iter()
            .map(|cell| cell.clone().unwrap())
            .collect()
    };
    let text = |s: &str| Value::Text(s.into());
    assert_eq!(
        cells(0),
        [
            text("alpha (Draft)"),
            Value::Null,
            Value::Null,
            Value::List(vec![text("Beta")]),
            Value::List(vec![]),
            Value::Bool(true),
            Value::Null,
            Value::List(vec![text("#attack/ad")]),
            text("Red"),
        ]
    );
    assert_eq!(
        cells(1),
        [
            text("beta (Ready)"),
            text("2026-02-02"),
            Value::Null,
            Value::List(vec![]),
            Value::List(vec![text("Alpha")]),
            Value::Bool(true),
            text("2026-01-02"),
            Value::List(vec![]),
            text("Red"),
        ]
    );
    assert_eq!(cells(2)[2], text("dc"));
    assert_eq!(cells(2)[5], Value::Bool(false));
    assert_eq!(all.columns[1].id, "formula.label");
    assert_eq!(all.columns[1].label, "label");
    assert!(!all.columns[1].link);
}

#[test]
fn a_view_is_evaluated_over_every_file_with_typed_cells() {
    let (vault, root) = vault("evaluate");
    write(&root, "Red/shot.png", "not a picture");
    write(
        &root,
        "Red/Linked.md",
        "---\nproject: \"[[Alpha]]\"\nsite: https://obsidian.md\ntags: [attack/web]\nStatus: draft\nwhen: 2026-03-04T10:30\n---\n![[shot.png]] and [[Nowhere]]\n",
    );
    write(
        &root,
        "Red/Board.base",
        "filters: 'file.inFolder(\"Red\")'\nformulas:\n  lead: project.asFile().properties.domain\n  here: file.hasLink(this.file)\nviews:\n  - type: kanban\n    name: Board\n    order: [file.name, note.status, formula.lead, note.project, note.site, note.tags, file.embeds, file.links, file.backlinks, file.size, note.when, formula.here]\n    groupBy:\n      property: note.status\n      direction: ASC\n    summaries:\n      file.name: Filled\n",
    );
    let vault = {
        drop(vault);
        let options = OpenOptions {
            cache_dir: scratch("evaluate-cache-2"),
            ignore: Vec::new(),
            watch: false,
        };
        Vault::open(&root, options).unwrap()
    };
    let board = evaluate(
        &vault,
        Path::new("Red/Board.base"),
        Some("Board"),
        Some(Path::new("Red/Beta.md")),
    )
    .unwrap();
    let paths: Vec<_> = board.rows.iter().map(|row| row.path.as_ref()).collect();
    assert_eq!(
        paths,
        [
            "Red/Alpha.md",
            "Red/Beta.md",
            "Red/Board.base",
            "Red/Gamma.md",
            "Red/Linked.md",
            "Red/shot.png"
        ]
    );
    assert_eq!(board.kind, "kanban");
    let linked = &board.rows[4];
    assert_eq!(linked.note, vault.note_by_path(Path::new("Red/Linked.md")));
    assert_eq!(linked.cells[1], Ok(Typed::Text("draft".into())));
    assert_eq!(linked.cells[2], Ok(Typed::Text("ad".into())));
    match &linked.cells[3] {
        Ok(Typed::Link(link)) => {
            assert_eq!(link.target, "Alpha");
            assert_eq!(link.file.as_deref(), Some("Red/Alpha.md"));
        }
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(&linked.cells[4], Ok(Typed::Url(url)) if url.address == "https://obsidian.md")
    );
    assert_eq!(
        linked.cells[5],
        Ok(Typed::List(vec![Typed::Tag("attack/web".into())]))
    );
    let targets = |cell: &Cell| match cell {
        Ok(Typed::List(items)) => items
            .iter()
            .map(|item| match item {
                Typed::Link(link) => (
                    link.target.clone(),
                    link.file.as_deref().map(str::to_string),
                ),
                other => panic!("{other:?}"),
            })
            .collect::<Vec<_>>(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        targets(&linked.cells[6]),
        [("shot.png".to_string(), Some("Red/shot.png".to_string()))]
    );
    let mut links = targets(&linked.cells[7]);
    links.sort();
    assert_eq!(
        links,
        [
            ("Alpha".to_string(), Some("Red/Alpha.md".to_string())),
            ("Nowhere".to_string(), None),
            ("shot.png".to_string(), Some("Red/shot.png".to_string())),
        ]
    );
    assert!(matches!(&linked.cells[10], Ok(Typed::Date(date)) if date.time));
    let shot = &board.rows[5];
    assert_eq!(shot.note, None);
    assert_eq!(shot.cells[1], Ok(Typed::Null));
    assert_eq!(shot.cells[9], Ok(Typed::Number(13.0)));
    assert_eq!(
        targets(&shot.cells[8]),
        [("Red/Linked".to_string(), Some("Red/Linked.md".to_string()))]
    );
    let here: Vec<_> = board.rows.iter().map(|row| row.cells[11].clone()).collect();
    assert_eq!(here[0], Ok(Typed::Bool(true)));
    assert!(here[1..].iter().all(|cell| *cell == Ok(Typed::Bool(false))));
    let groups: Vec<_> = board
        .groups
        .iter()
        .map(|group| {
            (
                group.key.clone(),
                group.rows.clone(),
                group.summaries.clone(),
            )
        })
        .collect();
    assert_eq!(
        groups,
        [
            (Typed::Null, vec![2, 5], vec![Ok(Typed::Number(2.0))]),
            (
                Typed::Text("draft".into()),
                vec![0, 4],
                vec![Ok(Typed::Number(2.0))]
            ),
            (
                Typed::Text("historical".into()),
                vec![3],
                vec![Ok(Typed::Number(1.0))]
            ),
            (
                Typed::Text("ready".into()),
                vec![1],
                vec![Ok(Typed::Number(1.0))]
            ),
        ]
    );
    write(
        &root,
        "Red/Plain.base",
        "filters: 'file.inFolder(\"Red\")'\nviews:\n  - type: table\n",
    );
    let plain = table(&vault, Path::new("Red/Plain.base"), None).unwrap();
    assert_eq!(names(&plain), ["Alpha", "Beta", "Gamma", "Linked"]);
    assert!(matches!(
        evaluate(&vault, Path::new("Red/Board.base"), Some("Nope"), None),
        Err(QueryError::Base(BasesError::UnknownView(_)))
    ));
}

#[test]
fn only_a_base_file_inside_the_vault_is_read() {
    let (vault, root) = vault("paths");
    write(&root, "Red/note.txt", "x");
    for bad in [
        "../Operations.base",
        "/etc/passwd",
        "Red/Alpha.md",
        "Red/note.txt",
        "Red/../../x.base",
    ] {
        assert!(
            matches!(load(&vault, Path::new(bad)), Err(BasesError::NotABase(_))),
            "{bad}"
        );
    }
    assert!(matches!(
        load(&vault, Path::new("Missing.base")),
        Err(BasesError::Io { .. })
    ));
    #[cfg(unix)]
    {
        let outside = scratch("paths-outside");
        write(&outside, "Out.base", "views:\n  - type: table\n");
        std::os::unix::fs::symlink(outside.join("Out.base"), root.join("Link.base")).unwrap();
        assert!(matches!(
            load(&vault, Path::new("Link.base")),
            Err(BasesError::NotABase(_))
        ));
    }
}

#[test]
fn every_view_of_a_page_has_its_own_result() {
    let (vault, _) = vault("page");
    let base = load(&vault, Path::new("Operations.base")).unwrap();
    assert_eq!(base.views.len(), 3);
    let counts: Vec<_> = (0..base.views.len())
        .map(|i| rows(&base.table_at(&vault, i, None).unwrap()).len())
        .collect();
    assert_eq!(counts, [2, 1, 1]);
    assert!(matches!(
        base.table_at(&vault, 3, None),
        Err(BasesError::NoView)
    ));
    assert!(matches!(
        Base::parse("views: [").unwrap_err(),
        BasesError::Yaml(_)
    ));
}

#[test]
fn this_is_the_base_on_its_page_and_the_embedding_note_in_an_embed() {
    let root = scratch("this");
    write(
        &root,
        "Red/Alpha.md",
        "---\nstatus: draft\n---\n[[Work.base]]\n",
    );
    write(&root, "Red/Beta.md", "---\nstatus: ready\n---\nBeta\n");
    write(
        &root,
        "Red/Host.md",
        "---\nstatus: ready\n---\n![[Work.base]]\n",
    );
    write(
        &root,
        "Work.base",
        "filters: 'file.inFolder(\"Red\")'\nformulas:\n  same: note.status == this.status\n  host: this.file.name\nviews:\n  - type: table\n    name: Linking\n    filters: 'file.hasLink(this.file)'\n    order: [file.name, formula.host]\n  - type: table\n    name: Same\n    filters: formula.same\n    order: [file.name, formula.host]\n",
    );
    let options = OpenOptions {
        cache_dir: scratch("this-cache"),
        ignore: Vec::new(),
        watch: false,
    };
    let vault = Vault::open(&root, options).unwrap();
    let base = load(&vault, Path::new("Work.base")).unwrap();
    let drawn = |view: &str, this: &str| {
        let table = base
            .table(&vault, Some(view), Some(Path::new(this)))
            .unwrap();
        rows(&table)
            .into_iter()
            .map(|row| (name(row), row.cells[1].clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        drawn("Linking", "Work.base"),
        [
            ("Alpha".to_string(), text("Work.base")),
            ("Host".to_string(), text("Work.base")),
        ]
    );
    assert!(drawn("Same", "Work.base").is_empty());
    assert_eq!(
        drawn("Same", "Red/Host.md"),
        [
            ("Beta".to_string(), text("Host.md")),
            ("Host".to_string(), text("Host.md")),
        ]
    );
    assert!(drawn("Linking", "Red/Host.md").is_empty());
    for (view, expression) in [
        ("Linking", "file.hasLink(this.file)"),
        ("Same", "note.status == this.status"),
    ] {
        match base.table(&vault, Some(view), None) {
            Err(BasesError::Unsupported {
                expression: found,
                feature: Feature::This,
                ..
            }) => assert_eq!(found, expression, "{view}"),
            other => panic!("{view}: {other:?}"),
        }
    }
}

#[test]
fn an_embed_target_resolves_to_the_path_the_table_function_takes() {
    let (vault, _) = vault("embed");
    let alpha = vault.note_by_path(Path::new("Red/Alpha.md")).unwrap();
    match vault.resolve_link(alpha, "Operations.base") {
        focus_core::LinkTarget::Attachment(path) => assert_eq!(path, Path::new("Operations.base")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_column_a_direction_and_the_log_text_are_typed() {
    let (vault, root) = vault("typed");
    write(
        &root,
        "Inode.base",
        "views:\n  - type: table\n    name: L\n    order: [file.inode]\n",
    );
    let error = table(&vault, Path::new("Inode.base"), None).unwrap_err();
    assert!(
        matches!(&error, BasesError::Unsupported { expression, feature: Feature::Property, subject } if expression == "file.inode" && subject == "file.inode")
    );
    assert_eq!(
        error.to_string(),
        "not supported: file.inode (property file.inode)"
    );
    write(
        &root,
        "Sideways.base",
        "views:\n  - type: table\n    sort:\n      - property: file.name\n        direction: SIDEWAYS\n",
    );
    let error = load(&vault, Path::new("Sideways.base")).unwrap_err();
    assert!(
        matches!(error, BasesError::Unsupported { feature: Feature::SortDirection, subject, .. } if subject == "SIDEWAYS")
    );
}
