use std::path::{Component, Path};

use focus_core::{NoteId, Vault};

use crate::base::Base;
use crate::date::Date;
use crate::error::{BasesError, QueryError};
use crate::eval::{Cell, EvalError};
use crate::expr::Feature;
use crate::query::{Column, Row, Settings, run};
use crate::source::VaultFiles;
use crate::typed::Typed;
use crate::value::Value;

const MAX_BASE_BYTES: u64 = 1 << 20;

pub type Drawn = Result<Value, EvalError>;

#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub note: NoteId,
    pub cells: Vec<Drawn>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableSummary {
    pub name: String,
    pub built_in: bool,
    pub value: Drawn,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableGroup {
    pub key: Option<Value>,
    pub rows: Vec<TableRow>,
    pub summaries: Vec<Option<TableSummary>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub view: Option<String>,
    pub columns: Vec<Column>,
    pub groups: Vec<TableGroup>,
    pub matched: usize,
}

fn read(vault: &Vault, base: &Path) -> Result<String, BasesError> {
    let relative_ok =
        base.is_relative() && base.components().all(|c| matches!(c, Component::Normal(_)));
    if !relative_ok || base.extension().is_none_or(|e| e != "base") {
        return Err(BasesError::NotABase(base.to_path_buf()));
    }
    let io = |source| BasesError::Io {
        path: base.to_path_buf(),
        source,
    };
    let absolute = vault.absolute(base);
    let canonical = dunce::canonicalize(&absolute).map_err(io)?;
    let root = dunce::canonicalize(vault.root()).map_err(io)?;
    if !canonical.starts_with(&root) {
        return Err(BasesError::NotABase(base.to_path_buf()));
    }
    let size = std::fs::metadata(&canonical).map_err(io)?.len();
    if size > MAX_BASE_BYTES {
        return Err(BasesError::TooLarge(MAX_BASE_BYTES));
    }
    Ok(String::from_utf8_lossy(&std::fs::read(&canonical).map_err(io)?).into_owned())
}

pub fn load(vault: &Vault, base: &Path) -> Result<Base, BasesError> {
    Base::parse(&read(vault, base)?)
}

pub fn table(vault: &Vault, base: &Path, view: Option<&str>) -> Result<Table, BasesError> {
    load(vault, base)?.table(vault, view, None)
}

fn drawn(value: &Typed, now: Date) -> Result<Value, &'static str> {
    Ok(match value {
        Typed::Null => Value::Null,
        Typed::Bool(b) => Value::Bool(*b),
        Typed::Number(n) => Value::Number(*n),
        Typed::Date(date) if date.time && !date.relative => Value::Date(date.system()),
        Typed::List(items) => Value::List(
            items
                .iter()
                .map(|item| drawn(item, now))
                .collect::<Result<_, _>>()?,
        ),
        Typed::Object(entries) => Value::Map(
            entries
                .iter()
                .map(|(key, value)| Ok((key.clone(), drawn(value, now)?)))
                .collect::<Result<_, &'static str>>()?,
        ),
        Typed::Link(link) => Value::Text(link.wikilink()),
        Typed::Html(_) => return Err("html"),
        Typed::Icon(_) => return Err("icon"),
        Typed::Image(_) => return Err("image"),
        other => Value::Text(other.text(now)),
    })
}

fn without_variant(error: QueryError) -> BasesError {
    match error {
        QueryError::Base(error) => error,
        QueryError::UnknownFormula { expression, name } => BasesError::Unsupported {
            expression,
            feature: Feature::Formula,
            subject: format!("formula.{name}"),
        },
        other => BasesError::Invalid(other.to_string()),
    }
}

impl Base {
    pub fn table(
        &self,
        vault: &Vault,
        view: Option<&str>,
        this: Option<&Path>,
    ) -> Result<Table, BasesError> {
        self.table_at(vault, self.view_index(view)?, this)
    }

    pub fn table_at(
        &self,
        vault: &Vault,
        index: usize,
        this: Option<&Path>,
    ) -> Result<Table, BasesError> {
        let view = self.views.get(index).ok_or(BasesError::NoView)?;
        if view.kind != "table" {
            return Err(BasesError::UnsupportedViewType(view.kind.clone()));
        }
        let now = Date::now();
        let files = VaultFiles::new(vault, false);
        let this = this.map(crate::source::slashed);
        let settings = Settings {
            this: this.as_deref(),
            now,
            refuse_this: this.is_none(),
        };
        let result = run(self, view, &files, &settings).map_err(without_variant)?;
        let refused = |expression: &str, function: &str| BasesError::Unsupported {
            expression: expression.to_string(),
            feature: Feature::Function,
            subject: function.to_string(),
        };
        let cell = |cell: &Cell, expression: &str| -> Result<Drawn, BasesError> {
            match cell {
                Ok(value) => drawn(value, now)
                    .map(Ok)
                    .map_err(|function| refused(expression, function)),
                Err(error) => Ok(Err(error.clone())),
            }
        };
        let row = |row: &Row| -> Result<TableRow, BasesError> {
            let note = row
                .note
                .ok_or_else(|| BasesError::Invalid(format!("{} is not a note", row.path)))?;
            let cells = row
                .cells
                .iter()
                .zip(&result.columns)
                .map(|(value, column)| {
                    if column.link && value.is_ok() {
                        let name = row.path.rsplit('/').next().unwrap_or(&row.path);
                        let stem = match name.rsplit_once('.') {
                            Some((stem, _)) if !stem.is_empty() => stem,
                            _ => name,
                        };
                        Ok(Ok(Value::Text(stem.to_string())))
                    } else {
                        cell(value, &column.id)
                    }
                })
                .collect::<Result<_, _>>()?;
            Ok(TableRow { note, cells })
        };
        let summarized = result
            .columns
            .iter()
            .any(|column| result.summaries.iter().any(|s| s.property == column.id));
        let mut groups = Vec::with_capacity(result.groups.len());
        for group in &result.groups {
            let key = result
                .group_by
                .as_ref()
                .map(|property| {
                    drawn(&group.key, now).map_err(|function| refused(property, function))
                })
                .transpose()?;
            let summaries = if summarized {
                result
                    .columns
                    .iter()
                    .map(|column| {
                        result
                            .summaries
                            .iter()
                            .position(|summary| summary.property == column.id)
                            .map(|at| {
                                let summary = &result.summaries[at];
                                Ok(TableSummary {
                                    name: summary.name.clone(),
                                    built_in: summary.built_in,
                                    value: cell(&group.summaries[at], &summary.name)?,
                                })
                            })
                            .transpose()
                    })
                    .collect::<Result<_, BasesError>>()?
            } else {
                Vec::new()
            };
            groups.push(TableGroup {
                key,
                rows: group
                    .rows
                    .iter()
                    .map(|at| row(&result.rows[*at]))
                    .collect::<Result<_, _>>()?,
                summaries,
            });
        }
        Ok(Table {
            view: result.view,
            columns: result.columns,
            groups,
            matched: result.matched,
        })
    }
}
