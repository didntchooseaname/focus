use serde_yaml::Value as Yaml;

use crate::error::BasesError;
use crate::expr::Feature;
use crate::typed::Typed;

const MAX_FILTER_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    Statement(String),
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Vec<Filter>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SortKey {
    pub property: String,
    pub direction: Direction,
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub kind: String,
    pub name: Option<String>,
    pub filter: Option<Filter>,
    pub order: Vec<String>,
    pub sort: Vec<SortKey>,
    pub limit: Option<usize>,
    pub group_by: Option<SortKey>,
    pub group_order: Option<Vec<Typed>>,
    pub summaries: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Base {
    pub filter: Option<Filter>,
    pub display_names: Vec<(String, String)>,
    pub formulas: Vec<(String, String)>,
    pub summaries: Vec<(String, String)>,
    pub views: Vec<View>,
}

fn invalid(what: impl Into<String>) -> BasesError {
    BasesError::Invalid(what.into())
}

fn key<'a>(map: &'a serde_yaml::Mapping, name: &str) -> Option<&'a Yaml> {
    map.get(Yaml::String(name.to_string()))
}

fn expressions(value: Option<&Yaml>, section: &str) -> Result<Vec<(String, String)>, BasesError> {
    let map = match value {
        None | Some(Yaml::Null) => return Ok(Vec::new()),
        Some(Yaml::Mapping(map)) => map,
        Some(_) => return Err(invalid(format!("{section} is a mapping"))),
    };
    map.iter()
        .map(|(name, source)| {
            let name = name
                .as_str()
                .ok_or_else(|| invalid(format!("a name in {section} is text")))?;
            let source = match source {
                Yaml::String(source) => source.clone(),
                Yaml::Number(n) => n.to_string(),
                Yaml::Bool(b) => b.to_string(),
                _ => return Err(invalid(format!("{section}.{name} is text"))),
            };
            Ok((name.to_string(), source))
        })
        .collect()
}

fn plain(value: &Yaml) -> Result<Typed, BasesError> {
    Ok(match value {
        Yaml::Null => Typed::Null,
        Yaml::Bool(b) => Typed::Bool(*b),
        Yaml::Number(n) => Typed::Number(
            n.as_f64()
                .ok_or_else(|| invalid("groupOrder holds a number that is not finite"))?,
        ),
        Yaml::String(text) => Typed::Text(text.clone()),
        Yaml::Sequence(items) => Typed::List(items.iter().map(plain).collect::<Result<_, _>>()?),
        Yaml::Mapping(_) | Yaml::Tagged(_) => {
            return Err(invalid("groupOrder lists plain values"));
        }
    })
}

fn filter(value: &Yaml, depth: usize) -> Result<Filter, BasesError> {
    if depth > MAX_FILTER_DEPTH {
        return Err(invalid("filters are nested too deeply"));
    }
    match value {
        Yaml::String(statement) => Ok(Filter::Statement(statement.clone())),
        Yaml::Mapping(map) => {
            let mut entries = map.iter();
            let (Some((name, list)), None) = (entries.next(), entries.next()) else {
                return Err(invalid("a filter object has exactly one of and, or, not"));
            };
            let Yaml::Sequence(items) = list else {
                return Err(invalid("and, or and not hold a list"));
            };
            let children = items
                .iter()
                .map(|item| filter(item, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            match name.as_str() {
                Some("and") => Ok(Filter::And(children)),
                Some("or") => Ok(Filter::Or(children)),
                Some("not") => Ok(Filter::Not(children)),
                _ => Err(invalid("a filter object has exactly one of and, or, not")),
            }
        }
        _ => Err(invalid("a filter is a statement or an and, or, not object")),
    }
}

fn view(value: &Yaml) -> Result<View, BasesError> {
    let Yaml::Mapping(map) = value else {
        return Err(invalid("a view is a mapping"));
    };
    let text = |name: &str| match key(map, name) {
        None | Some(Yaml::Null) => Ok(None),
        Some(Yaml::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(invalid(format!("view {name} is text"))),
    };
    let kind = text("type")?.ok_or_else(|| invalid("a view has no type"))?;
    let order = match key(map, "order") {
        None | Some(Yaml::Null) => Vec::new(),
        Some(Yaml::Sequence(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| invalid("order lists property names"))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(invalid("order is a list")),
    };
    let sort = match key(map, "sort") {
        None | Some(Yaml::Null) => Vec::new(),
        Some(Yaml::Sequence(items)) => items.iter().map(sort_key).collect::<Result<_, _>>()?,
        Some(_) => return Err(invalid("sort is a list")),
    };
    let limit = match key(map, "limit") {
        None | Some(Yaml::Null) => None,
        Some(Yaml::Number(n)) => Some(
            n.as_u64()
                .ok_or_else(|| invalid("limit is a whole number"))? as usize,
        ),
        Some(_) => return Err(invalid("limit is a whole number")),
    };
    let group_by = match key(map, "groupBy") {
        None | Some(Yaml::Null) => None,
        Some(entry @ Yaml::Mapping(_)) => Some(sort_key(entry)?),
        Some(_) => return Err(invalid("groupBy is a mapping")),
    };
    let group_order = match key(map, "groupOrder") {
        None | Some(Yaml::Null) => None,
        Some(Yaml::Sequence(items)) => Some(items.iter().map(plain).collect::<Result<_, _>>()?),
        Some(_) => return Err(invalid("groupOrder is a list")),
    };
    let summaries = expressions(key(map, "summaries"), "summaries")?;
    Ok(View {
        kind,
        name: text("name")?,
        filter: key(map, "filters")
            .filter(|v| !v.is_null())
            .map(|v| filter(v, 0))
            .transpose()?,
        order,
        sort,
        limit,
        group_by,
        group_order,
        summaries,
    })
}

fn sort_key(value: &Yaml) -> Result<SortKey, BasesError> {
    let Yaml::Mapping(map) = value else {
        return Err(invalid("a sort or groupBy entry is a mapping"));
    };
    let property = key(map, "property")
        .and_then(Yaml::as_str)
        .ok_or_else(|| invalid("a sort or groupBy entry has a property"))?;
    let direction = match key(map, "direction").and_then(Yaml::as_str) {
        None => Direction::Asc,
        Some(d) if d.eq_ignore_ascii_case("asc") => Direction::Asc,
        Some(d) if d.eq_ignore_ascii_case("desc") => Direction::Desc,
        Some(d) => {
            return Err(BasesError::Unsupported {
                expression: format!("direction: {d}"),
                feature: Feature::SortDirection,
                subject: d.to_string(),
            });
        }
    };
    Ok(SortKey {
        property: property.to_string(),
        direction,
    })
}

impl Base {
    pub fn parse(source: &str) -> Result<Base, BasesError> {
        let root: Yaml =
            serde_yaml::from_str(source).map_err(|e| BasesError::Yaml(e.to_string()))?;
        let map = match &root {
            Yaml::Mapping(map) => map,
            Yaml::Null => return Ok(Base::default()),
            _ => return Err(invalid("a base is a mapping")),
        };
        let filter_of = match key(map, "filters") {
            None | Some(Yaml::Null) => None,
            Some(value) => Some(filter(value, 0)?),
        };
        let mut display_names = Vec::new();
        match key(map, "properties") {
            None | Some(Yaml::Null) => {}
            Some(Yaml::Mapping(properties)) => {
                for (id, settings) in properties {
                    let (Some(id), Yaml::Mapping(settings)) = (id.as_str(), settings) else {
                        continue;
                    };
                    if let Some(name) = key(settings, "displayName").and_then(Yaml::as_str) {
                        display_names.push((id.to_string(), name.to_string()));
                    }
                }
            }
            Some(_) => return Err(invalid("properties is a mapping")),
        }
        let views = match key(map, "views") {
            None | Some(Yaml::Null) => Vec::new(),
            Some(Yaml::Sequence(items)) => items.iter().map(view).collect::<Result<_, _>>()?,
            Some(_) => return Err(invalid("views is a list")),
        };
        Ok(Base {
            filter: filter_of,
            display_names,
            formulas: expressions(key(map, "formulas"), "formulas")?,
            summaries: expressions(key(map, "summaries"), "summaries")?,
            views,
        })
    }
}
