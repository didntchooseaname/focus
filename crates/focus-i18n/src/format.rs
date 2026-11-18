use std::time::Duration;

use crate::state::{Arg, language, t, t_args};

fn separators() -> (char, char) {
    if language().split('-').next() == Some("fr") {
        ('\u{a0}', ',')
    } else {
        (',', '.')
    }
}

pub(crate) fn number_text(value: f64, max_fraction: usize, group: char, decimal: char) -> String {
    let fixed = format!("{:.*}", max_fraction, value.abs());
    let (int_part, frac_part) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let frac_part = frac_part.trim_end_matches('0');
    let mut out = String::with_capacity(fixed.len() + int_part.len() / 3 + 1);
    let negative = value < 0. && (int_part.bytes().any(|b| b != b'0') || !frac_part.is_empty());
    if negative {
        out.push('-');
    }
    for (i, digit) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            out.push(group);
        }
        out.push(digit);
    }
    if !frac_part.is_empty() {
        out.push(decimal);
        out.push_str(frac_part);
    }
    out
}

pub fn format_number(value: i64) -> String {
    let (group, decimal) = separators();
    number_text(value as f64, 0, group, decimal)
}

pub fn format_decimal(value: f64, fraction_digits: usize) -> String {
    let (group, decimal) = separators();
    let fixed = number_text(value, fraction_digits, group, decimal);
    if fraction_digits == 0 || fixed.contains(decimal) {
        let have = fixed.split(decimal).nth(1).map_or(0, |f| f.chars().count());
        if have >= fraction_digits {
            return fixed;
        }
        let mut padded = fixed;
        if have == 0 {
            padded.push(decimal);
        }
        padded.push_str(&"0".repeat(fraction_digits - have));
        return padded;
    }
    let mut padded = fixed;
    padded.push(decimal);
    padded.push_str(&"0".repeat(fraction_digits));
    padded
}

pub fn format_bytes(bytes: u64) -> String {
    const STEP: f64 = 1024.;
    let value = bytes as f64;
    let (scaled, key) = if value >= STEP * STEP * STEP {
        (value / (STEP * STEP * STEP), "fmt-bytes-gb")
    } else if value >= STEP * STEP {
        (value / (STEP * STEP), "fmt-bytes-mb")
    } else if value >= STEP {
        (value / STEP, "fmt-bytes-kb")
    } else {
        return t_args(
            "fmt-bytes-b",
            &[("value", Arg::Str(&format_number(bytes as i64)))],
        );
    };
    let text = if scaled < 10. {
        let (group, decimal) = separators();
        number_text(scaled, 1, group, decimal)
    } else {
        format_number(scaled.round() as i64)
    };
    t_args(key, &[("value", Arg::Str(&text))])
}

pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs == 0 {
        return t_args(
            "fmt-duration-millis",
            &[("ms", Arg::Int(duration.as_millis() as i64))],
        );
    }
    let int = |n: u64| Arg::Int(n as i64);
    if secs < 60 {
        t_args("fmt-duration-seconds", &[("seconds", int(secs))])
    } else if secs < 3600 {
        t_args(
            "fmt-duration-min-sec",
            &[("minutes", int(secs / 60)), ("seconds", int(secs % 60))],
        )
    } else if secs < 86_400 {
        t_args(
            "fmt-duration-hour-min",
            &[
                ("hours", int(secs / 3600)),
                ("minutes", int(secs % 3600 / 60)),
            ],
        )
    } else {
        t_args(
            "fmt-duration-day-hour",
            &[
                ("days", int(secs / 86_400)),
                ("hours", int(secs % 86_400 / 3600)),
            ],
        )
    }
}

#[cfg(unix)]
fn local_offset(unix_secs: i64) -> i64 {
    let time: libc::time_t = unix_secs as libc::time_t;
    // SAFETY: `tm` is filled by `localtime_r`, which is reentrant.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&time, &mut tm).is_null() {
            0
        } else {
            tm.tm_gmtoff
        }
    }
}

#[cfg(windows)]
fn local_offset(unix_secs: i64) -> i64 {
    let time: libc::time_t = unix_secs as libc::time_t;
    // SAFETY: both structures are written by the CRT, which checks its arguments.
    let (local, utc) = unsafe {
        let mut local: libc::tm = std::mem::zeroed();
        let mut utc: libc::tm = std::mem::zeroed();
        if libc::localtime_s(&mut local, &time) != 0 || libc::gmtime_s(&mut utc, &time) != 0 {
            return 0;
        }
        (local, utc)
    };
    let days = match local.tm_year.cmp(&utc.tm_year) {
        std::cmp::Ordering::Equal => i64::from(local.tm_yday - utc.tm_yday),
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Less => -1,
    };
    days * 86_400
        + i64::from(local.tm_hour - utc.tm_hour) * 3600
        + i64::from(local.tm_min - utc.tm_min) * 60
        + i64::from(local.tm_sec - utc.tm_sec)
}

fn civil(unix_secs: i64, offset: i64) -> (i64, u32, u32, u32, u32) {
    let local = unix_secs + offset;
    let days = local.div_euclid(86_400);
    let minutes = local.rem_euclid(86_400) / 60;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        month,
        day,
        (minutes / 60) as u32,
        (minutes % 60) as u32,
    )
}

pub fn format_date(unix_secs: i64) -> String {
    format_date_at(unix_secs, local_offset(unix_secs))
}

pub fn format_date_at(unix_secs: i64, offset_secs: i64) -> String {
    let (year, month, day, _, _) = civil(unix_secs, offset_secs);
    let month_name = t(&format!("fmt-month-{month}"));
    t_args(
        "fmt-date",
        &[
            ("year", Arg::Str(&year.to_string())),
            ("month", Arg::Str(&month_name)),
            ("day", Arg::Str(&day.to_string())),
        ],
    )
}

pub fn format_datetime(unix_secs: i64) -> String {
    format_datetime_at(unix_secs, local_offset(unix_secs))
}

pub fn format_datetime_at(unix_secs: i64, offset_secs: i64) -> String {
    let (_, _, _, hour, minute) = civil(unix_secs, offset_secs);
    let date = format_date_at(unix_secs, offset_secs);
    let time = t_args(
        "fmt-time",
        &[
            ("hour", Arg::Str(&format!("{hour:02}"))),
            ("minute", Arg::Str(&format!("{minute:02}"))),
        ],
    );
    t_args(
        "fmt-datetime",
        &[("date", Arg::Str(&date)), ("time", Arg::Str(&time))],
    )
}
