use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{
    Datelike, Days, FixedOffset, Local, LocalResult, Months, NaiveDate, NaiveDateTime, NaiveTime,
    TimeDelta, TimeZone, Timelike,
};

use crate::eval::EvalError;

const DAY_MILLIS: f64 = 86_400_000.0;
const MONTH_DAYS: f64 = 30.436_875;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Date {
    pub millis: i64,
    pub time: bool,
    pub relative: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Duration {
    pub months: f64,
    pub days: f64,
    pub millis: f64,
}

fn local_of(millis: i64) -> Option<NaiveDateTime> {
    match Local.timestamp_millis_opt(millis) {
        LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => Some(at.naive_local()),
        LocalResult::None => None,
    }
}

fn instant_of(local: NaiveDateTime) -> Option<i64> {
    match Local.from_local_datetime(&local) {
        LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => Some(at.timestamp_millis()),
        LocalResult::None => Local
            .from_local_datetime(&local.checked_add_signed(TimeDelta::try_hours(1)?)?)
            .earliest()
            .map(|at| at.timestamp_millis()),
    }
}

fn digits(text: &str, count: usize) -> Option<(u32, &str)> {
    let head = text.get(..count)?;
    if !head.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((head.parse().ok()?, &text[count..]))
}

fn time_of_day(text: &str) -> Option<(NaiveTime, &str)> {
    let (hour, rest) = digits(text, 2)?;
    let (minute, mut rest) = digits(rest.strip_prefix(':')?, 2)?;
    let mut second = 0;
    let mut milli = 0;
    if let Some(after) = rest.strip_prefix(':') {
        (second, rest) = digits(after, 2)?;
        if let Some(after) = rest.strip_prefix('.') {
            let end = after
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after.len());
            if end == 0 {
                return None;
            }
            let fraction = format!("{:0<3}", &after[..end.min(3)]);
            milli = fraction.parse().ok()?;
            rest = &after[end..];
        }
    }
    Some((
        NaiveTime::from_hms_milli_opt(hour, minute, second, milli)?,
        rest,
    ))
}

fn offset_of(text: &str) -> Option<FixedOffset> {
    if text == "Z" || text == "z" {
        return FixedOffset::east_opt(0);
    }
    let sign = match text.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (hours, rest) = digits(&text[1..], 2)?;
    let minutes = match rest.strip_prefix(':').unwrap_or(rest) {
        "" => 0,
        rest => match digits(rest, 2)? {
            (minutes, "") => minutes,
            _ => return None,
        },
    };
    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60) as i32)
}

impl Date {
    pub fn from_millis(millis: i64, time: bool) -> Option<Date> {
        local_of(millis).map(|_| Date {
            millis,
            time,
            relative: false,
        })
    }

    pub fn now() -> Date {
        Date::from_system(SystemTime::now()).unwrap_or(Date {
            millis: 0,
            time: true,
            relative: false,
        })
    }

    pub fn from_system(time: SystemTime) -> Option<Date> {
        let millis = match time.duration_since(UNIX_EPOCH) {
            Ok(after) => i64::try_from(after.as_millis()).ok()?,
            Err(before) => -i64::try_from(before.duration().as_millis()).ok()?,
        };
        Date::from_millis(millis, true)
    }

    pub fn system(self) -> SystemTime {
        let span = std::time::Duration::from_millis(self.millis.unsigned_abs());
        if self.millis >= 0 {
            UNIX_EPOCH + span
        } else {
            UNIX_EPOCH - span
        }
    }

    pub(crate) fn local(self) -> NaiveDateTime {
        local_of(self.millis).unwrap_or_default()
    }

    fn at_local(local: NaiveDateTime, time: bool) -> Option<Date> {
        Date::from_millis(instant_of(local)?, time)
    }

    pub fn parse(text: &str) -> Option<Date> {
        let text = text.trim();
        if (text.len() == 8 || text.len() == 12)
            && let Some((year, rest)) = digits(text, 4)
        {
            let (month, rest) = digits(rest, 2)?;
            let (day, rest) = digits(rest, 2)?;
            let date = NaiveDate::from_ymd_opt(year as i32, month, day)?;
            if rest.is_empty() {
                return Date::at_local(date.and_time(NaiveTime::MIN), false);
            }
            let (hour, rest) = digits(rest, 2)?;
            let (minute, _) = digits(rest, 2)?;
            return Date::at_local(date.and_hms_opt(hour, minute, 0)?, true);
        }
        let (year, rest) = digits(text, 4)?;
        let (month, rest) = digits(rest.strip_prefix('-')?, 2)?;
        let (day, rest) = digits(rest.strip_prefix('-')?, 2)?;
        let date = NaiveDate::from_ymd_opt(year as i32, month, day)?;
        if rest.is_empty() {
            return Date::at_local(date.and_time(NaiveTime::MIN), false);
        }
        let rest = rest
            .strip_prefix('T')
            .or_else(|| rest.strip_prefix('t'))
            .or_else(|| rest.strip_prefix(' '))?;
        let (time, rest) = time_of_day(rest)?;
        let local = date.and_time(time);
        if rest.is_empty() {
            return Date::at_local(local, true);
        }
        let offset = offset_of(rest.trim_start())?;
        let at = offset.from_local_datetime(&local).single()?;
        Date::from_millis(at.timestamp_millis(), true)
    }

    pub(crate) fn parse_property(text: &str) -> Option<Date> {
        let bytes = text.as_bytes();
        if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        Date::parse(text)
    }

    pub fn date_only(self) -> Option<Date> {
        Date::at_local(self.local().date().and_time(NaiveTime::MIN), false)
    }

    pub(crate) fn field(self, name: &str) -> Option<f64> {
        let local = self.local();
        Some(match name {
            "year" => local.year() as f64,
            "month" => local.month() as f64,
            "day" => local.day() as f64,
            "hour" => local.hour() as f64,
            "minute" => local.minute() as f64,
            "second" => local.second() as f64,
            "millisecond" => local.and_utc().timestamp_subsec_millis() as f64,
            _ => return None,
        })
    }

    pub(crate) fn since(self, other: Date) -> Duration {
        if !self.time && !other.time {
            let days = self
                .local()
                .date()
                .signed_duration_since(other.local().date())
                .num_days();
            return Duration {
                months: 0.0,
                days: days as f64,
                millis: 0.0,
            };
        }
        Duration {
            months: 0.0,
            days: 0.0,
            millis: (self.millis - other.millis) as f64,
        }
    }

    pub(crate) fn shifted(self, by: Duration, sign: f64) -> Option<Date> {
        let months = by.months * sign;
        let whole_months = months.trunc();
        let days = by.days * sign + (months - whole_months) * MONTH_DAYS;
        let whole_days = days.trunc();
        let millis = (by.millis * sign + (days - whole_days) * DAY_MILLIS).round();
        if !(whole_months.abs() < 1e7 && whole_days.abs() < 1e9 && millis.abs() < 1e17) {
            return None;
        }
        let mut local = self.local();
        if whole_months > 0.0 {
            local = local.checked_add_months(Months::new(whole_months as u32))?;
        } else if whole_months < 0.0 {
            local = local.checked_sub_months(Months::new(-whole_months as u32))?;
        }
        if whole_days > 0.0 {
            local = local.checked_add_days(Days::new(whole_days as u64))?;
        } else if whole_days < 0.0 {
            local = local.checked_sub_days(Days::new(-whole_days as u64))?;
        }
        let instant = instant_of(local)?.checked_add(millis as i64)?;
        Date::from_millis(instant, self.time || millis != 0.0)
    }

    pub(crate) fn same(self, other: Date) -> bool {
        self.order(other).is_eq()
    }

    pub(crate) fn order(self, other: Date) -> std::cmp::Ordering {
        if self.time && other.time {
            self.millis.cmp(&other.millis)
        } else {
            self.local().date().cmp(&other.local().date())
        }
    }

    pub fn text(self, now: Date) -> String {
        if self.relative {
            return self.relative_text(now);
        }
        let local = self.local();
        if self.time {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                local.year(),
                local.month(),
                local.day(),
                local.hour(),
                local.minute(),
                local.second()
            )
        } else {
            format!(
                "{:04}-{:02}-{:02}",
                local.year(),
                local.month(),
                local.day()
            )
        }
    }

    pub(crate) fn time_text(self) -> String {
        let local = self.local();
        format!(
            "{:02}:{:02}:{:02}",
            local.hour(),
            local.minute(),
            local.second()
        )
    }

    pub fn relative_text(self, now: Date) -> String {
        let span = (self.millis - now.millis) as f64;
        let seconds = (span.abs() / 1000.0).round();
        let minutes = (span.abs() / 60_000.0).round();
        let hours = (span.abs() / 3_600_000.0).round();
        let days = (span.abs() / DAY_MILLIS).round();
        let months = (span.abs() / (DAY_MILLIS * MONTH_DAYS)).round();
        let years = (span.abs() / (DAY_MILLIS * 365.2425)).round();
        let distance = if seconds < 45.0 {
            "a few seconds".to_string()
        } else if seconds < 90.0 {
            "a minute".to_string()
        } else if minutes < 45.0 {
            format!("{minutes} minutes")
        } else if minutes < 90.0 {
            "an hour".to_string()
        } else if hours < 22.0 {
            format!("{hours} hours")
        } else if hours < 36.0 {
            "a day".to_string()
        } else if days < 26.0 {
            format!("{days} days")
        } else if days < 46.0 {
            "a month".to_string()
        } else if days < 320.0 {
            format!("{months} months")
        } else if days < 548.0 {
            "a year".to_string()
        } else {
            format!("{years} years")
        };
        if span > 0.0 {
            format!("in {distance}")
        } else {
            format!("{distance} ago")
        }
    }

    pub(crate) fn format(self, pattern: &str) -> Result<String, EvalError> {
        const TOKENS: [&str; 43] = [
            "YYYY", "YY", "Y", "Qo", "Q", "MMMM", "MMM", "MM", "Mo", "M", "DDDD", "DDDo", "DDD",
            "DD", "Do", "D", "dddd", "ddd", "dd", "do", "d", "E", "HH", "H", "hh", "h", "kk", "k",
            "mm", "m", "ss", "s", "A", "a", "ZZ", "Z", "X", "x", "WW", "Wo", "W", "GGGG", "GG",
        ];
        let local = self.local();
        let offset = match Local.timestamp_millis_opt(self.millis) {
            LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => {
                at.offset().local_minus_utc()
            }
            LocalResult::None => 0,
        };
        let hour12 = match local.hour() % 12 {
            0 => 12,
            hour => hour,
        };
        let weekday = local.weekday().num_days_from_sunday() as usize;
        let milli = local.and_utc().timestamp_subsec_millis();
        let mut out = String::new();
        let mut rest = pattern;
        while let Some(c) = rest.chars().next() {
            if c == '['
                && let Some(end) = rest.find(']')
            {
                out.push_str(&rest[1..end]);
                rest = &rest[end + 1..];
                continue;
            }
            if c == 'S' {
                let run = rest.bytes().take_while(|b| *b == b'S').count();
                let fraction = format!("{milli:03}{}", "0".repeat(run.saturating_sub(3)));
                out.push_str(&fraction[..run.min(fraction.len())]);
                rest = &rest[run..];
                continue;
            }
            let Some(token) = TOKENS.iter().find(|t| rest.starts_with(**t)) else {
                if matches!(c, 'L' | 'l' | 'w' | 'g' | 'e' | 'z' | 'N' | 'y') {
                    let run = rest.chars().take_while(|x| *x == c).count();
                    return Err(EvalError::Format(rest[..run].to_string()));
                }
                out.push(c);
                rest = &rest[c.len_utf8()..];
                continue;
            };
            rest = &rest[token.len()..];
            match *token {
                "YYYY" => out.push_str(&format!("{:04}", local.year())),
                "YY" => out.push_str(&format!("{:02}", local.year().rem_euclid(100))),
                "Y" => out.push_str(&local.year().to_string()),
                "Q" => out.push_str(&local.month().div_ceil(3).to_string()),
                "Qo" => out.push_str(&ordinal(local.month().div_ceil(3))),
                "MMMM" => out.push_str(MONTHS[local.month0() as usize]),
                "MMM" => out.push_str(&MONTHS[local.month0() as usize][..3]),
                "MM" => out.push_str(&format!("{:02}", local.month())),
                "Mo" => out.push_str(&ordinal(local.month())),
                "M" => out.push_str(&local.month().to_string()),
                "DDDD" => out.push_str(&format!("{:03}", local.ordinal())),
                "DDDo" => out.push_str(&ordinal(local.ordinal())),
                "DDD" => out.push_str(&local.ordinal().to_string()),
                "DD" => out.push_str(&format!("{:02}", local.day())),
                "Do" => out.push_str(&ordinal(local.day())),
                "D" => out.push_str(&local.day().to_string()),
                "dddd" => out.push_str(WEEKDAYS[weekday]),
                "ddd" => out.push_str(&WEEKDAYS[weekday][..3]),
                "dd" => out.push_str(&WEEKDAYS[weekday][..2]),
                "do" => out.push_str(&ordinal(weekday as u32)),
                "d" => out.push_str(&weekday.to_string()),
                "E" => out.push_str(&local.weekday().number_from_monday().to_string()),
                "HH" => out.push_str(&format!("{:02}", local.hour())),
                "H" => out.push_str(&local.hour().to_string()),
                "hh" => out.push_str(&format!("{hour12:02}")),
                "h" => out.push_str(&hour12.to_string()),
                "kk" => out.push_str(&format!("{:02}", (local.hour() + 23) % 24 + 1)),
                "k" => out.push_str(&((local.hour() + 23) % 24 + 1).to_string()),
                "mm" => out.push_str(&format!("{:02}", local.minute())),
                "m" => out.push_str(&local.minute().to_string()),
                "ss" => out.push_str(&format!("{:02}", local.second())),
                "s" => out.push_str(&local.second().to_string()),
                "A" => out.push_str(if local.hour() < 12 { "AM" } else { "PM" }),
                "a" => out.push_str(if local.hour() < 12 { "am" } else { "pm" }),
                "Z" | "ZZ" => {
                    let sign = if offset < 0 { '-' } else { '+' };
                    let minutes = offset.abs() / 60;
                    let colon = if *token == "Z" { ":" } else { "" };
                    out.push_str(&format!(
                        "{sign}{:02}{colon}{:02}",
                        minutes / 60,
                        minutes % 60
                    ));
                }
                "X" => out.push_str(&self.millis.div_euclid(1000).to_string()),
                "x" => out.push_str(&self.millis.to_string()),
                "WW" => out.push_str(&format!("{:02}", local.iso_week().week())),
                "Wo" => out.push_str(&ordinal(local.iso_week().week())),
                "W" => out.push_str(&local.iso_week().week().to_string()),
                "GGGG" => out.push_str(&format!("{:04}", local.iso_week().year())),
                "GG" => out.push_str(&format!("{:02}", local.iso_week().year().rem_euclid(100))),
                _ => out.push_str(token),
            }
        }
        Ok(out)
    }
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

impl Duration {
    pub(crate) fn of_millis(millis: f64) -> Duration {
        Duration {
            months: 0.0,
            days: 0.0,
            millis,
        }
    }

    pub fn parse(text: &str) -> Option<Duration> {
        let mut rest = text.trim();
        let sign = match rest.strip_prefix('-') {
            Some(after) => {
                rest = after.trim_start();
                -1.0
            }
            None => 1.0,
        };
        let mut total = Duration::of_millis(0.0);
        let mut pairs = 0;
        while !rest.is_empty() {
            let end = rest
                .find(|c: char| !c.is_ascii_digit() && c != '.')
                .unwrap_or(rest.len());
            let amount: f64 = rest[..end].parse().ok()?;
            rest = rest[end..].trim_start();
            let end = rest
                .find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(rest.len());
            let amount = amount * sign;
            match &rest[..end] {
                "y" | "year" | "years" => total.months += amount * 12.0,
                "M" | "month" | "months" => total.months += amount,
                "w" | "week" | "weeks" => total.days += amount * 7.0,
                "d" | "day" | "days" => total.days += amount,
                "h" | "hour" | "hours" => total.millis += amount * 3_600_000.0,
                "m" | "minute" | "minutes" => total.millis += amount * 60_000.0,
                "s" | "second" | "seconds" => total.millis += amount * 1000.0,
                _ => return None,
            }
            rest = rest[end..].trim_start_matches([' ', ',']);
            pairs += 1;
        }
        (pairs > 0).then_some(total)
    }

    pub fn total_millis(self) -> f64 {
        (self.months * MONTH_DAYS + self.days) * DAY_MILLIS + self.millis
    }

    pub(crate) fn plus(self, other: Duration, sign: f64) -> Duration {
        Duration {
            months: self.months + other.months * sign,
            days: self.days + other.days * sign,
            millis: self.millis + other.millis * sign,
        }
    }

    pub(crate) fn scaled(self, factor: f64) -> Duration {
        Duration {
            months: self.months * factor,
            days: self.days * factor,
            millis: self.millis * factor,
        }
    }

    pub fn text(self) -> String {
        let rest = self.days * DAY_MILLIS + self.millis;
        if self.months * rest < 0.0 {
            return Duration::of_millis(self.total_millis()).text();
        }
        let negative = self.months < 0.0 || rest < 0.0;
        let months = self.months.abs();
        let mut millis = rest.abs().round();
        let mut parts = Vec::new();
        let years = (months / 12.0).trunc();
        let months = months - years * 12.0;
        for (amount, unit) in [(years, "y"), (months, "M")] {
            if amount != 0.0 {
                parts.push(format!("{amount}{unit}"));
            }
        }
        for (size, unit) in [
            (DAY_MILLIS, "d"),
            (3_600_000.0, "h"),
            (60_000.0, "m"),
            (1000.0, "s"),
        ] {
            let amount = (millis / size).trunc();
            millis -= amount * size;
            if amount != 0.0 {
                parts.push(format!("{amount}{unit}"));
            }
        }
        if millis != 0.0 {
            parts.push(format!("{millis}ms"));
        }
        if parts.is_empty() {
            return "0s".to_string();
        }
        format!("{}{}", if negative { "-" } else { "" }, parts.join(" "))
    }
}
