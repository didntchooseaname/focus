mod support;

use focus_bases::{EvalError, Typed, compile_summary};
use support::*;

fn check(cases: &[(&str, Typed)]) {
    for (source, expected) in cases {
        assert_eq!(&value(source), expected, "{source}");
    }
}

#[test]
fn global_functions() {
    check(&[
        (
            r#"escapeHTML("<b>a & 'b'</b>")"#,
            text("&lt;b&gt;a &amp; &#39;b&#39;&lt;/b&gt;"),
        ),
        (
            r#"date("2025-01-01 12:00:00").format("YYYY-MM-DD HH:mm:ss")"#,
            text("2025-01-01 12:00:00"),
        ),
        (
            r#"(now() + (duration('1d') * 2)).format("YYYY-MM-DD HH:mm")"#,
            text("2025-06-17 12:00"),
        ),
        (r#"(now() + '1d').format("YYYY-MM-DD")"#, text("2025-06-16")),
        (r#"file("Index")"#, Typed::File("Index.md".into())),
        (r#"file(link("[[Index]]"))"#, Typed::File("Index.md".into())),
        (r#"file("nowhere")"#, Typed::Null),
        (r#"html("<b>x</b>")"#, Typed::Html("<b>x</b>".into())),
        (
            r#"if(isModified, "Modified", "Unmodified")"#,
            text("Modified"),
        ),
        (
            r#"if(missing, "Modified", "Unmodified")"#,
            text("Unmodified"),
        ),
        (r#"if(missing, "Modified")"#, Typed::Null),
        (
            r#"image("https://obsidian.md/images/obsidian-logo-gradient.svg")"#,
            Typed::Image("https://obsidian.md/images/obsidian-logo-gradient.svg".into()),
        ),
        (
            r#"image(file("diagram.png"))"#,
            Typed::Image("diagram.png".into()),
        ),
        (r#"icon("arrow-right")"#, Typed::Icon("arrow-right".into())),
        (r#"list("value")"#, texts(&["value"])),
        (r#"list(aliases)"#, texts(&["dc"])),
        ("max(1, 5, 3)", number(5.0)),
        ("min(1, 5, 3)", number(1.0)),
        (r#"number("3.4")"#, number(3.4)),
        ("number(true)", number(1.0)),
        ("number(false)", number(0.0)),
        (
            r#"number(date("2025-01-01T00:00:00Z"))"#,
            number(1_735_689_600_000.0),
        ),
        (
            r#"now().format("YYYY-MM-DD HH:mm:ss")"#,
            text("2025-06-15 12:00:00"),
        ),
        (
            r#"today().format("YYYY-MM-DD HH:mm:ss")"#,
            text("2025-06-15 00:00:00"),
        ),
    ]);
    assert_eq!(
        eval(r#"number("abc")"#),
        Err(EvalError::Parse {
            what: "number",
            text: "abc".into()
        })
    );
    assert_eq!(
        eval(r#"number("")"#),
        Err(EvalError::Parse {
            what: "number",
            text: String::new()
        })
    );
    for _ in 0..100 {
        match value("random()") {
            Typed::Number(n) => assert!((0.0..1.0).contains(&n), "{n}"),
            other => panic!("{other:?}"),
        }
    }
    assert_ne!(value("random()"), value("random()"));
}

#[test]
fn link_makes_a_link_or_a_url() {
    match value(r#"link("Index")"#) {
        Typed::Link(link) => {
            assert_eq!(link.target, "Index");
            assert_eq!(link.file.as_deref(), Some("Index.md"));
            assert_eq!(link.display, None);
        }
        other => panic!("{other:?}"),
    }
    match value(r#"link("[[Index#Top|Home]]")"#) {
        Typed::Link(link) => {
            assert_eq!(link.anchor.as_deref(), Some("Top"));
            assert_eq!(link.display.as_deref(), Some(&text("Home")));
        }
        other => panic!("{other:?}"),
    }
    match value(r#"link("filename", "display")"#) {
        Typed::Link(link) => {
            assert_eq!(link.file, None);
            assert_eq!(link.display.as_deref(), Some(&text("display")));
        }
        other => panic!("{other:?}"),
    }
    match value(r#"link("filename", icon("plus"))"#) {
        Typed::Link(link) => {
            assert_eq!(link.display.as_deref(), Some(&Typed::Icon("plus".into())))
        }
        other => panic!("{other:?}"),
    }
    match value(r#"link("https://obsidian.md", "Obsidian")"#) {
        Typed::Url(url) => {
            assert_eq!(url.address, "https://obsidian.md");
            assert_eq!(url.display.as_deref(), Some(&text("Obsidian")));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        value(r#"link("obsidian://open?vault=a b")"#),
        Typed::Url(_)
    ));
    assert_eq!(shown(r#"link(file.mtime.date().toString())"#), "2025-06-14");
}

#[test]
fn functions_of_any_type() {
    check(&[
        ("(1).isTruthy()", Typed::Bool(true)),
        ("(0).isTruthy()", Typed::Bool(false)),
        (r#""".isTruthy()"#, Typed::Bool(false)),
        ("missing.isTruthy()", Typed::Bool(false)),
        (r#""example".isType("string")"#, Typed::Bool(true)),
        (r#"true.isType("boolean")"#, Typed::Bool(true)),
        (r#"score.isType("number")"#, Typed::Bool(true)),
        (r#"score.isType("string")"#, Typed::Bool(false)),
        (r#"missing.isType("null")"#, Typed::Bool(true)),
        (r#"now().isType("date")"#, Typed::Bool(true)),
        (r#"[1].isType("list")"#, Typed::Bool(true)),
        (r#"file.isType("file")"#, Typed::Bool(true)),
        ("(123).toString()", text("123")),
        ("(1.5).toString()", text("1.5")),
        ("true.toString()", text("true")),
        ("[1, 2].toString()", text("1, 2")),
        (r#"date("2025-05-27").toString()"#, text("2025-05-27")),
        (
            r#"date("2025-05-27 08:00:00").toString()"#,
            text("2025-05-27 08:00:00"),
        ),
        ("missing.toString()", Typed::Null),
    ]);
    assert_eq!(
        eval(r#"score.isType("text")"#),
        Err(EvalError::UnknownType("text".into()))
    );
}

#[test]
fn date_fields_and_functions() {
    check(&[
        (r#"date("2025-05-27 13:14:15").year"#, number(2025.0)),
        (r#"date("2025-05-27 13:14:15").month"#, number(5.0)),
        (r#"date("2025-05-27 13:14:15").day"#, number(27.0)),
        (r#"date("2025-05-27 13:14:15").hour"#, number(13.0)),
        (r#"date("2025-05-27 13:14:15").minute"#, number(14.0)),
        (r#"date("2025-05-27 13:14:15").second"#, number(15.0)),
        (
            r#"date("2025-05-27 13:14:15.250").millisecond"#,
            number(250.0),
        ),
        (
            r#"now().date().format("YYYY-MM-DD HH:mm:ss")"#,
            text("2025-06-15 00:00:00"),
        ),
        (
            r#"date("2025-05-27").format("YYYY-MM-DD")"#,
            text("2025-05-27"),
        ),
        (r#"due.format("YYYY-MM-DD")"#, text("2025-06-20")),
        (
            r#"date("2025-05-27 13:04:05").format("dddd, MMMM Do YYYY, h:mm:ss a")"#,
            text("Tuesday, May 27th 2025, 1:04:05 pm"),
        ),
        (
            r#"date("2025-05-27 13:04:05").format("ddd MMM D YY [at] HH[h] Q DDD E")"#,
            text("Tue May 27 25 at 13h 2 147 2"),
        ),
        (
            r#"date("2025-05-27 13:04:05.120").format("M/DD k:m:s.SSS A W GGGG")"#,
            text("5/27 13:4:5.120 PM 22 2025"),
        ),
        (r#"date("2025-05-27 23:59:59").time()"#, text("23:59:59")),
        ("now().isEmpty()", Typed::Bool(false)),
        (
            r#"date("20250527").format("YYYY-MM-DD")"#,
            text("2025-05-27"),
        ),
        (
            r#"date("202505271314").format("YYYY-MM-DD HH:mm")"#,
            text("2025-05-27 13:14"),
        ),
        (
            r#"date("2025-12-31T23:59").format("YYYY-MM-DD HH:mm")"#,
            text("2025-12-31 23:59"),
        ),
        (
            r#"date("2025-12-31T23:59:59+02:00") == date("2025-12-31T21:59:59Z")"#,
            Typed::Bool(true),
        ),
    ]);
    assert_eq!(shown(r#"(now() - "3d").relative()"#), "3 days ago");
    assert_eq!(shown(r#"(now() + "2h").relative()"#), "in 2 hours");
    assert_eq!(shown(r#"(now() - "10s").relative()"#), "a few seconds ago");
    assert_eq!(shown(r#"(now() - "1y").relative()"#), "a year ago");
    assert_eq!(shown("file.mtime.relative()"), "a day ago");
    assert!(matches!(value("file.mtime.relative()"), Typed::Date(d) if d.relative));
    assert_eq!(
        eval(r#"date("01/01/2025")"#),
        Err(EvalError::Parse {
            what: "date",
            text: "01/01/2025".into()
        })
    );
    assert_eq!(
        eval(r#"now().format("LL")"#),
        Err(EvalError::Format("LL".into()))
    );
    assert!(matches!(
        eval("now().century"),
        Err(EvalError::NoField { on: "date", .. })
    ));
}

#[test]
fn string_fields_and_functions() {
    check(&[
        (r#""hello".length"#, number(5.0)),
        (r#""héllo".length"#, number(5.0)),
        (r#""hello".contains("ell")"#, Typed::Bool(true)),
        (r#""hello".contains("ELL")"#, Typed::Bool(false)),
        (r#""hello".containsAll("h", "e")"#, Typed::Bool(true)),
        (r#""hello".containsAll("h", "x")"#, Typed::Bool(false)),
        (r#""hello".containsAny("x", "y", "e")"#, Typed::Bool(true)),
        (r#""hello".containsAny("x", "y")"#, Typed::Bool(false)),
        (r#""hello".endsWith("lo")"#, Typed::Bool(true)),
        (r#""Hello world".isEmpty()"#, Typed::Bool(false)),
        (r#""".isEmpty()"#, Typed::Bool(true)),
        ("missing.isEmpty()", Typed::Bool(true)),
        (r#""HeLLo".lower()"#, text("hello")),
        (r#""a:b:c:d".replace(/:/, "-")"#, text("a-b:c:d")),
        (r#""a:b:c:d".replace(/:/g, "-")"#, text("a-b-c-d")),
        (r#""a:b:c:d".replace(":", "-")"#, text("a-b-c-d")),
        (
            r#""John Smith".replace(/(\w+) (\w+)/, "$2, $1")"#,
            text("Smith, John"),
        ),
        (r#""a1b22".replace(/\d+/g, "<$&>")"#, text("a<1>b<22>")),
        (
            r#""cost".replace(/(?<w>cost)/, "$<w> $$5")"#,
            text("cost $5"),
        ),
        (r#""a/b".replace(/\//, "-")"#, text("a-b")),
        (r#""123".repeat(2)"#, text("123123")),
        (r#""hello".reverse()"#, text("olleh")),
        (r#""hello".slice(1, 4)"#, text("ell")),
        (r#""hello".slice(2)"#, text("llo")),
        (r#""hello".slice(-3)"#, text("llo")),
        (r#""hello".slice(1, -1)"#, text("ell")),
        (r#""a,b,c,d".split(",", 3)"#, texts(&["a", "b", "c"])),
        (r#""a,b,c,d".split(/,/, 3)"#, texts(&["a", "b", "c"])),
        (r#""a,b".split(",")"#, texts(&["a", "b"])),
        (r#""ab".split("")"#, texts(&["a", "b"])),
        (r#""hello".startsWith("he")"#, Typed::Bool(true)),
        (r#""hello world".title()"#, text("Hello World")),
        (r#""  hi  ".trim()"#, text("hi")),
        (r#""A B".split(' ').sort()[0].lower()"#, text("a")),
    ]);
    assert_eq!(
        eval(r#""x".repeat(10000000)"#),
        Err(EvalError::TooLarge { function: "repeat" })
    );
}

#[test]
fn number_functions() {
    check(&[
        ("(-5).abs()", number(5.0)),
        ("(2.1).ceil()", number(3.0)),
        ("(2.9).floor()", number(2.0)),
        ("(0).floor()", number(0.0)),
        ("(5).isEmpty()", Typed::Bool(false)),
        ("missing.isEmpty()", Typed::Bool(true)),
        ("(2.5).round()", number(3.0)),
        ("(-2.5).round()", number(-2.0)),
        ("(2.4).round()", number(2.0)),
        ("(2.3333).round(2)", number(2.33)),
        ("(3.14159).toFixed(2)", text("3.14")),
        ("(2.5).toFixed(0)", text("3")),
        ("(0.125).toFixed(2)", text("0.13")),
        ("(1.005).toFixed(2)", text("1.00")),
        ("(-1.5).toFixed(0)", text("-2")),
        ("(4).toFixed(2)", text("4.00")),
    ]);
}

#[test]
fn list_fields_and_functions() {
    check(&[
        ("[1, 2, 3].length", number(3.0)),
        ("[1,2,3].contains(2)", Typed::Bool(true)),
        ("[1,2,3].contains(4)", Typed::Bool(false)),
        ("[1,2,3].containsAll(2,3)", Typed::Bool(true)),
        ("[1,2,3].containsAll(2,4)", Typed::Bool(false)),
        ("[1,2,3].containsAny(3,4)", Typed::Bool(true)),
        ("[1,2,3].containsAny(4,5)", Typed::Bool(false)),
        ("[1,2,3,4].filter(value > 2)", numbers(&[3.0, 4.0])),
        ("[5,6,7].filter(index > 0)", numbers(&[6.0, 7.0])),
        ("[1,[2,3]].flat()", numbers(&[1.0, 2.0, 3.0])),
        ("[1,2,3].isEmpty()", Typed::Bool(false)),
        ("[].isEmpty()", Typed::Bool(true)),
        (r#"[1,2,3].join(",")"#, text("1,2,3")),
        ("[1,2,3,4].map(value + 1)", numbers(&[2.0, 3.0, 4.0, 5.0])),
        ("[5,5].map(value * index)", numbers(&[0.0, 5.0])),
        ("[1,2,3].reduce(acc + value, 0)", number(6.0)),
        (
            "[3, 9, 4].reduce(if(acc == null || value > acc, value, acc), null)",
            number(9.0),
        ),
        ("[1,2,3].reverse()", numbers(&[3.0, 2.0, 1.0])),
        ("[1,2,3,4].slice(1,3)", numbers(&[2.0, 3.0])),
        ("[1,2,3,4].slice(2)", numbers(&[3.0, 4.0])),
        ("[3, 1, 2].sort()", numbers(&[1.0, 2.0, 3.0])),
        (r#"["c", "a", "b"].sort()"#, texts(&["a", "b", "c"])),
        ("[1,2,2,3].unique()", numbers(&[1.0, 2.0, 3.0])),
        ("[1,2,3][0]", number(1.0)),
        ("[1,2,3][5]", Typed::Null),
        ("[1,2] + [3]", numbers(&[1.0, 2.0, 3.0])),
        ("[1, 2, 3, 6].mean()", number(3.0)),
        ("[1, 2, 3, 6].mean().round(3)", number(3.0)),
        ("[5, 1, 3].median()", number(3.0)),
        ("[5, 1, 3, 2].median()", number(2.5)),
        ("[2, 4, 4, 4, 5, 5, 7, 9].stddev()", number(2.0)),
        ("[].mean()", Typed::Null),
        (
            "[[1,2],[3]].map(value.map(value * 2)).flat()",
            numbers(&[2.0, 4.0, 6.0]),
        ),
        ("aliases.length", number(1.0)),
        ("list(status)[0]", text("draft")),
    ]);
    let formula = compile_summary(
        r#"values.filter(value.isType("number")).reduce(if(acc == null || value > acc, value, acc), null)"#,
    )
    .unwrap();
    let memory = vault();
    let column = list([number(2.0), text("x"), number(8.0), Typed::Null]);
    let scope = summary_scope(&memory, &column);
    assert_eq!(formula.eval(&scope), Ok(number(8.0)));
    let empty = list([text("x")]);
    assert_eq!(
        formula.eval(&summary_scope(&memory, &empty)),
        Ok(Typed::Null)
    );
}

#[test]
fn link_and_file_functions() {
    check(&[
        (
            r#"link("[[Index]]").asFile()"#,
            Typed::File("Index.md".into()),
        ),
        (r#"link("[[nowhere]]").asFile()"#, Typed::Null),
        (r#"link("Index").asFile().properties.status"#, text("ready")),
        (r#"link("Index").linksTo(file)"#, Typed::Bool(true)),
        (r#"link("Notes/Other").linksTo(file)"#, Typed::Bool(false)),
        (r#"link("Notes/Other").linksTo("Index")"#, Typed::Bool(true)),
        (r#"file.hasLink("Index")"#, Typed::Bool(true)),
        (r#"file.hasLink("[[Index#Top|Home]]")"#, Typed::Bool(true)),
        (r#"file.hasLink(link("Index"))"#, Typed::Bool(true)),
        (r#"file.hasLink(file("Index"))"#, Typed::Bool(true)),
        (r#"file.hasLink(this.file)"#, Typed::Bool(true)),
        (r#"file.hasLink(this)"#, Typed::Bool(true)),
        (r#"file.hasLink("Notes/Other")"#, Typed::Bool(false)),
        (r#"file.hasLink("[[Missing]]")"#, Typed::Bool(true)),
        (r#"file.hasLink("Absent")"#, Typed::Bool(false)),
        (r#"file.hasLink("diagram.png")"#, Typed::Bool(true)),
        (r#"file.hasProperty("status")"#, Typed::Bool(true)),
        (r#"file.hasProperty("blank")"#, Typed::Bool(true)),
        (r#"file.hasProperty("nope")"#, Typed::Bool(false)),
        (r#"file.hasTag("attack")"#, Typed::Bool(true)),
        (r##"file.hasTag("#Attack/AD")"##, Typed::Bool(true)),
        (r#"file.hasTag("tag1", "draft")"#, Typed::Bool(true)),
        (r#"file.hasTag("att")"#, Typed::Bool(false)),
        (r#"file.inFolder("Labs")"#, Typed::Bool(true)),
        (r#"file.inFolder("Labs/Network")"#, Typed::Bool(true)),
        (r#"file.inFolder("Labs/")"#, Typed::Bool(true)),
        (r#"file.inFolder("Lab")"#, Typed::Bool(false)),
        (r#"file.inFolder("Labs/Network/Deep")"#, Typed::Bool(false)),
        (r#"file.asLink().asFile() == file"#, Typed::Bool(true)),
        (r#"file.asLink() == file"#, Typed::Bool(true)),
    ]);
    match value(r#"file.asLink("Shown")"#) {
        Typed::Link(link) => {
            assert_eq!(link.file.as_deref(), Some("Labs/Network/Host.md"));
            assert_eq!(link.display.as_deref(), Some(&text("Shown")));
            assert_eq!(link.wikilink(), "[[Labs/Network/Host|Shown]]");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn file_fields() {
    check(&[
        ("file.name", text("Host.md")),
        ("file.basename", text("Host")),
        ("file.path", text("Labs/Network/Host.md")),
        ("file.folder", text("Labs/Network")),
        ("file.ext", text("md")),
        ("file.size", number(2048.0)),
        ("file.file", Typed::File("Labs/Network/Host.md".into())),
        ("file", Typed::File("Labs/Network/Host.md".into())),
        ("file.properties.status", text("draft")),
        (r#"file.properties["a b"]"#, number(1.0)),
        ("file.properties.isEmpty()", Typed::Bool(false)),
        (
            "file.tags",
            list([Typed::Tag("attack/ad".into()), Typed::Tag("draft".into())]),
        ),
        (
            r#"file.mtime.format("YYYY-MM-DD HH:mm")"#,
            text("2025-06-14 09:30"),
        ),
        (r#"file.ctime.format("YYYY-MM-DD")"#, text("2025-06-14")),
        ("file.links.length", number(3.0)),
        ("file.embeds.length", number(1.0)),
        (
            "file.links.map(value.asFile())",
            list([
                Typed::File("Index.md".into()),
                Typed::Null,
                Typed::File("diagram.png".into()),
            ]),
        ),
        ("file.embeds[0].asFile().ext", text("png")),
        (
            "file.backlinks.map(value.asFile().path)",
            texts(&["Index.md"]),
        ),
        (
            "file.backlinks.map(value.asFile().properties.hours).reduce(acc + value, 0)",
            number(3.0),
        ),
        (r#"file("diagram.png").size"#, number(10.0)),
        (
            r#"file("diagram.png").properties.isEmpty()"#,
            Typed::Bool(true),
        ),
        (r#"file("Index").name"#, text("Index.md")),
        (r#"file("Index").folder"#, text("")),
    ]);
    assert!(matches!(
        eval(r#"file("Index").nope"#),
        Err(EvalError::NoField { on: "file", .. })
    ));
}

#[test]
fn object_and_regular_expression_functions() {
    check(&[
        (r#"{"name": "value"}.isEmpty()"#, Typed::Bool(false)),
        ("{}.isEmpty()", Typed::Bool(true)),
        (r#"{"a": 1, b: 2}.keys()"#, texts(&["a", "b"])),
        (r#"{"a": 1, b: 2}.values()"#, numbers(&[1.0, 2.0])),
        (r#"{"a": 1}.a"#, number(1.0)),
        (r#"{"a": 1}["a"]"#, number(1.0)),
        (r#"{"a": 1}.b"#, Typed::Null),
        (
            "file.properties.keys().slice(0, 2)",
            texts(&["status", "score"]),
        ),
        (r#"/abc/.matches("abcde")"#, Typed::Bool(true)),
        (r#"/^abc$/.matches("abcde")"#, Typed::Bool(false)),
        (r#"/ABC/i.matches("abcde")"#, Typed::Bool(true)),
        (r#"/a.c/s.matches("a\nc")"#, Typed::Bool(true)),
        (r#"/[/]b/.matches("a/b")"#, Typed::Bool(true)),
        (r#"/abc/.matches(score)"#, Typed::Bool(false)),
    ]);
}

#[test]
fn a_url_takes_the_functions_of_a_string() {
    check(&[
        (
            r#"link("https://Obsidian.md").lower()"#,
            text("https://obsidian.md"),
        ),
        (
            r#"link("https://obsidian.md").startsWith("https")"#,
            Typed::Bool(true),
        ),
        (r#"link("https://obsidian.md").length"#, number(19.0)),
    ]);
    assert_eq!(
        eval(r#"link("https://obsidian.md").asFile()"#),
        Err(EvalError::NoMethod {
            method: "asFile",
            on: "url"
        })
    );
}
