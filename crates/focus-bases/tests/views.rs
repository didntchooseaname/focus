mod support;

use focus_bases::{
    Base, BasesError, Cell, EvalError, Feature, Part, QueryError, Typed, ViewResult,
};
use support::*;

fn book(path: &'static str, props: Vec<(&'static str, Typed)>) -> File {
    File {
        path,
        props,
        size: 100,
        ..File::default()
    }
}

fn library() -> Memory {
    Memory {
        files: vec![
            book(
                "Books/Dune.md",
                vec![
                    ("price", number(10.0)),
                    ("age", number(4.0)),
                    ("status", text("done")),
                    ("read", Typed::Bool(true)),
                    ("bought", Typed::Date(date("2024-03-01"))),
                    ("genres", texts(&["scifi", "classic"])),
                ],
            ),
            book(
                "Books/Emma.md",
                vec![
                    ("price", number(4.0)),
                    ("age", number(2.0)),
                    ("status", text("reading")),
                    ("read", Typed::Bool(false)),
                    ("bought", Typed::Date(date("2025-01-15"))),
                    ("genres", texts(&["classic"])),
                ],
            ),
            book(
                "Books/Ubik.md",
                vec![
                    ("price", number(7.0)),
                    ("age", number(7.0)),
                    ("status", text("done")),
                    ("read", Typed::Bool(true)),
                    ("bought", Typed::Date(date("2024-03-01"))),
                    ("genres", texts(&["scifi", "classic"])),
                ],
            ),
            book(
                "Books/Solaris.md",
                vec![
                    ("price", number(4.0)),
                    ("age", number(1.0)),
                    ("status", text("reading")),
                    ("read", Typed::Bool(false)),
                    ("genres", texts(&["scifi"])),
                ],
            ),
            book("Books/Untitled.md", vec![("status", text(""))]),
            File {
                path: "Books/cover.png",
                size: 5,
                ..File::default()
            },
        ],
    }
}

fn run(source: &str) -> Result<ViewResult, QueryError> {
    Base::parse(source)
        .unwrap()
        .evaluate_in(&library(), 0, Some("Books/Dune.md"), now())
}

fn view(source: &str) -> ViewResult {
    run(source).unwrap_or_else(|error| panic!("{error}"))
}

fn names(result: &ViewResult) -> Vec<&str> {
    result
        .rows
        .iter()
        .map(|row| row.path.rsplit('/').next().unwrap())
        .collect()
}

fn column(result: &ViewResult, at: usize) -> Vec<Cell> {
    result
        .rows
        .iter()
        .map(|row| row.cells[at].clone())
        .collect()
}

const NOTES: &str = "filters: 'file.ext == \"md\"'\n";

#[test]
fn a_base_lists_every_file_and_a_filter_narrows_it() {
    let all = view("views:\n  - type: table\n");
    assert_eq!(
        names(&all),
        [
            "cover.png",
            "Dune.md",
            "Emma.md",
            "Solaris.md",
            "Ubik.md",
            "Untitled.md"
        ]
    );
    assert_eq!(all.kind, "table");
    assert_eq!(all.rows[0].note, None);
    assert!(all.rows[1].note.is_some());
    assert_eq!(all.columns[0].id, "file.name");
    assert!(all.columns[0].link);
    assert_eq!(all.groups.len(), 1);
    assert_eq!(all.groups[0].rows, [0, 1, 2, 3, 4, 5]);
    let cheap = view(
        "filters:\n  and:\n    - 'file.ext == \"md\"'\n    - 'price < 8'\nviews:\n  - type: cards\n    name: Cheap\n    filters:\n      not:\n        - 'status == \"done\"'\n    limit: 1\n",
    );
    assert_eq!(names(&cheap), ["Emma.md"]);
    assert_eq!(cheap.matched, 2);
    assert_eq!(cheap.kind, "cards");
    assert_eq!(cheap.view.as_deref(), Some("Cheap"));
}

#[test]
fn formulas_are_columns_filters_and_sort_keys() {
    let source = format!(
        "{NOTES}formulas:\n  formatted_price: 'if(price, price.toFixed(2) + \" dollars\")'\n  ppu: \"(price / age).toFixed(2)\"\n  double: formula.ppu * 2\n  label: 'formula.formatted_price + \" for \" + file.basename'\nproperties:\n  formula.formatted_price:\n    displayName: Price\nviews:\n  - type: table\n    filters:\n      or:\n        - 'formula.ppu > 2'\n        - 'price > 9'\n    order: [file.name, formula.ppu, formula.formatted_price, formula.label, note.age]\n    sort:\n      - property: formula.ppu\n        direction: DESC\n"
    );
    let result = view(&source);
    assert_eq!(names(&result), ["Solaris.md", "Dune.md"]);
    assert_eq!(column(&result, 1), [Ok(text("4.00")), Ok(text("2.50"))]);
    assert_eq!(
        column(&result, 2),
        [Ok(text("4.00 dollars")), Ok(text("10.00 dollars"))]
    );
    assert_eq!(
        column(&result, 3),
        [
            Ok(text("4.00 dollars for Solaris")),
            Ok(text("10.00 dollars for Dune"))
        ]
    );
    let ids: Vec<_> = result
        .columns
        .iter()
        .map(|c| (c.id.as_str(), c.label.as_str(), c.named))
        .collect();
    assert_eq!(
        ids,
        [
            ("file.name", "name", false),
            ("formula.ppu", "ppu", false),
            ("formula.formatted_price", "Price", true),
            ("formula.label", "label", false),
            ("note.age", "age", false),
        ]
    );
}

#[test]
fn a_missing_property_gives_an_empty_formula_cell_and_a_wrong_type_an_error() {
    let source = format!(
        "{NOTES}formulas:\n  ppu: \"(price / age).toFixed(2)\"\n  shout: status * 2\n  twice: formula.shout\nviews:\n  - type: table\n    order: [file.name, formula.ppu, formula.shout, formula.twice]\n    sort:\n      - property: file.name\n"
    );
    let result = view(&source);
    assert_eq!(result.rows[4].path.as_ref(), "Books/Untitled.md");
    assert_eq!(result.rows[4].cells[1], Ok(Typed::Null));
    let wrong = Err(EvalError::Operator {
        operator: "*",
        left: "string",
        right: "number",
    });
    for row in &result.rows {
        assert_eq!(row.cells[2], wrong);
        assert_eq!(row.cells[3], wrong);
    }
}

#[test]
fn a_cycle_of_formulas_is_an_error_that_names_them() {
    let cycle = |formulas: &str, order: &str| {
        run(&format!(
            "formulas:\n{formulas}views:\n  - type: table\n    order: [{order}]\n"
        ))
    };
    match cycle(
        "  a: formula.b + 1\n  b: formula.c + 1\n  c: formula.a + 1\n",
        "formula.a",
    ) {
        Err(QueryError::FormulaCycle(names)) => assert_eq!(names, ["a", "b", "c", "a"]),
        other => panic!("{other:?}"),
    }
    match cycle("  self: formula.self + 1\n", "formula.self") {
        Err(QueryError::FormulaCycle(names)) => assert_eq!(names, ["self", "self"]),
        other => panic!("{other:?}"),
    }
    match cycle(
        "  a: formula.b\n  b: '[1].map(formula[\"b\"])'\n",
        "formula.a",
    ) {
        Err(error @ QueryError::FormulaCycle(_)) => {
            assert_eq!(error.to_string(), "formulas refer to each other: b, b")
        }
        other => panic!("{other:?}"),
    }
    assert!(cycle("  a: formula.a\n  fine: price * 2\n", "formula.fine").is_ok());
    match cycle("  a: formula.nope + 1\n", "formula.a") {
        Err(QueryError::UnknownFormula { expression, name }) => {
            assert_eq!(
                (expression.as_str(), name.as_str()),
                ("formula.nope + 1", "nope")
            )
        }
        other => panic!("{other:?}"),
    }
    match cycle("  a: price.frob()\n", "formula.a") {
        Err(QueryError::Base(BasesError::Unsupported {
            expression,
            feature: Feature::Method,
            subject,
        })) => assert_eq!(
            (expression.as_str(), subject.as_str()),
            ("price.frob()", "frob")
        ),
        other => panic!("{other:?}"),
    }
    let mut chain = String::new();
    for at in 0..70 {
        chain.push_str(&format!("  f{at}: formula.f{} + 1\n", at + 1));
    }
    chain.push_str("  f70: 1\n");
    assert!(matches!(
        cycle(&chain, "formula.f0"),
        Err(QueryError::Base(BasesError::Invalid(_)))
    ));
}

#[test]
fn a_failure_that_decides_for_a_row_stops_the_view() {
    let failing = |view: &str| run(&format!("{NOTES}views:\n  - type: table\n{view}"));
    match failing("    filters: 'status * 2 > 1'\n") {
        Err(QueryError::Row {
            part: Part::Filter,
            expression,
            path,
            error,
        }) => {
            assert_eq!(expression, "status * 2 > 1");
            assert!(path.starts_with("Books/"), "{path}");
            assert_eq!(
                error,
                EvalError::Operator {
                    operator: "*",
                    left: "string",
                    right: "number"
                }
            );
        }
        other => panic!("{other:?}"),
    }
    let source = format!(
        "{NOTES}formulas:\n  bad: status.toFixed(1)\nviews:\n  - type: table\n    sort:\n      - property: formula.bad\n  - type: table\n    groupBy:\n      property: formula.bad\n"
    );
    let base = Base::parse(&source).unwrap();
    assert!(matches!(
        base.evaluate_in(&library(), 0, None, now()),
        Err(QueryError::Row { part: Part::Sort, expression, .. }) if expression == "formula.bad"
    ));
    let grouped = base.evaluate_in(&library(), 1, None, now());
    assert!(matches!(
        &grouped,
        Err(QueryError::Row { part: Part::Group, expression, .. }) if expression == "formula.bad"
    ));
    assert!(
        grouped
            .unwrap_err()
            .to_string()
            .starts_with("group formula.bad fails on Books/"),
    );
    let guarded = view(&format!(
        "{NOTES}views:\n  - type: table\n    filters:\n      and:\n        - 'price > 100'\n        - 'status * 2 > 1'\n"
    ));
    assert!(guarded.rows.is_empty());
    assert!(matches!(
        base.evaluate_in(&library(), 5, None, now()),
        Err(QueryError::Base(BasesError::NoView))
    ));
}

#[test]
fn rows_sort_by_several_keys_with_missing_values_first() {
    let sorted = |sort: &str| {
        let result = view(&format!(
            "{NOTES}views:\n  - type: table\n    sort:\n{sort}"
        ));
        names(&result)
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sorted("      - property: price\n        direction: ASC\n"),
        ["Untitled.md", "Emma.md", "Solaris.md", "Ubik.md", "Dune.md"]
    );
    assert_eq!(
        sorted(
            "      - property: price\n        direction: DESC\n      - property: file.name\n        direction: DESC\n"
        ),
        ["Dune.md", "Ubik.md", "Solaris.md", "Emma.md", "Untitled.md"]
    );
    assert_eq!(
        sorted("      - property: note.bought\n      - property: age\n        direction: DESC\n"),
        ["Solaris.md", "Untitled.md", "Ubik.md", "Dune.md", "Emma.md"]
    );
    assert_eq!(
        sorted("      - property: read\n"),
        ["Untitled.md", "Emma.md", "Solaris.md", "Dune.md", "Ubik.md"]
    );
}

#[test]
fn rows_are_grouped_by_the_value_of_a_property() {
    let grouped = |view_keys: &str| {
        view(&format!(
            "{NOTES}views:\n  - type: kanban\n    name: Board\n    sort:\n      - property: price\n        direction: DESC\n{view_keys}"
        ))
    };
    let groups = |result: &ViewResult| -> Vec<(String, Vec<String>)> {
        result
            .groups
            .iter()
            .map(|group| {
                (
                    group.key.text(now()),
                    group
                        .rows
                        .iter()
                        .map(|at| {
                            result.rows[*at]
                                .path
                                .rsplit('/')
                                .next()
                                .unwrap()
                                .to_string()
                        })
                        .collect(),
                )
            })
            .collect()
    };
    let pair = |key: &str, rows: &[&str]| {
        (
            key.to_string(),
            rows.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
        )
    };

    let by_status = grouped("    groupBy:\n      property: note.status\n      direction: ASC\n");
    assert_eq!(by_status.group_by.as_deref(), Some("note.status"));
    assert_eq!(by_status.kind, "kanban");
    assert_eq!(by_status.groups[0].key, Typed::Null);
    assert_eq!(
        groups(&by_status),
        [
            pair("", &["Untitled.md"]),
            pair("done", &["Dune.md", "Ubik.md"]),
            pair("reading", &["Emma.md", "Solaris.md"]),
        ]
    );
    let descending = grouped("    groupBy:\n      property: status\n      direction: DESC\n");
    assert_eq!(
        groups(&descending),
        [
            pair("reading", &["Emma.md", "Solaris.md"]),
            pair("done", &["Dune.md", "Ubik.md"]),
            pair("", &["Untitled.md"]),
        ]
    );
    let by_list = grouped("    groupBy:\n      property: genres\n");
    assert_eq!(
        groups(&by_list),
        [
            pair("", &["Untitled.md"]),
            pair("classic", &["Emma.md"]),
            pair("scifi", &["Solaris.md"]),
            pair("scifi, classic", &["Dune.md", "Ubik.md"]),
        ]
    );
    let by_number = grouped("    groupBy:\n      property: price\n    limit: 3\n");
    assert_eq!(by_number.matched, 5);
    assert_eq!(
        groups(&by_number),
        [
            pair("4", &["Emma.md"]),
            pair("7", &["Ubik.md"]),
            pair("10", &["Dune.md"])
        ]
    );
    let by_date = grouped("    groupBy:\n      property: bought\n      direction: DESC\n");
    assert_eq!(
        groups(&by_date),
        [
            pair("2025-01-15", &["Emma.md"]),
            pair("2024-03-01", &["Dune.md", "Ubik.md"]),
            pair("", &["Solaris.md", "Untitled.md"]),
        ]
    );
    let by_formula = view(&format!(
        "{NOTES}formulas:\n  cheap: 'if(price < 5, \"cheap\", \"dear\")'\nviews:\n  - type: table\n    groupBy:\n      property: formula.cheap\n"
    ));
    assert_eq!(
        groups(&by_formula),
        [
            pair("cheap", &["Emma.md", "Solaris.md"]),
            pair("dear", &["Dune.md", "Ubik.md", "Untitled.md"]),
        ]
    );
}

#[test]
fn group_order_lists_the_groups_to_show() {
    let ordered = |order: &str| {
        let result = view(&format!(
            "{NOTES}views:\n  - type: kanban\n    groupBy:\n      property: note.status\n      direction: ASC\n    groupOrder: {order}\n"
        ));
        result
            .groups
            .iter()
            .map(|group| (group.key.clone(), group.rows.len()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ordered("[reading, Planned, done]"),
        [
            (text("reading"), 2),
            (text("Planned"), 0),
            (text("done"), 2)
        ]
    );
    assert_eq!(
        ordered("[done, null, done]"),
        [(text("done"), 2), (Typed::Null, 1)]
    );
    assert!(ordered("[]").is_empty());
    let numbers_listed = view(&format!(
        "{NOTES}views:\n  - type: kanban\n    groupBy:\n      property: price\n    groupOrder: [10, 4]\n"
    ));
    let sizes: Vec<_> = numbers_listed.groups.iter().map(|g| g.rows.len()).collect();
    assert_eq!(sizes, [1, 2]);
    assert_eq!(numbers_listed.rows.len(), 5);
}

fn summary(property: &str, name: &str, extra: &str) -> Cell {
    let result = view(&format!(
        "{NOTES}{extra}views:\n  - type: table\n    summaries:\n      {property}: {name}\n"
    ));
    assert_eq!(result.summaries.len(), 1);
    assert_eq!(result.summaries[0].name, name);
    result.groups[0].summaries[0].clone()
}

#[test]
fn every_built_in_summary() {
    for (name, expected) in [
        ("Average", number(6.25)),
        ("Min", number(4.0)),
        ("Max", number(10.0)),
        ("Sum", number(25.0)),
        ("Range", number(6.0)),
        ("Median", number(5.5)),
        ("Stddev", number((24.75f64 / 4.0).sqrt())),
        ("Empty", number(1.0)),
        ("Filled", number(4.0)),
        ("Unique", number(3.0)),
        ("Earliest", Typed::Null),
        ("Checked", number(0.0)),
    ] {
        assert_eq!(summary("price", name, ""), Ok(expected), "{name}");
    }
    assert_eq!(
        summary("note.bought", "Earliest", ""),
        Ok(Typed::Date(date("2024-03-01")))
    );
    assert_eq!(
        summary("bought", "Latest", ""),
        Ok(Typed::Date(date("2025-01-15")))
    );
    assert_eq!(
        summary("bought", "Range", "").map(|range| range.text(now())),
        Ok("320d".to_string())
    );
    assert_eq!(summary("bought", "Empty", ""), Ok(number(2.0)));
    assert_eq!(summary("bought", "Unique", ""), Ok(number(2.0)));
    assert_eq!(summary("bought", "Sum", ""), Ok(Typed::Null));
    assert_eq!(summary("read", "Checked", ""), Ok(number(2.0)));
    assert_eq!(summary("read", "Unchecked", ""), Ok(number(2.0)));
    assert_eq!(summary("status", "Empty", ""), Ok(number(1.0)));
    assert_eq!(summary("status", "Filled", ""), Ok(number(4.0)));
    assert_eq!(summary("status", "Unique", ""), Ok(number(2.0)));
    assert_eq!(summary("genres", "Unique", ""), Ok(number(3.0)));
    assert_eq!(summary("file.size", "Sum", ""), Ok(number(500.0)));
    let formulas = "formulas:\n  ppu: price / age\n  mixed: 'if(price > 5, price, bought)'\n  bad: status * 2\n";
    assert_eq!(summary("formula.ppu", "Max", formulas), Ok(number(4.0)));
    assert_eq!(
        summary("formula.mixed", "Range", formulas),
        Err(EvalError::Operator {
            operator: "Range",
            left: "number",
            right: "date"
        })
    );
    assert_eq!(
        summary("formula.bad", "Filled", formulas),
        Err(EvalError::Operator {
            operator: "*",
            left: "string",
            right: "number"
        })
    );
}

#[test]
fn custom_summaries_run_a_formula_over_the_values_of_a_column() {
    let custom = "summaries:\n  customAverage: 'values.mean().round(3)'\n  Spread: 'values.filter(value.isType(\"number\")).sort()[0] + \" to \" + values.filter(value.isType(\"number\")).sort().reverse()[0]'\n  Count: values.length\n  Sum: '\"custom\"'\n  Mine: 'values.filter(value == this.price).length'\n  Bad: values.frob()\n  Row: values.length + price\n";
    assert_eq!(summary("price", "customAverage", custom), Ok(number(6.25)));
    assert_eq!(summary("price", "Spread", custom), Ok(text("4 to 10")));
    assert_eq!(summary("price", "Count", custom), Ok(number(5.0)));
    assert_eq!(summary("price", "Sum", custom), Ok(text("custom")));
    assert_eq!(summary("price", "Mine", custom), Ok(number(1.0)));
    let failing = |name: &str| {
        run(&format!(
            "{custom}views:\n  - type: table\n    summaries:\n      price: {name}\n"
        ))
    };
    assert!(matches!(
        failing("Bad"),
        Err(QueryError::Base(BasesError::Unsupported { feature: Feature::Method, subject, .. })) if subject == "frob"
    ));
    assert!(matches!(
        failing("Row"),
        Err(QueryError::Base(BasesError::Unsupported { feature: Feature::Property, subject, .. })) if subject == "price"
    ));
    match failing("Nope") {
        Err(QueryError::UnknownSummary { property, name }) => {
            assert_eq!((property.as_str(), name.as_str()), ("note.price", "Nope"))
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn each_group_has_its_summaries() {
    let result = view(&format!(
        "{NOTES}views:\n  - type: table\n    groupBy:\n      property: status\n    summaries:\n      price: Sum\n      file.name: Filled\n      note.bought: Latest\n"
    ));
    let headers: Vec<_> = result
        .summaries
        .iter()
        .map(|s| (s.property.as_str(), s.name.as_str()))
        .collect();
    assert_eq!(
        headers,
        [
            ("note.price", "Sum"),
            ("file.name", "Filled"),
            ("note.bought", "Latest")
        ]
    );
    let per_group: Vec<_> = result
        .groups
        .iter()
        .map(|group| (group.key.clone(), group.summaries.clone()))
        .collect();
    assert_eq!(
        per_group,
        [
            (
                Typed::Null,
                vec![Ok(Typed::Null), Ok(number(1.0)), Ok(Typed::Null)]
            ),
            (
                text("done"),
                vec![
                    Ok(number(17.0)),
                    Ok(number(2.0)),
                    Ok(Typed::Date(date("2024-03-01")))
                ]
            ),
            (
                text("reading"),
                vec![
                    Ok(number(8.0)),
                    Ok(number(2.0)),
                    Ok(Typed::Date(date("2025-01-15")))
                ]
            ),
        ]
    );
}

#[test]
fn the_sections_of_a_base_are_read_as_written() {
    let base = Base::parse(
        "formulas:\n  ppu: price / age\n  count: 5\nsummaries:\n  customAverage: 'values.mean().round(3)'\nviews:\n  - type: table\n    groupBy:\n      property: note.age\n      direction: DESC\n    groupOrder: [3, null, done]\n    summaries:\n      formula.ppu: Average\n",
    )
    .unwrap();
    assert_eq!(
        base.formulas,
        [
            ("ppu".to_string(), "price / age".to_string()),
            ("count".to_string(), "5".to_string())
        ]
    );
    assert_eq!(
        base.summaries,
        [(
            "customAverage".to_string(),
            "values.mean().round(3)".to_string()
        )]
    );
    let view = &base.views[0];
    let group = view.group_by.as_ref().unwrap();
    assert_eq!(
        (group.property.as_str(), group.direction),
        ("note.age", focus_bases::Direction::Desc)
    );
    assert_eq!(
        view.group_order,
        Some(vec![number(3.0), Typed::Null, text("done")])
    );
    assert_eq!(
        view.summaries,
        [("formula.ppu".to_string(), "Average".to_string())]
    );
    for bad in [
        "formulas: [a]\n",
        "formulas:\n  a: [1]\n",
        "summaries: text\n",
        "views:\n  - type: table\n    groupBy: status\n",
        "views:\n  - type: table\n    groupBy:\n      direction: ASC\n",
        "views:\n  - type: table\n    groupOrder: done\n",
        "views:\n  - type: table\n    groupOrder: [{a: 1}]\n",
        "views:\n  - type: table\n    summaries: [price]\n",
    ] {
        assert!(
            matches!(Base::parse(bad), Err(BasesError::Invalid(_))),
            "{bad}"
        );
    }
}

#[test]
fn formulas_that_nest_past_the_stack_budget_are_an_error_in_the_cell() {
    let mut source = String::from("formulas:\n");
    for at in 0..60 {
        source.push_str(&format!(
            "  f{at}: 'formula.f{}{}'\n",
            at + 1,
            " + 1".repeat(90)
        ));
    }
    source.push_str("  f60: price\nviews:\n  - type: table\n    filters: 'file.name == \"Dune.md\"'\n    order: [formula.f0, formula.f59]\n");
    let cells = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || view(&source).rows[0].cells.clone())
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(cells, [Err(EvalError::TooDeep), Ok(number(100.0))]);
}
