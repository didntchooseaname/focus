use std::collections::{BTreeMap, BTreeSet};

use fluent_syntax::ast::{
    CallArguments, Entry, Expression, InlineExpression, Pattern, PatternElement, VariantKey,
};
use fluent_syntax::parser;

use crate::catalog::embedded_domains;

#[derive(Debug, Clone, Default)]
pub struct Message {
    pub domain: &'static str,
    pub variables: BTreeSet<String>,
    pub text: String,
    pub variant_keys: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct Catalog {
    pub messages: BTreeMap<String, Message>,
    pub problems: Vec<String>,
}

pub fn embedded(code: &str) -> Catalog {
    let mut catalog = Catalog::default();
    for (domain, source) in embedded_domains(code) {
        let resource = match parser::parse(source) {
            Ok(resource) => resource,
            Err((resource, errors)) => {
                for error in errors {
                    catalog
                        .problems
                        .push(format!("{code}/{domain}.ftl: {error}"));
                }
                resource
            }
        };
        for entry in &resource.body {
            let (id, value) = match entry {
                Entry::Message(message) => (message.id.name.to_string(), message.value.as_ref()),
                Entry::Term(term) => (format!("-{}", term.id.name), Some(&term.value)),
                _ => continue,
            };
            let mut message = Message {
                domain,
                ..Message::default()
            };
            if let Some(pattern) = value {
                message.text =
                    pattern_text(pattern, &mut message.variables, &mut message.variant_keys);
            }
            if catalog.messages.insert(id.clone(), message).is_some() {
                catalog.problems.push(format!("{code}: duplicate id {id}"));
            }
        }
    }
    catalog
}

fn pattern_text(
    pattern: &Pattern<&str>,
    variables: &mut BTreeSet<String>,
    keys: &mut BTreeSet<String>,
) -> String {
    let mut text = String::new();
    for element in &pattern.elements {
        match element {
            PatternElement::TextElement { value } => text.push_str(value),
            PatternElement::Placeable { expression } => {
                text.push_str(&expression_text(expression, variables, keys))
            }
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn expression_text(
    expression: &Expression<&str>,
    variables: &mut BTreeSet<String>,
    keys: &mut BTreeSet<String>,
) -> String {
    match expression {
        Expression::Inline(inline) => inline_text(inline, variables, keys),
        Expression::Select { selector, variants } => {
            inline_text(selector, variables, keys);
            for variant in variants {
                keys.insert(match &variant.key {
                    VariantKey::Identifier { name } => (*name).to_string(),
                    VariantKey::NumberLiteral { value } => format!("[{value}]"),
                });
            }
            variants
                .iter()
                .map(|v| pattern_text(&v.value, variables, keys))
                .max_by_key(|t| t.chars().count())
                .unwrap_or_default()
        }
    }
}

fn inline_text(
    inline: &InlineExpression<&str>,
    variables: &mut BTreeSet<String>,
    keys: &mut BTreeSet<String>,
) -> String {
    match inline {
        InlineExpression::VariableReference { id } => {
            variables.insert(id.name.to_string());
            String::new()
        }
        InlineExpression::StringLiteral { value } => (*value).to_string(),
        InlineExpression::Placeable { expression } => expression_text(expression, variables, keys),
        InlineExpression::FunctionReference { arguments, .. } => {
            collect_arguments(arguments, variables, keys);
            String::new()
        }
        InlineExpression::TermReference { arguments, .. } => {
            if let Some(arguments) = arguments {
                collect_arguments(arguments, variables, keys);
            }
            String::new()
        }
        InlineExpression::NumberLiteral { .. } | InlineExpression::MessageReference { .. } => {
            String::new()
        }
    }
}

fn collect_arguments(
    arguments: &CallArguments<&str>,
    variables: &mut BTreeSet<String>,
    keys: &mut BTreeSet<String>,
) {
    for positional in &arguments.positional {
        inline_text(positional, variables, keys);
    }
    for named in &arguments.named {
        inline_text(&named.value, variables, keys);
    }
}
