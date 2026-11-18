mod files;
mod markdown;
mod render;
mod style;

#[cfg(test)]
mod security_tests;

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodeColors {
    #[default]
    Soft,
    Mono,
}

#[derive(Debug, Clone)]
pub struct Labels {
    pub copy: String,
    pub copied: String,
    pub diagram: String,
    pub detected: String,
    pub properties: String,
    pub footnotes: String,
    pub back_to_text: String,
    pub callout_titles: Vec<(String, String)>,
}

impl Default for Labels {
    fn default() -> Self {
        Labels {
            copy: "Copy".into(),
            copied: "Copied".into(),
            diagram: "Diagram".into(),
            detected: "detected".into(),
            properties: "Properties".into(),
            footnotes: "Footnotes".into(),
            back_to_text: "Back to text".into(),
            callout_titles: Vec::new(),
        }
    }
}

impl Labels {
    pub(crate) fn callout_title(&self, kind: &str) -> String {
        let lower = kind.trim().to_lowercase();
        match self.callout_titles.iter().find(|(k, _)| *k == lower) {
            Some((_, label)) => label.clone(),
            None => crate::callout_kind(kind).1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HtmlOptions {
    pub title: String,
    pub dark: bool,
    pub embed_images: bool,
    pub render_mermaid: bool,
    pub embed_fonts: bool,
    pub code_colors: CodeColors,
    pub lang: String,
    pub labels: Labels,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        HtmlOptions {
            title: String::new(),
            dark: true,
            embed_images: true,
            render_mermaid: true,
            embed_fonts: true,
            code_colors: CodeColors::Soft,
            lang: "en".into(),
            labels: Labels::default(),
        }
    }
}

pub trait Resolver {
    fn image(&self, src: &str) -> Option<PathBuf>;
    fn link(&self, target: &str, anchor: Option<&str>) -> Option<String>;
    fn embed(&self, target: &str) -> Option<String>;
    fn include(&self, _path: &str) -> Option<String> {
        None
    }
}

pub fn to_html(source: &str, options: &HtmlOptions, resolver: &dyn Resolver) -> String {
    render::page(source, options, resolver)
}

pub fn to_html_fragment(source: &str, resolver: &dyn Resolver) -> String {
    render::fragment(source, resolver)
}

pub fn to_standard_markdown(source: &str, resolver: &dyn Resolver) -> String {
    markdown::to_standard(source, resolver)
}

pub fn pdf_available() -> bool {
    files::pdf_available()
}

pub fn html_to_pdf(html_file: &Path, pdf_file: &Path) -> anyhow::Result<bool> {
    files::html_to_pdf(html_file, pdf_file)
}

pub fn zip(files: &[(PathBuf, PathBuf)], destination: &Path) -> anyhow::Result<u64> {
    files::zip(files, destination)
}
