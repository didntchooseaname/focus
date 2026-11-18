mod support;

use focus_bases::{Date, Duration, EvalError, Typed};
use support::*;

const STAMP: &str = "YYYY-MM-DD HH:mm:ss";

fn stamp(source: &str) -> String {
    shown(&format!("({source}).format(\"{STAMP}\")"))
}

#[test]
fn a_duration_text_moves_a_date() {
    assert_eq!(
        stamp(r#"date("2024-12-01") + "1M" + "4h" + "3m""#),
        "2025-01-01 04:03:00"
    );
    assert_eq!(stamp(r#"now() + "1 day""#), "2025-06-16 12:00:00");
    assert_eq!(stamp(r#"now() + "1 hour""#), "2025-06-15 13:00:00");
    assert_eq!(stamp(r#"now() - "2h""#), "2025-06-15 10:00:00");
    assert_eq!(stamp(r#"today() + "7d""#), "2025-06-22 00:00:00");
    assert_eq!(stamp(r#"due + "2w""#), "2025-07-04 00:00:00");
    assert_eq!(
        stamp(r#"now() + duration("1d") * 2"#),
        "2025-06-17 12:00:00"
    );
    assert_eq!(stamp(r#"duration("1d") + now()"#), "2025-06-16 12:00:00");
    assert_eq!(stamp(r#"now() - duration("1h")"#), "2025-06-15 11:00:00");
    assert!(holds(r#"file.mtime > now() - "1 week""#));
    assert!(holds(r#"file.mtime > now() - '7d'"#));
    assert!(!holds(r#"file.mtime > now() - "1 hour""#));
    assert!(holds(r#"due > now() && status != "Done""#));
}

#[test]
fn every_unit_of_the_documentation() {
    for (text, expected) in [
        ("1y", "2026-06-15 12:00:00"),
        ("1 year", "2026-06-15 12:00:00"),
        ("2 years", "2027-06-15 12:00:00"),
        ("1M", "2025-07-15 12:00:00"),
        ("1 month", "2025-07-15 12:00:00"),
        ("2 months", "2025-08-15 12:00:00"),
        ("1d", "2025-06-16 12:00:00"),
        ("1 day", "2025-06-16 12:00:00"),
        ("2 days", "2025-06-17 12:00:00"),
        ("1w", "2025-06-22 12:00:00"),
        ("1 week", "2025-06-22 12:00:00"),
        ("2 weeks", "2025-06-29 12:00:00"),
        ("1h", "2025-06-15 13:00:00"),
        ("1 hour", "2025-06-15 13:00:00"),
        ("2 hours", "2025-06-15 14:00:00"),
        ("1m", "2025-06-15 12:01:00"),
        ("1 minute", "2025-06-15 12:01:00"),
        ("2 minutes", "2025-06-15 12:02:00"),
        ("1s", "2025-06-15 12:00:01"),
        ("1 second", "2025-06-15 12:00:01"),
        ("2 seconds", "2025-06-15 12:00:02"),
        ("1d 12h", "2025-06-17 00:00:00"),
        ("1.5h", "2025-06-15 13:30:00"),
    ] {
        assert_eq!(stamp(&format!("now() + \"{text}\"")), expected, "{text}");
    }
    for text in ["", "soon", "1", "d", "1 fortnight", "1 Month", "P1D"] {
        assert_eq!(Duration::parse(text), None, "{text:?}");
        assert_eq!(
            eval(&format!("now() + \"{text}\"")),
            Err(EvalError::Parse {
                what: "duration",
                text: text.into()
            }),
            "{text:?}"
        );
    }
    assert_eq!(
        eval(r#"duration("soon")"#),
        Err(EvalError::Parse {
            what: "duration",
            text: "soon".into()
        })
    );
}

#[test]
fn a_month_ends_on_the_last_day_it_has() {
    assert_eq!(stamp(r#"date("2025-01-31") + "1M""#), "2025-02-28 00:00:00");
    assert_eq!(stamp(r#"date("2024-01-31") + "1M""#), "2024-02-29 00:00:00");
    assert_eq!(stamp(r#"date("2025-03-31") - "1M""#), "2025-02-28 00:00:00");
    assert_eq!(stamp(r#"date("2024-02-29") + "1y""#), "2025-02-28 00:00:00");
}

#[test]
fn two_dates_subtract_to_a_duration() {
    assert_eq!(shown(r#"date("2025-06-20") - date("2025-06-15")"#), "5d");
    assert_eq!(shown(r#"due - today()"#), "5d");
    assert_eq!(shown(r#"today() - due"#), "-5d");
    assert_eq!(
        shown(r#"date("2025-06-15 14:30:00") - date("2025-06-15 12:00:00")"#),
        "2h 30m"
    );
    assert_eq!(shown(r#"now() - now()"#), "0s");
    assert!(holds(r#"due - today() > duration("4d")"#));
    assert!(holds(r#"due - today() == duration("5d")"#));
    assert!(holds(r#"(due - today()).isType("duration")"#));
    assert_eq!(stamp(r#"today() + (due - today())"#), "2025-06-20 00:00:00");
}

#[test]
fn durations_have_their_own_arithmetic() {
    assert_eq!(shown(r#"duration("5h") * 2"#), "10h");
    assert_eq!(shown(r#"duration("1d") + duration("12h")"#), "1d 12h");
    assert_eq!(shown(r#"duration("1d") - duration("12h")"#), "12h");
    assert_eq!(shown(r#"duration("3h") / 2"#), "1h 30m");
    assert_eq!(shown(r#"duration("1y") + duration("2M")"#), "1y 2M");
    assert_eq!(shown(r#"-duration("90m")"#), "-1h 30m");
    assert_eq!(shown(r#"duration("14M")"#), "1y 2M");
    assert_eq!(shown(r#"duration("1500s")"#), "25m");
    assert_eq!(
        eval(r#"2 * duration("5h")"#),
        Err(EvalError::Operator {
            operator: "*",
            left: "number",
            right: "duration"
        })
    );
    assert_eq!(
        eval(r#"now() * 2"#),
        Err(EvalError::Operator {
            operator: "*",
            left: "date",
            right: "number"
        })
    );
    assert_eq!(
        eval(r#"now() + 1"#),
        Err(EvalError::Operator {
            operator: "+",
            left: "date",
            right: "number"
        })
    );
}

#[test]
fn a_date_without_time_compares_by_the_day() {
    assert!(holds(
        r#"date("2025-06-15") == date("2025-06-15 18:00:00")"#
    ));
    assert!(holds("today() == now()"));
    assert!(!holds("today() < now()"));
    assert!(holds(r#"today() < now() + "1d""#));
    assert!(holds(
        r#"date("2025-06-15 10:00:00") < date("2025-06-15 18:00:00")"#
    ));
    assert!(holds(
        r#"date("2025-06-15 10:00:00") != date("2025-06-15 18:00:00")"#
    ));
    assert!(holds("now().date() == today()"));
    assert_eq!(value("today().hour"), number(0.0));
    assert_eq!(shown("today()"), "2025-06-15");
    assert_eq!(shown("now()"), "2025-06-15 12:00:00");
    assert_eq!(shown(r#"today() + "1d""#), "2025-06-16");
    assert_eq!(shown(r#"today() + "1h""#), "2025-06-15 01:00:00");
}

#[test]
fn dates_are_read_from_the_documented_forms_only() {
    for written in [
        "2025-12-31",
        "2025-12-31 23:59",
        "2025-12-31 23:59:59",
        "2025-12-31T23:59",
        "2025-12-31T23:59:59",
        "2025-12-31T23:59:59.123",
        "2025-12-31T23:59:59Z",
        "2025-12-31T23:59:59-07",
        "2025-12-31T23:59:59+05:30",
        "2025-12-31T23:59:59+0530",
        "20251231",
        "202512312359",
    ] {
        assert!(Date::parse(written).is_some(), "{written}");
    }
    for written in [
        "",
        "2025",
        "2025-13-01",
        "2025-02-30",
        "2025-12-31 25:00",
        "2025-12-31T23:59:59 UTC",
        "31/12/2025",
        "12/31/2025",
        "tomorrow",
        "2025-12-31 and more",
    ] {
        assert_eq!(Date::parse(written), None, "{written}");
    }
    assert!(!date("2025-12-31").time);
    assert!(date("2025-12-31 00:00").time);
    let utc = date("2025-01-01T00:00:00Z");
    assert_eq!(utc.millis, 1_735_689_600_000);
    assert_eq!(date("2025-01-01T02:00:00+02:00").millis, utc.millis);
    assert_eq!(
        value(r#"date("2025-01-01T00:00:00Z").format("x")"#),
        text("1735689600000")
    );
    assert!(matches!(value(r#"date("2025-06-15")"#), Typed::Date(d) if !d.time));
}
