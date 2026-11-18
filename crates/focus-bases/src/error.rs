use std::path::PathBuf;

use crate::eval::EvalError;
use crate::expr::Feature;

#[derive(Debug, thiserror::Error)]
pub enum BasesError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a base file inside the vault: {0}")]
    NotABase(PathBuf),
    #[error("base file larger than {0} bytes")]
    TooLarge(u64),
    #[error("invalid YAML: {0}")]
    Yaml(String),
    #[error("invalid base: {0}")]
    Invalid(String),
    #[error("no view named {0}")]
    UnknownView(String),
    #[error("the base has no view")]
    NoView,
    #[error("view type not supported: {0}")]
    UnsupportedViewType(String),
    #[error("not supported: {expression} ({} {subject})", .feature.name())]
    Unsupported {
        expression: String,
        feature: Feature,
        subject: String,
    },
    #[error("cannot read {expression}: {message}")]
    Syntax { expression: String, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Filter,
    Sort,
    Group,
}

impl Part {
    pub fn name(self) -> &'static str {
        match self {
            Part::Filter => "filter",
            Part::Sort => "sort",
            Part::Group => "group",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error(transparent)]
    Base(#[from] BasesError),
    #[error("no formula named {name} (in {expression})")]
    UnknownFormula { expression: String, name: String },
    #[error("formulas refer to each other: {}", .0.join(", "))]
    FormulaCycle(Vec<String>),
    #[error("no summary named {name} (for {property})")]
    UnknownSummary { property: String, name: String },
    #[error("{} {expression} fails on {path}: {error}", .part.name())]
    Row {
        part: Part,
        expression: String,
        path: String,
        error: EvalError,
    },
}
