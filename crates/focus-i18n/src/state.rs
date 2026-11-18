use std::path::Path;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

use fluent_bundle::{FluentArgs, FluentValue};
use parking_lot::RwLock;

use crate::catalog::{self, Diagnostic, FALLBACK, Locale};
use crate::{Direction, Error, LanguageInfo};

struct State {
    locales: Vec<Locale>,
    current: usize,
}

static STATE: LazyLock<RwLock<State>> = LazyLock::new(|| {
    let (locales, _) = catalog::load(None);
    let current = position(&locales, FALLBACK).unwrap_or(0);
    RwLock::new(State { locales, current })
});
static GENERATION: AtomicU64 = AtomicU64::new(0);
static MISSING: AtomicU64 = AtomicU64::new(0);

fn position(locales: &[Locale], code: &str) -> Option<usize> {
    locales.iter().position(|l| l.info.code == code)
}

fn resolve(locales: &[Locale], code: &str) -> Option<usize> {
    let cleaned = code
        .split(['.', '@'])
        .next()
        .unwrap_or(code)
        .replace('_', "-");
    if cleaned.is_empty() {
        return None;
    }
    let lower = cleaned.to_lowercase();
    if let Some(index) = locales
        .iter()
        .position(|l| l.info.code.to_lowercase() == lower)
    {
        return Some(index);
    }
    let primary = lower.split('-').next().unwrap_or(&lower);
    locales
        .iter()
        .position(|l| l.info.code.to_lowercase().split('-').next() == Some(primary))
}

pub fn init(user_dir: Option<&Path>) -> Vec<Diagnostic> {
    let (locales, diagnostics) = catalog::load(user_dir);
    let mut state = STATE.write();
    let previous = state
        .locales
        .get(state.current)
        .map(|l| l.info.code.clone());
    state.current = previous
        .and_then(|code| position(&locales, &code))
        .or_else(|| position(&locales, FALLBACK))
        .unwrap_or(0);
    state.locales = locales;
    GENERATION.fetch_add(1, Ordering::Relaxed);
    diagnostics
}

pub fn set_language(code: &str) -> Result<(), Error> {
    let mut state = STATE.write();
    let index =
        resolve(&state.locales, code).ok_or_else(|| Error::UnknownLanguage(code.to_string()))?;
    if index != state.current {
        state.current = index;
        GENERATION.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

pub fn language() -> String {
    let state = STATE.read();
    state.locales[state.current].info.code.clone()
}

pub fn direction() -> Direction {
    let state = STATE.read();
    state.locales[state.current].info.direction
}

pub fn available() -> Vec<LanguageInfo> {
    STATE
        .read()
        .locales
        .iter()
        .map(|l| l.info.clone())
        .collect()
}

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

pub fn detect_system() -> String {
    let state = STATE.read();
    for variable in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        let Ok(value) = std::env::var(variable) else {
            continue;
        };
        if value.is_empty() || value == "C" || value == "POSIX" {
            continue;
        }
        if let Some(index) = resolve(&state.locales, &value) {
            return state.locales[index].info.code.clone();
        }
        break;
    }
    FALLBACK.to_string()
}

pub enum Arg<'a> {
    Str(&'a str),
    Int(i64),
    Float(f64),
}

impl<'a> From<&'a str> for Arg<'a> {
    fn from(value: &'a str) -> Self {
        Arg::Str(value)
    }
}
impl<'a> From<&'a String> for Arg<'a> {
    fn from(value: &'a String) -> Self {
        Arg::Str(value)
    }
}
impl From<i64> for Arg<'_> {
    fn from(value: i64) -> Self {
        Arg::Int(value)
    }
}
impl From<i32> for Arg<'_> {
    fn from(value: i32) -> Self {
        Arg::Int(value.into())
    }
}
impl From<u32> for Arg<'_> {
    fn from(value: u32) -> Self {
        Arg::Int(value.into())
    }
}
impl From<u64> for Arg<'_> {
    fn from(value: u64) -> Self {
        Arg::Int(value as i64)
    }
}
impl From<usize> for Arg<'_> {
    fn from(value: usize) -> Self {
        Arg::Int(value as i64)
    }
}
impl From<f64> for Arg<'_> {
    fn from(value: f64) -> Self {
        Arg::Float(value)
    }
}

pub fn t(key: &str) -> String {
    lookup(key, None)
}

pub fn t_args(key: &str, args: &[(&str, Arg)]) -> String {
    let mut fluent = FluentArgs::with_capacity(args.len());
    for (name, arg) in args {
        let value = match arg {
            Arg::Str(s) => FluentValue::from(*s),
            Arg::Int(n) => FluentValue::from(*n),
            Arg::Float(f) => FluentValue::from(*f),
        };
        fluent.set(*name, value);
    }
    lookup(key, Some(&fluent))
}

fn lookup(key: &str, args: Option<&FluentArgs>) -> String {
    let state = STATE.read();
    let current = &state.locales[state.current];
    if let Some(text) = format_in(current, key, args) {
        return text;
    }
    if current.info.code != FALLBACK
        && let Some(english) = state.locales.iter().find(|l| l.info.code == FALLBACK)
        && let Some(text) = format_in(english, key, args)
    {
        return text;
    }
    MISSING.fetch_add(1, Ordering::Relaxed);
    key.to_string()
}

fn format_in(locale: &Locale, key: &str, args: Option<&FluentArgs>) -> Option<String> {
    let pattern = locale.bundle.get_message(key)?.value()?;
    let mut errors = Vec::new();
    Some(
        locale
            .bundle
            .format_pattern(pattern, args, &mut errors)
            .into_owned(),
    )
}

pub fn template(name: &str) -> Option<String> {
    let state = STATE.read();
    state.locales[state.current]
        .templates
        .get(name)
        .or_else(|| {
            state
                .locales
                .iter()
                .find(|l| l.info.code == FALLBACK)
                .and_then(|l| l.templates.get(name))
        })
        .cloned()
}

pub fn missing_count() -> u64 {
    MISSING.load(Ordering::Relaxed)
}

pub fn reset_missing_count() {
    MISSING.store(0, Ordering::Relaxed);
}
