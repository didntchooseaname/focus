use std::collections::BTreeMap;

use crate::{PropValue, PropertyDate, PropertyKind, PropertyType, TypeOrigin, ValueError};

pub(crate) fn name(kind: PropertyType) -> &'static str {
    match kind {
        PropertyType::Text => "text",
        PropertyType::Multitext => "multitext",
        PropertyType::Number => "number",
        PropertyType::Checkbox => "checkbox",
        PropertyType::Date => "date",
        PropertyType::Datetime => "datetime",
        PropertyType::Tags => "tags",
        PropertyType::Aliases => "aliases",
    }
}

pub(crate) fn from_name(name: &str) -> Option<PropertyType> {
    Some(match name {
        "text" => PropertyType::Text,
        "multitext" => PropertyType::Multitext,
        "number" => PropertyType::Number,
        "checkbox" => PropertyType::Checkbox,
        "date" => PropertyType::Date,
        "datetime" => PropertyType::Datetime,
        "tags" => PropertyType::Tags,
        "aliases" => PropertyType::Aliases,
        _ => return None,
    })
}

pub(crate) fn read_types(json: &serde_json::Value) -> BTreeMap<String, PropertyType> {
    json.get("types")
        .and_then(|t| t.as_object())
        .into_iter()
        .flatten()
        .filter_map(|(name, kind)| Some((name.clone(), from_name(kind.as_str()?)?)))
        .collect()
}

pub(crate) fn lookup(types: &BTreeMap<String, PropertyType>, name: &str) -> Option<PropertyType> {
    types.get(name).copied().or_else(|| {
        types
            .iter()
            .find(|(k, _)| k.to_lowercase() == name.to_lowercase())
            .map(|(_, v)| *v)
    })
}

fn own_type(name: &str) -> Option<PropertyType> {
    match name {
        "tags" => Some(PropertyType::Tags),
        "aliases" => Some(PropertyType::Aliases),
        _ => None,
    }
}

pub(crate) fn resolve(
    name: &str,
    value: &PropValue,
    chosen: Option<PropertyType>,
    obsidian: Option<PropertyType>,
) -> PropertyKind {
    let own = own_type(name);
    let allowed = |kind: &PropertyType| {
        !matches!(kind, PropertyType::Tags | PropertyType::Aliases) || Some(*kind) == own
    };
    let obsidian = obsidian.filter(allowed);
    let (kind, origin) = if let Some(own) = own {
        (own, TypeOrigin::Default)
    } else if let Some(kind) = chosen.filter(allowed) {
        (kind, TypeOrigin::Focus)
    } else if let Some(kind) = obsidian {
        (kind, TypeOrigin::Obsidian)
    } else if name == "cssclasses" {
        (PropertyType::Multitext, TypeOrigin::Default)
    } else {
        (of_value(value), TypeOrigin::Value)
    };
    PropertyKind {
        kind,
        origin,
        obsidian,
    }
}

pub(crate) fn of_value(value: &PropValue) -> PropertyType {
    match value {
        PropValue::Bool(_) => PropertyType::Checkbox,
        PropValue::Number(_) => PropertyType::Number,
        PropValue::List(_) => PropertyType::Multitext,
        PropValue::Text(t) if parse_date(t, false).is_ok() => PropertyType::Date,
        PropValue::Text(t) if parse_date(t, true).is_ok() => PropertyType::Datetime,
        PropValue::Null | PropValue::Text(_) | PropValue::Map(_) => PropertyType::Text,
    }
}

pub(crate) fn fits(kind: PropertyType, value: &PropValue) -> bool {
    let scalar = |v: &PropValue| !matches!(v, PropValue::List(_) | PropValue::Map(_));
    match (kind, value) {
        (_, PropValue::Null) => true,
        (PropertyType::Text, v) => scalar(v),
        (PropertyType::Multitext | PropertyType::Tags | PropertyType::Aliases, v) => {
            matches!(v, PropValue::List(items) if items.iter().all(scalar))
        }
        (PropertyType::Number, v) => matches!(v, PropValue::Number(_)),
        (PropertyType::Checkbox, v) => matches!(v, PropValue::Bool(_)),
        (PropertyType::Date, PropValue::Text(t)) => parse_date(t, false).is_ok(),
        (PropertyType::Datetime, PropValue::Text(t)) => parse_date(t, true).is_ok(),
        (PropertyType::Date | PropertyType::Datetime, _) => false,
    }
}

fn number(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let digits = |i: &mut usize| {
        let from = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - from
    };
    let mut count = digits(&mut i);
    if b.get(i) == Some(&b'.') {
        i += 1;
        count += digits(&mut i);
    }
    if count == 0 {
        return false;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len() && text.parse::<f64>().is_ok_and(f64::is_finite)
}

pub(crate) fn check(kind: PropertyType, text: &str) -> Result<(), ValueError> {
    let text = text.trim();
    match kind {
        PropertyType::Number if text.is_empty() || number(text) => Ok(()),
        PropertyType::Number => Err(ValueError::NotNumber),
        PropertyType::Date | PropertyType::Datetime if text.is_empty() => Ok(()),
        PropertyType::Date => parse_date(text, false).map(|_| ()),
        PropertyType::Datetime => parse_date(text, true).map(|_| ()),
        _ => Ok(()),
    }
}

const DATE_FORM: &[u8] = b"dddd-dd-dd";
const DATETIME_FORM: &[u8] = b"dddd-dd-ddTdd:dd:dd";

fn starts_form(text: &str, form: &[u8]) -> bool {
    text.len() <= form.len()
        && text.bytes().zip(form).all(|(c, f)| match f {
            b'd' => c.is_ascii_digit(),
            f => c == *f,
        })
}

fn number_prefix(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let mut mantissa = false;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        mantissa = true;
        i += 1;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            mantissa = true;
            i += 1;
        }
    }
    if mantissa && matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    i == b.len()
}

pub(crate) fn accepts_typing(kind: PropertyType, text: &str) -> bool {
    match kind {
        PropertyType::Number => number_prefix(text),
        PropertyType::Date => starts_form(text, DATE_FORM),
        PropertyType::Datetime => starts_form(text, DATETIME_FORM),
        _ => true,
    }
}

fn digits(text: &str) -> Option<u32> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in(year: i32, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

pub(crate) fn parse_date(text: &str, with_time: bool) -> Result<PropertyDate, ValueError> {
    let form = if with_time {
        ValueError::DatetimeForm
    } else {
        ValueError::DateForm
    };
    let (date, time) = match text.split_once('T') {
        Some((date, time)) if with_time => (date, Some(time)),
        None if !with_time => (text, None),
        _ => return Err(form),
    };
    let b = date.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(form);
    }
    let (Some(year), Some(month), Some(day)) =
        (digits(&date[..4]), digits(&date[5..7]), digits(&date[8..]))
    else {
        return Err(form);
    };
    let year = year as i32;
    if !(1..=12).contains(&month) || day == 0 || day > days_in(year, month) {
        return Err(ValueError::NoSuchDay);
    }
    let time = match time {
        None => None,
        Some(time) => {
            let parts: Vec<&str> = time.split(':').collect();
            if !matches!(parts.len(), 2 | 3) || parts.iter().any(|p| p.len() != 2) {
                return Err(form);
            }
            let values: Option<Vec<u32>> = parts.iter().map(|p| digits(p)).collect();
            let Some(values) = values else {
                return Err(form);
            };
            if values[0] > 23 || values[1] > 59 || values.get(2).is_some_and(|s| *s > 59) {
                return Err(ValueError::NoSuchTime);
            }
            Some((values[0], values[1], values.get(2).copied()))
        }
    };
    Ok(PropertyDate {
        year,
        month,
        day,
        time,
    })
}

pub(crate) fn unix_seconds(date: &PropertyDate) -> i64 {
    let year = i64::from(date.year) - i64::from(date.month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = i64::from(date.month);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(date.day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let (hour, minute, second) = date.time.unwrap_or((0, 0, None));
    days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_meta;

    fn value(yaml_value: &str) -> PropValue {
        let meta = parse_meta(&format!("---\nkey: {yaml_value}\n---\n"));
        meta.props.into_iter().next().map(|(_, v)| v).unwrap()
    }

    #[test]
    fn each_documented_form_reads_back_as_its_type() {
        let cases = [
            ("A New Hope", PropertyType::Text),
            ("\"[[Episode IV]]\"", PropertyType::Text),
            ("https://www.example.com", PropertyType::Text),
            ("1977", PropertyType::Number),
            ("3.14", PropertyType::Number),
            ("true", PropertyType::Checkbox),
            ("false", PropertyType::Checkbox),
            ("2020-08-21", PropertyType::Date),
            ("2020-08-21T10:30:00", PropertyType::Datetime),
        ];
        for (written, kind) in cases {
            let read = value(written);
            assert_eq!(of_value(&read), kind, "{written}");
            assert!(fits(kind, &read), "{written}");
        }
        let list = parse_meta("---\ncast:\n  - Mark Hamill\n  - Harrison Ford\n---\n").props;
        assert_eq!(of_value(&list[0].1), PropertyType::Multitext);
        assert!(fits(PropertyType::Multitext, &list[0].1));
        assert!(fits(PropertyType::Tags, &list[0].1));
        assert!(fits(PropertyType::Checkbox, &value("")));
        assert!(fits(PropertyType::Date, &value("")));
    }

    #[test]
    fn values_out_of_their_form_do_not_fit() {
        assert!(!fits(PropertyType::Number, &value("12 + 3")));
        assert!(!fits(PropertyType::Number, &value("\"12\"")));
        assert!(!fits(PropertyType::Checkbox, &value("yes")));
        assert!(!fits(PropertyType::Date, &value("21/08/2020")));
        assert!(!fits(PropertyType::Date, &value("2020-02-30")));
        assert!(!fits(PropertyType::Date, &value("2020-08-21T10:30:00")));
        assert!(!fits(PropertyType::Datetime, &value("2020-08-21")));
        assert!(!fits(PropertyType::Datetime, &value("2020-08-21T25:00")));
        assert!(!fits(PropertyType::Multitext, &value("one")));
        assert!(!fits(PropertyType::Text, &value("[a, b]")));
        assert!(fits(PropertyType::Text, &value("12")));
    }

    #[test]
    fn checks_say_why() {
        assert_eq!(check(PropertyType::Number, "-1.5e3"), Ok(()));
        assert_eq!(
            check(PropertyType::Number, "1,5"),
            Err(ValueError::NotNumber)
        );
        assert_eq!(check(PropertyType::Number, "-"), Err(ValueError::NotNumber));
        assert_eq!(check(PropertyType::Date, "2024-02-29"), Ok(()));
        assert_eq!(
            check(PropertyType::Date, "2023-02-29"),
            Err(ValueError::NoSuchDay)
        );
        assert_eq!(
            check(PropertyType::Date, "2023-13-01"),
            Err(ValueError::NoSuchDay)
        );
        assert_eq!(
            check(PropertyType::Date, "2023-2-1"),
            Err(ValueError::DateForm)
        );
        assert_eq!(check(PropertyType::Datetime, "2020-08-21T10:30"), Ok(()));
        assert_eq!(
            check(PropertyType::Datetime, "2020-08-21T10:60"),
            Err(ValueError::NoSuchTime)
        );
        assert_eq!(
            check(PropertyType::Datetime, "2020-08-21 10:30"),
            Err(ValueError::DatetimeForm)
        );
        assert_eq!(check(PropertyType::Text, "anything"), Ok(()));
    }

    #[test]
    fn typing_keeps_to_the_form() {
        for partial in ["", "-", "+", "1", "1.", "-0.5", "1e", "1e-", "1e-3", ".5"] {
            assert!(accepts_typing(PropertyType::Number, partial), "{partial}");
        }
        for wrong in ["a", "1a", "1.2.3", "--1", "1e3e", "1 "] {
            assert!(!accepts_typing(PropertyType::Number, wrong), "{wrong}");
        }
        for partial in ["2", "2020-", "2020-08-2", "2020-08-21"] {
            assert!(accepts_typing(PropertyType::Date, partial), "{partial}");
        }
        for wrong in ["2020/", "2020-08-211", "x"] {
            assert!(!accepts_typing(PropertyType::Date, wrong), "{wrong}");
        }
        assert!(accepts_typing(PropertyType::Datetime, "2020-08-21T10:3"));
        assert!(!accepts_typing(PropertyType::Datetime, "2020-08-21 10"));
    }

    #[test]
    fn the_type_comes_from_focus_then_obsidian_then_defaults_then_the_value() {
        let date = value("2020-08-21");
        let k = resolve(
            "due",
            &date,
            Some(PropertyType::Text),
            Some(PropertyType::Datetime),
        );
        assert_eq!(
            (k.kind, k.origin, k.obsidian),
            (
                PropertyType::Text,
                TypeOrigin::Focus,
                Some(PropertyType::Datetime)
            )
        );
        let k = resolve("due", &date, None, Some(PropertyType::Datetime));
        assert_eq!(
            (k.kind, k.origin),
            (PropertyType::Datetime, TypeOrigin::Obsidian)
        );
        let k = resolve("cssclasses", &value("wide"), None, None);
        assert_eq!(
            (k.kind, k.origin),
            (PropertyType::Multitext, TypeOrigin::Default)
        );
        let k = resolve("due", &date, None, None);
        assert_eq!((k.kind, k.origin), (PropertyType::Date, TypeOrigin::Value));
        let k = resolve(
            "tags",
            &value("a"),
            Some(PropertyType::Text),
            Some(PropertyType::Text),
        );
        assert_eq!(k.kind, PropertyType::Tags);
        let k = resolve("topics", &value("[a]"), Some(PropertyType::Tags), None);
        assert_eq!(
            (k.kind, k.origin),
            (PropertyType::Multitext, TypeOrigin::Value)
        );
    }

    #[test]
    fn types_json_reads_known_types_only() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"types":{"aliases":"aliases","due":"date","done":"checkbox","x":"widget","n":3}}"#,
        )
        .unwrap();
        let types = read_types(&json);
        assert_eq!(types.len(), 3);
        assert_eq!(lookup(&types, "Due"), Some(PropertyType::Date));
        assert!(read_types(&serde_json::json!(["types"])).is_empty());
        assert!(read_types(&serde_json::json!({"types": "date"})).is_empty());
    }

    #[test]
    fn dates_count_seconds_from_1970() {
        let date = parse_date("2020-08-21T10:30:15", true).unwrap();
        assert_eq!(unix_seconds(&date), 1_598_005_815);
        assert_eq!(unix_seconds(&parse_date("1970-01-01", false).unwrap()), 0);
        assert_eq!(
            unix_seconds(&parse_date("1969-12-31", false).unwrap()),
            -86_400
        );
    }
}
