mod support;

use focus_bases::{EvalError, ExprError, Feature, Typed, compile, compile_summary};
use support::*;

#[test]
fn a_bare_name_is_a_note_property() {
    assert!(holds(r#"status == "draft""#));
    assert!(holds(r#"note.status == "draft""#));
    assert!(holds(r#"note["status"] == "draft""#));
    assert!(holds(r#"note["a b"] == 1"#));
    assert!(holds(r#"note["sta" + "tus"] == "draft""#));
    assert!(!holds(r#"status == "ready""#));
    assert_eq!(value("note.status.length"), number(5.0));
}

#[test]
fn arithmetic_operators_and_their_precedence() {
    for (source, expected) in [
        ("1 + 2 * 3", 7.0),
        ("(1 + 2) * 3", 9.0),
        ("10 / 5 / 2", 1.0),
        ("10 - 4 - 3", 3.0),
        ("7 % 4", 3.0),
        ("-7 % 4", -3.0),
        ("2 * 3 % 4", 2.0),
        ("-2 * 3", -6.0),
        ("2 - -3", 5.0),
        ("-(2 + 3)", -5.0),
        ("1.5e2 + 1", 151.0),
        ("price * (2 * 3.14)", 25.12),
        ("(price / age) * 100", 200.0),
        ("price + age * 2", 8.0),
    ] {
        assert_eq!(value(source), number(expected), "{source}");
    }
    assert_eq!(value("1 / 0"), number(f64::INFINITY));
    assert_eq!(shown("1 / 0"), "Infinity");
    assert_eq!(shown("(price / age).toFixed(2)"), "2.00");
}

#[test]
fn comparison_and_boolean_operators_and_their_precedence() {
    for source in [
        "score > 7",
        "score <= 7.5",
        "score >= 7.5 && score != 7",
        "1 + 2 > 2",
        "1 + 2 == 3",
        "2 * 3 >= 6 && 1 < 2",
        "true || false && false",
        "!(true && false)",
        "!false && true",
        "!missing",
        "1 < 2 == true",
        "2 > 1 != false",
        r#"file.size >= 2048"#,
        r#"file.ext == "md" && !(status == "ready")"#,
        r#"status == "x" || file.ext == 'md'"#,
        r#""b" > "a""#,
        r#""B" > "a""#,
        "true > false",
        "done == true",
        "[1, 2] == [1, 2]",
        "[1, 2] != [2, 1]",
        r#"{"a": 1} == {"a": 1}"#,
        "null == null",
        "missing == null",
        r#"status != null"#,
    ] {
        assert!(holds(source), "{source}");
    }
    for source in [
        "(true || false) && false",
        "score > 8",
        "missing > 1",
        "missing < 1",
        "missing == 0",
        r#"missing == """#,
        "[1] > [0]",
        "true == 1",
    ] {
        assert!(!holds(source), "{source}");
    }
    assert_eq!(value(r#""a" && "b""#), Typed::Bool(true));
    assert_eq!(value(r#"missing || "b""#), Typed::Bool(true));
}

#[test]
fn a_text_converts_to_the_number_or_the_date_it_is_compared_with() {
    for source in [
        r#""7.50" > 5"#,
        r#"(price / age).toFixed(2) > 1"#,
        r#"score > "7""#,
        r#""7.5" == score"#,
        r#"due > "2025-06-19""#,
        r#"due == "2025-06-20""#,
        r#""2025-06-21" > due"#,
        r#"date("2025-06-20 10:00:00") > "2025-06-20 09:00""#,
    ] {
        assert!(holds(source), "{source}");
    }
    for source in [
        r#"score > "high""#,
        r#"score < "high""#,
        r#"score == "high""#,
        r#"due > "soon""#,
        r#"due == "soon""#,
    ] {
        assert!(!holds(source), "{source}");
    }
}

#[test]
fn plus_joins_text_and_lists() {
    assert_eq!(shown(r#"file.name + " - " + status"#), "Host.md - draft");
    assert_eq!(shown(r#""$" + price.toFixed(2)"#), "$4.00");
    assert_eq!(
        shown(r#"if(price, price.toFixed(2) + " dollars")"#),
        "4.00 dollars"
    );
    assert_eq!(shown(r#""n=" + 5"#), "n=5");
    assert_eq!(shown(r#"5 + "px""#), "5px");
    assert_eq!(shown(r#""Due: " + due"#), "Due: 2025-06-20");
    assert_eq!(shown(r#""is " + done"#), "is true");
    assert_eq!(value("aliases + [1]"), list([text("dc"), number(1.0)]));
}

#[test]
fn a_missing_value_stays_missing() {
    for source in [
        "missing + 1",
        "1 - missing",
        "missing * 2",
        r#"missing + " dollars""#,
        r#""$" + missing"#,
        "(missing / age).toFixed(2)",
        "missing.toFixed(2)",
        "missing.lower()",
        "missing.length",
        "missing.year",
        "missing[0]",
        "missing.a.b",
        "missing.asFile().properties.status",
        "-missing",
        "missing.map(value + 1)",
        "missing.reduce(acc + value, 0)",
        "score.toFixed(missing)",
        "date(missing)",
        "number(missing)",
        "link(missing)",
        "max(1, missing)",
        "now() + missing",
        "missing.format(\"YYYY\")",
        "note.missing",
        "file.properties.missing",
    ] {
        assert_eq!(value(source), Typed::Null, "{source}");
    }
    assert_eq!(value("list(missing)"), list([Typed::Null]));
    assert_eq!(value("[missing].contains(null)"), Typed::Bool(true));
    assert_eq!(value("if(missing, 1, 2)"), number(2.0));
}

#[test]
fn a_question_asked_of_the_wrong_type_answers_no() {
    for source in [
        r#"missing.contains("x")"#,
        r#"score.startsWith("7")"#,
        r#"score.contains(7)"#,
        r#"status.contains(1)"#,
        r#"done.endsWith("e")"#,
        r#"status.hasTag("draft")"#,
        r#"missing.inFolder("Labs")"#,
        r#"missing.hasLink("Index")"#,
        r#"file.hasLink(5)"#,
        r#"status.matches("draft")"#,
        r#"aliases.contains("d")"#,
    ] {
        assert_eq!(value(source), Typed::Bool(false), "{source}");
    }
    assert!(holds(r#"status.contains("raf")"#));
    assert!(holds(r#"aliases.contains("dc")"#));
    assert!(holds(r#"file.name.endsWith(".md")"#));
}

#[test]
fn an_operation_without_meaning_for_its_types_is_an_error() {
    assert_eq!(
        eval(r#""text" * 2"#),
        Err(EvalError::Operator {
            operator: "*",
            left: "string",
            right: "number"
        })
    );
    assert_eq!(
        eval("done + 1"),
        Err(EvalError::Operator {
            operator: "+",
            left: "boolean",
            right: "number"
        })
    );
    assert_eq!(
        eval(r#"status - 1"#),
        Err(EvalError::Operator {
            operator: "-",
            left: "string",
            right: "number"
        })
    );
    assert_eq!(eval("-status"), Err(EvalError::Negate("string")));
    assert_eq!(
        eval("status.toFixed(2)"),
        Err(EvalError::NoMethod {
            method: "toFixed",
            on: "string"
        })
    );
    assert_eq!(
        eval("aliases.lower()"),
        Err(EvalError::NoMethod {
            method: "lower",
            on: "list"
        })
    );
    assert_eq!(
        eval("score.map(value)"),
        Err(EvalError::NoMethod {
            method: "map",
            on: "number"
        })
    );
    assert_eq!(
        eval("score.length"),
        Err(EvalError::NoField {
            field: "length".into(),
            on: "number"
        })
    );
    assert_eq!(
        eval("score[0]"),
        Err(EvalError::NoIndex {
            on: "number",
            key: "number"
        })
    );
    assert_eq!(
        eval(r#"score.toFixed("2")"#),
        Err(EvalError::Argument {
            function: "toFixed",
            position: 1,
            expected: "number",
            found: "string"
        })
    );
    assert_eq!(
        eval(r#"max(1, "2")"#),
        Err(EvalError::Argument {
            function: "max",
            position: 2,
            expected: "number",
            found: "string"
        })
    );
    assert_eq!(
        eval("formula.age"),
        Err(EvalError::UnknownFormula("age".into()))
    );
    assert_eq!(value(r#"if(true, 1, "text" * 2)"#), number(1.0));
    assert_eq!(value(r#"false && "text" * 2"#), Typed::Bool(false));
    assert_eq!(value(r#"true || "text" * 2"#), Typed::Bool(true));
}

#[test]
fn links_files_and_tags_compare_by_what_they_name() {
    for source in [
        r#"link("Index") == link("[[Index|Home]]")"#,
        r#"link("Index") == file("Index")"#,
        r#"file("Index") == link("Index")"#,
        r#"link("Index") == this"#,
        r#"this == file("Index")"#,
        r#"link("Index") == "Index""#,
        r#"link("Index") == "[[Index]]""#,
        r#"link("nowhere") == link("nowhere")"#,
        r#"link("nowhere") != link("elsewhere")"#,
        r#"link("nowhere") != link("Index")"#,
        r#"[link("Index"), link("nowhere")].contains(this)"#,
        r#"[link("Index")].contains(file("Index"))"#,
        r#"file.links.contains(this.file)"#,
        r#"file.tags.contains("attack")"#,
        r##"file.tags.contains("#Attack")"##,
        r#"tags.contains("attack")"#,
        r#"file.tags.containsAny("x", "draft")"#,
        r#"file.tags.containsAll("attack/ad", "draft")"#,
        r##"file.tags[1] == "#draft""##,
        r#"file.tags[1] == "DRAFT""#,
    ] {
        assert!(holds(source), "{source}");
    }
    for source in [
        r#"file.tags.contains("att")"#,
        r#"file.tags[0] == "attack""#,
        r#"link("Index") == file"#,
        r#"file == this"#,
    ] {
        assert!(!holds(source), "{source}");
    }
    assert_eq!(shown("file.tags"), "#attack/ad, #draft");
    assert_eq!(shown(r#"file.tags[1].lower()"#), "#draft");
}

#[test]
fn this_reads_the_file_that_shows_the_base() {
    assert_eq!(shown("this.file.name"), "Index.md");
    assert_eq!(shown("this.file.folder"), "");
    assert_eq!(shown("this.status"), "ready");
    assert_eq!(shown(r#"this["status"]"#), "ready");
    assert_eq!(value("this.hours + 1"), number(4.0));
    assert!(holds(r#"this.hasProperty("hours")"#));
    let memory = vault();
    let nowhere = focus_bases::Scope::new(&memory, now()).file("Labs/Network/Host.md");
    for source in ["this", "this.file", "this.file.name", "this.status"] {
        assert_eq!(
            compile(source).unwrap().eval(&nowhere),
            Ok(Typed::Null),
            "{source}"
        );
    }
}

#[test]
fn value_index_and_acc_are_names_inside_a_list_function_only() {
    let mut memory = vault();
    memory.files[0].props.push(("value", number(40.0)));
    memory.files[0].props.push(("acc", number(2.0)));
    assert_eq!(eval_in(&memory, "value + acc"), Ok(number(42.0)));
    assert_eq!(
        eval_in(&memory, "[1, 2].map(value + acc)"),
        Ok(numbers(&[3.0, 4.0]))
    );
    assert_eq!(
        eval_in(&memory, "[1, 2].reduce(acc + value, value)"),
        Ok(number(43.0))
    );
    assert_eq!(
        eval_in(&memory, "[1, 2].reduce(acc + [10].map(value + acc)[0], 0)"),
        Ok(number(30.0))
    );
}

#[test]
fn what_the_documentation_does_not_have_is_named() {
    let cases = [
        (
            r#"status.toLowerCase() == "x""#,
            Feature::Method,
            "toLowerCase",
        ),
        (r#"[1].sum()"#, Feature::Method, "sum"),
        (
            r#"file.frobnicate("a")"#,
            Feature::Function,
            "file.frobnicate",
        ),
        (r#"file.inode > 1"#, Feature::Property, "file.inode"),
        (r#"dateBefore(a, b)"#, Feature::Function, "dateBefore"),
        (r#"checkbox(done)"#, Feature::Function, "checkbox"),
        (r#"formula > 1"#, Feature::Formula, "formula"),
        (r#"formula[name] > 1"#, Feature::Index, "formula[...]"),
    ];
    for (source, feature, subject) in cases {
        match compile(source) {
            Err(ExprError::Unsupported(found, value)) => {
                assert_eq!((found, value.as_str()), (feature, subject), "{source}")
            }
            other => panic!("{source}: {other:?}"),
        }
    }
    for (source, subject) in [
        ("values.length + price", "price"),
        ("file.name", "file"),
        ("note.price", "note"),
        ("formula.ppu", "formula"),
    ] {
        match compile_summary(source) {
            Err(ExprError::Unsupported(Feature::Property, value)) => {
                assert_eq!(value, subject, "{source}")
            }
            other => panic!("{source}: {other:?}"),
        }
    }
    assert!(compile_summary("values.mean().round(3)").is_ok());
    assert!(compile_summary("values.filter(value > this.hours).length").is_ok());
    assert_eq!(value("values"), Typed::Null);
}

#[test]
fn a_malformed_expression_is_a_syntax_error() {
    for source in [
        "",
        r#"status == "draft"#,
        "status ==",
        "(status",
        "a = 1",
        "a b",
        "1 +",
        "[1, 2",
        "{a: 1",
        "{a 1}",
        "a.",
        "/abc",
        "/(?=a)b/",
        "/a/y",
        "file.inFolder()",
        r#"file.inFolder("a", "b")"#,
        "if(a)",
        "now(1)",
        "max()",
        "score.round(1, 2)",
        "[1].reduce(acc + value)",
        "status.isEmpty(1)",
    ] {
        assert!(
            matches!(compile(source), Err(ExprError::Syntax(_))),
            "{source:?}: {:?}",
            compile(source)
        );
    }
}

#[test]
fn an_expression_has_a_bounded_depth() {
    let nested = |levels: usize| format!("{}1{}", "(".repeat(levels), ")".repeat(levels));
    let chained = |links: usize| format!("1{}", " + 1".repeat(links));
    let called = |links: usize| format!("status{}", ".trim()".repeat(links));
    assert_eq!(value(&nested(90)), number(1.0));
    assert_eq!(value(&chained(90)), number(91.0));
    assert_eq!(value(&called(90)), text("draft"));
    assert_eq!(
        value(&format!("({}) * ({})", chained(90), chained(90))),
        number(8281.0)
    );
    for source in [
        nested(120),
        chained(120),
        called(120),
        "1".to_string() + &" + 1".repeat(100_000),
    ] {
        assert!(
            matches!(compile(&source), Err(ExprError::Syntax(message)) if message.contains("nested too deeply")),
            "{}",
            &source[..40]
        );
    }
    assert_eq!(
        eval(r#""x".repeat(100).split("").reduce([acc], 0)"#),
        Err(EvalError::TooDeep)
    );
    assert_eq!(
        value(r#""x".repeat(1000).split("").reduce(acc + [index], []).length"#),
        number(1000.0)
    );
}
