use std::sync::Mutex;
use std::time::Duration;

use focus_i18n::{
    Direction, format_bytes, format_date_at, format_datetime_at, format_decimal, format_duration,
    format_number, text_direction,
};

static SERIAL: Mutex<()> = Mutex::new(());

fn in_language(code: &str) -> std::sync::MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    focus_i18n::init(None);
    focus_i18n::set_language(code).unwrap();
    guard
}

const INSTANT: i64 = 1_794_528_180;

#[test]
fn numbers_use_the_separators_of_the_language() {
    let _guard = in_language("en");
    assert_eq!(format_number(2545), "2,545");
    assert_eq!(format_number(-1_234_567), "-1,234,567");
    assert_eq!(format_number(999), "999");
    assert_eq!(format_decimal(1.5, 2), "1.50");
    assert_eq!(format_decimal(1234.5678, 1), "1,234.6");
    focus_i18n::set_language("fr").unwrap();
    assert_eq!(format_number(2545), "2\u{a0}545");
    assert_eq!(format_number(1_000_000), "1\u{a0}000\u{a0}000");
    assert_eq!(format_decimal(1.5, 2), "1,50");
    focus_i18n::set_language("ar").unwrap();
    assert_eq!(format_number(2545), "2,545");
    assert_eq!(format_decimal(0.26, 1), "0.3");
    focus_i18n::set_language("zh-CN").unwrap();
    assert_eq!(format_number(2545), "2,545");
    focus_i18n::set_language("en").unwrap();
}

#[test]
fn sizes_follow_the_unit_names_of_the_language() {
    let _guard = in_language("en");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(84 * 1024 * 1024), "84 MB");
    assert_eq!(format_bytes(1_258_291), "1.2 MB");
    assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3 GB");
    focus_i18n::set_language("fr").unwrap();
    assert_eq!(format_bytes(512), "512 o");
    assert_eq!(format_bytes(84 * 1024 * 1024), "84 Mo");
    assert_eq!(format_bytes(1_258_291), "1,2 Mo");
    assert_eq!(format_bytes(12 * 1024), "12 Ko");
    focus_i18n::set_language("ar").unwrap();
    assert!(format_bytes(84 * 1024 * 1024).starts_with("84 "));
    focus_i18n::set_language("zh-CN").unwrap();
    assert!(format_bytes(84 * 1024 * 1024).contains("84"));
    focus_i18n::set_language("en").unwrap();
}

#[test]
fn durations_keep_two_units() {
    let _guard = in_language("en");
    assert_eq!(format_duration(Duration::from_millis(850)), "850 ms");
    assert_eq!(format_duration(Duration::from_secs(42)), "42 s");
    assert_eq!(format_duration(Duration::from_secs(185)), "3 min 5 s");
    assert_eq!(format_duration(Duration::from_secs(7800)), "2 h 10 min");
    assert_eq!(
        format_duration(Duration::from_secs(3 * 86_400 + 4 * 3600 + 59)),
        "3 d 4 h"
    );
    focus_i18n::set_language("fr").unwrap();
    assert_eq!(
        format_duration(Duration::from_secs(3 * 86_400 + 4 * 3600)),
        "3 j 4 h"
    );
    focus_i18n::set_language("en").unwrap();
}

#[test]
fn dates_use_the_pattern_of_the_language_and_the_given_offset() {
    let _guard = in_language("en");
    assert_eq!(format_date_at(INSTANT, 0), "Nov 13, 2026");
    assert_eq!(format_datetime_at(INSTANT, 0), "Nov 13, 2026, 00:03");
    assert_eq!(format_date_at(INSTANT, -3600), "Nov 12, 2026");
    assert_eq!(format_datetime_at(INSTANT, 3600), "Nov 13, 2026, 01:03");
    focus_i18n::set_language("fr").unwrap();
    assert_eq!(format_datetime_at(INSTANT, 0), "13 nov. 2026 à 00:03");
    focus_i18n::set_language("zh-CN").unwrap();
    assert_eq!(format_date_at(INSTANT, 0), "2026年11月13日");
    focus_i18n::set_language("ar").unwrap();
    let arabic = format_date_at(INSTANT, 0);
    assert!(arabic.contains("2026") && arabic.contains("13"), "{arabic}");
    focus_i18n::set_language("en").unwrap();
}

#[test]
fn text_direction_follows_the_first_strong_character() {
    assert_eq!(text_direction("Hello"), Direction::Ltr);
    assert_eq!(text_direction("مرحبا بالعالم"), Direction::Rtl);
    assert_eq!(text_direction("  12 - مرحبا"), Direction::Rtl);
    assert_eq!(text_direction("`code` مرحبا"), Direction::Ltr);
    assert_eq!(text_direction("שלום"), Direction::Rtl);
    assert_eq!(text_direction("你好"), Direction::Ltr);
    assert_eq!(text_direction("123 !?"), Direction::Ltr);
    assert_eq!(text_direction(""), Direction::Ltr);
}
