pub mod audit;
mod bidi;
mod catalog;
mod format;
mod keys;
mod state;

pub use bidi::text_direction;
pub use catalog::Diagnostic;
pub use format::{
    format_bytes, format_date, format_date_at, format_datetime, format_datetime_at, format_decimal,
    format_duration, format_number,
};
pub use keys::keys_text;
pub use state::{
    Arg, available, detect_system, direction, generation, init, language, missing_count,
    reset_missing_count, set_language, t, t_args, template,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageInfo {
    pub code: String,
    pub native_name: String,
    pub direction: Direction,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown language: {0}")]
    UnknownLanguage(String),
}
