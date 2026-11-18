use std::collections::{HashMap, HashSet};

use hayro::hayro_interpret::TransformExt as _;
use hayro::hayro_interpret::kurbo::{Point, Rect};
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::object::{Array, Dict, MaybeRef, Name, Object, ObjectIdentifier};
use hayro::hayro_syntax::page::Page;
use serde::{Deserialize, Serialize};

const MAX_OUTLINE_ITEMS: usize = 10_000;
const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutlineItem {
    pub title: String,
    pub depth: usize,
    pub page: Option<usize>,
    pub top: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LinkTarget {
    Page { page: usize, top: Option<f32> },
    Uri(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub rect: [f32; 4],
    pub target: LinkTarget,
}

pub fn text_string(bytes: &[u8]) -> String {
    if let Some(utf16) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let units: Vec<u16> = utf16
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(utf8) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(utf8).into_owned();
    }
    bytes
        .iter()
        .map(|b| match b {
            0x80 => '\u{2022}',
            0x84 => '\u{2014}',
            0x85 => '\u{2013}',
            0x8d => '\u{201c}',
            0x8e => '\u{201d}',
            0x8f => '\u{2018}',
            0x90 => '\u{2019}',
            0x91 => '\u{201a}',
            0x92 => '\u{2122}',
            0x93 => '\u{fb01}',
            0x94 => '\u{fb02}',
            0xa0 => '\u{20ac}',
            b => char::from(*b),
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub struct Pages {
    index: HashMap<ObjectIdentifier, usize>,
    heights: Vec<f32>,
}

impl Pages {
    pub fn of(pdf: &Pdf) -> Pages {
        let mut index = HashMap::new();
        let mut heights = Vec::new();
        for (i, page) in pdf.pages().iter().enumerate() {
            if let Some(id) = page.raw().obj_id() {
                index.insert(id, i);
            }
            heights.push(page.render_dimensions().1);
        }
        Pages { index, heights }
    }
}

fn catalog(pdf: &Pdf) -> Option<Dict<'_>> {
    pdf.xref().get::<Dict<'_>>(pdf.xref().root_id())
}

fn explicit(array: &Array<'_>, pages: &Pages) -> Option<(usize, Option<f32>)> {
    let mut items = array.raw_iter();
    let page = match items.next()? {
        MaybeRef::Ref(r) => *pages.index.get(&ObjectIdentifier::from(r))?,
        MaybeRef::NotRef(object) => object.into_f32()? as usize,
    };
    let kind = match items.next() {
        Some(MaybeRef::NotRef(Object::Name(name))) => name.as_str().to_string(),
        _ => String::new(),
    };
    let top = match kind.as_str() {
        "XYZ" => {
            items.next();
            items.next()
        }
        "FitH" | "FitBH" => items.next(),
        _ => None,
    }
    .and_then(|item| match item {
        MaybeRef::NotRef(object) => object.into_f32(),
        MaybeRef::Ref(_) => None,
    });
    let height = *pages.heights.get(page)?;
    Some((page, top.map(|top| (height - top).clamp(0., height))))
}

fn named(pdf: &Pdf, name: &[u8], pages: &Pages) -> Option<(usize, Option<f32>)> {
    let catalog = catalog(pdf)?;
    if let Some(dests) = catalog.get::<Dict<'_>>("Dests")
        && let Some(dest) = dests.get::<Object<'_>>(name)
    {
        return destination(pdf, dest, pages, 1);
    }
    let tree = catalog.get::<Dict<'_>>("Names")?.get::<Dict<'_>>("Dests")?;
    let found = name_tree(&tree, name, 0)?;
    destination(pdf, found, pages, 1)
}

fn name_tree<'a>(node: &Dict<'a>, name: &[u8], depth: usize) -> Option<Object<'a>> {
    if depth > MAX_DEPTH {
        return None;
    }
    if let Some(names) = node.get::<Array<'a>>("Names") {
        let mut items = names.iter::<Object<'a>>();
        while let (Some(key), Some(value)) = (items.next(), items.next()) {
            if let Object::String(key) = key
                && key.as_bytes() == name
            {
                return Some(value);
            }
        }
    }
    let kids = node.get::<Array<'a>>("Kids")?;
    kids.iter::<Dict<'a>>()
        .find_map(|kid| name_tree(&kid, name, depth + 1))
}

fn destination(
    pdf: &Pdf,
    dest: Object<'_>,
    pages: &Pages,
    depth: usize,
) -> Option<(usize, Option<f32>)> {
    if depth > 4 {
        return None;
    }
    match dest {
        Object::Array(array) => explicit(&array, pages),
        Object::Name(name) => named(pdf, name.as_ref(), pages),
        Object::String(string) => named(pdf, string.as_bytes(), pages),
        Object::Dict(dict) => destination(pdf, dict.get::<Object<'_>>("D")?, pages, depth + 1),
        _ => None,
    }
}

fn target(pdf: &Pdf, item: &Dict<'_>, pages: &Pages) -> Option<LinkTarget> {
    if let Some(dest) = item.get::<Object<'_>>("Dest") {
        let (page, top) = destination(pdf, dest, pages, 0)?;
        return Some(LinkTarget::Page { page, top });
    }
    let action = item.get::<Dict<'_>>("A")?;
    let kind = action.get::<Name<'_>>("S")?;
    match kind.as_str() {
        "GoTo" => {
            let (page, top) = destination(pdf, action.get::<Object<'_>>("D")?, pages, 0)?;
            Some(LinkTarget::Page { page, top })
        }
        "URI" => {
            let uri = action.get::<hayro::hayro_syntax::object::String<'_>>("URI")?;
            let uri = String::from_utf8_lossy(uri.as_bytes()).trim().to_string();
            (!uri.is_empty()).then_some(LinkTarget::Uri(uri))
        }
        _ => None,
    }
}

pub fn outline(pdf: &Pdf, pages: &Pages) -> Vec<OutlineItem> {
    let mut out = Vec::new();
    let Some(root) = catalog(pdf).and_then(|c| c.get::<Dict<'_>>("Outlines")) else {
        return out;
    };
    let mut seen = HashSet::new();
    walk(pdf, &root, 0, pages, &mut seen, &mut out);
    out
}

fn walk(
    pdf: &Pdf,
    parent: &Dict<'_>,
    depth: usize,
    pages: &Pages,
    seen: &mut HashSet<ObjectIdentifier>,
    out: &mut Vec<OutlineItem>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut next = parent.get::<Dict<'_>>("First");
    while let Some(item) = next {
        if out.len() >= MAX_OUTLINE_ITEMS {
            return;
        }
        if let Some(id) = item.obj_id()
            && !seen.insert(id)
        {
            return;
        }
        let title = item
            .get::<hayro::hayro_syntax::object::String<'_>>("Title")
            .map(|t| text_string(t.as_bytes()))
            .unwrap_or_default();
        let (page, top) = match target(pdf, &item, pages) {
            Some(LinkTarget::Page { page, top }) => (Some(page), top),
            _ => (None, None),
        };
        out.push(OutlineItem {
            title,
            depth,
            page,
            top,
        });
        walk(pdf, &item, depth + 1, pages, seen, out);
        next = item.get::<Dict<'_>>("Next");
    }
}

pub fn links(pdf: &Pdf, page: &Page<'_>, pages: &Pages) -> Vec<Link> {
    let transform = page.initial_transform(true).to_kurbo();
    let Some(annots) = page.raw().get::<Array<'_>>("Annots") else {
        return Vec::new();
    };
    annots
        .iter::<Dict<'_>>()
        .filter(|annot| {
            annot
                .get::<Name<'_>>("Subtype")
                .is_some_and(|s| s.as_str() == "Link")
        })
        .filter_map(|annot| {
            let r = annot.get::<[f32; 4]>("Rect")?;
            let a = transform * Point::new(f64::from(r[0]), f64::from(r[1]));
            let b = transform * Point::new(f64::from(r[2]), f64::from(r[3]));
            let rect = Rect::from_points(a, b);
            Some(Link {
                rect: [
                    rect.x0 as f32,
                    rect.y0 as f32,
                    rect.x1 as f32,
                    rect.y1 as f32,
                ],
                target: target(pdf, &annot, pages)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::{open, sample};

    #[test]
    fn the_outline_follows_its_tree_and_its_destinations() {
        let pdf = open(sample::document()).unwrap();
        let pages = Pages::of(&pdf);
        let items = outline(&pdf, &pages);
        assert_eq!(
            items,
            vec![
                OutlineItem {
                    title: "Summary".into(),
                    depth: 0,
                    page: Some(0),
                    top: Some(0.)
                },
                OutlineItem {
                    title: "Détails".into(),
                    depth: 1,
                    page: Some(1),
                    top: Some(400.)
                },
                OutlineItem {
                    title: "Annex".into(),
                    depth: 0,
                    page: Some(2),
                    top: None
                },
            ]
        );
    }

    #[test]
    fn links_go_to_a_page_or_give_their_address() {
        let pdf = open(sample::document()).unwrap();
        let pages = Pages::of(&pdf);
        let all = pdf.pages();
        let found = links(&pdf, &all[0], &pages);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].target, LinkTarget::Page { page: 2, top: None });
        assert_eq!(found[0].rect, [72., 130., 200., 146.]);
        assert_eq!(
            found[1].target,
            LinkTarget::Uri("https://example.org/report".into())
        );
        assert!(links(&pdf, &all[1], &pages).is_empty());
    }

    #[test]
    fn text_strings_are_decoded_by_their_mark() {
        assert_eq!(text_string(b"\xfe\xff\x00A\x00\xe9"), "Aé");
        assert_eq!(text_string(b"\xef\xbb\xbfZ\xc3\xa9"), "Zé");
        assert_eq!(text_string(b"Caf\xe9 \x84 x"), "Café \u{2014} x");
    }
}
