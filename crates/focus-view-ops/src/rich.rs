use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_ops::Host;
use focus_ops::relations::host_mentions;
use focus_view_reading::model::LinkGoal;
use focus_view_reading::{ReadingEvent, ReadingView, Resolver, Typesetting};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, IntoElement, Subscription,
};

use crate::event::OpsEvent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Segment {
    Text(String),
    Host(String, PathBuf),
    Wiki { target: String, label: String },
    Tag(String),
}

pub(crate) fn segments(text: &str, hosts: &[Host]) -> Vec<Segment> {
    let mut spans: Vec<(Range<usize>, Segment)> = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find("[[") {
        let start = from + open;
        let Some(close) = text[start + 2..].find("]]") else {
            break;
        };
        let end = start + 2 + close + 2;
        let inner = &text[start + 2..end - 2];
        if !inner.is_empty() && !inner.contains('[') {
            let (target, label) = inner
                .split_once('|')
                .map_or((inner, inner), |(t, l)| (t, l));
            spans.push((
                start..end,
                Segment::Wiki {
                    target: target.trim().to_string(),
                    label: label.trim().to_string(),
                },
            ));
        }
        from = end;
    }
    let bytes = text.as_bytes();
    for (start, _) in text.match_indices('#') {
        if start > 0
            && !bytes[start - 1].is_ascii_whitespace()
            && !matches!(bytes[start - 1], b'(' | b'[')
        {
            continue;
        }
        let name: String = text[start + 1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'))
            .collect();
        if !name.is_empty() && !name.chars().all(|c| c.is_ascii_digit()) {
            spans.push((start..start + 1 + name.len(), Segment::Tag(name)));
        }
    }
    for mention in host_mentions(text, hosts) {
        spans.push((
            mention.range.clone(),
            Segment::Host(text[mention.range].to_string(), mention.host),
        ));
    }
    spans.sort_by_key(|(range, _)| (range.start, std::cmp::Reverse(range.len())));
    let mut out = Vec::new();
    let mut cursor = 0;
    for (range, segment) in spans {
        if range.start < cursor {
            continue;
        }
        if range.start > cursor {
            out.push(Segment::Text(text[cursor..range.start].to_string()));
        }
        out.push(segment);
        cursor = range.end;
    }
    if cursor < text.len() {
        out.push(Segment::Text(text[cursor..].to_string()));
    }
    out
}

pub(crate) struct NoVault;

impl Resolver for NoVault {
    fn resolve_link(&self, _from: focus_core::NoteId, _target: &str) -> focus_core::LinkTarget {
        focus_core::LinkTarget::Unresolved
    }

    fn known_file(&self, target: &str) -> Option<PathBuf> {
        let path = Path::new(target);
        (path.is_absolute() && path.exists()).then(|| path.to_path_buf())
    }

    fn absolute(&self, relative: &Path) -> PathBuf {
        relative.to_path_buf()
    }
}

pub(crate) fn prepare(text: &str, hosts: &[Host]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for segment in segments(text, hosts) {
        match segment {
            Segment::Text(t) => out.push_str(&t),
            Segment::Host(label, path) => out.push_str(&format!("[[{}|{label}]]", path.display())),
            Segment::Wiki { target, label } if target == label => {
                out.push_str(&format!("[[{target}]]"))
            }
            Segment::Wiki { target, label } => out.push_str(&format!("[[{target}|{label}]]")),
            Segment::Tag(name) => out.push_str(&format!("#{name}")),
        }
    }
    out
}

pub(crate) fn to_event(event: &ReadingEvent) -> Option<OpsEvent> {
    match event {
        ReadingEvent::OpenLink { goal, .. } => match goal {
            LinkGoal::Unresolved(target) if Path::new(target).is_absolute() => {
                Some(OpsEvent::OpenNote {
                    path: PathBuf::from(target),
                    line: None,
                })
            }
            LinkGoal::Unresolved(target) => Some(OpsEvent::OpenLink {
                target: target.clone(),
            }),
            LinkGoal::Attachment(path) => Some(OpsEvent::OpenNote {
                path: path.clone(),
                line: None,
            }),
            LinkGoal::Tag(name) => Some(OpsEvent::SearchTag(name.clone())),
            LinkGoal::Note { .. } | LinkGoal::External(_) | LinkGoal::Footnote(_) => None,
        },
        ReadingEvent::Zoom(source) => Some(OpsEvent::ZoomDiagram(source.clone())),
        ReadingEvent::Hover {
            goal: Some(LinkGoal::Unresolved(target)),
            position,
        } => Some(OpsEvent::HoverLink {
            target: Some(target.clone()),
            position: *position,
        }),
        ReadingEvent::Hover {
            goal: Some(LinkGoal::Attachment(path)),
            position,
        } => Some(OpsEvent::HoverLink {
            target: Some(path.display().to_string()),
            position: *position,
        }),
        ReadingEvent::Hover {
            goal: None,
            position,
        } => Some(OpsEvent::HoverLink {
            target: None,
            position: *position,
        }),
        ReadingEvent::Hover { .. } | ReadingEvent::ToggleTask(_) => None,
    }
}

pub(crate) fn rendering_view<V: EventEmitter<OpsEvent> + 'static>(
    measure: f32,
    cx: &mut Context<V>,
) -> (Entity<ReadingView>, Subscription) {
    let view = cx.new(|cx| ReadingView::compact(Typesetting::compact(measure), cx));
    let events = cx.subscribe(&view, |_: &mut V, _, event: &ReadingEvent, cx| {
        if let Some(event) = to_event(event) {
            cx.emit(event);
        }
    });
    (view, events)
}

pub(crate) fn set_text(view: &Entity<ReadingView>, text: String, cx: &mut App) {
    view.update(cx, |view, cx| {
        view.set_markdown(text, None, Arc::new(NoVault), cx)
    });
}

pub(crate) fn set_text_now(view: &Entity<ReadingView>, text: String, cx: &mut App) {
    view.update(cx, |view, cx| {
        view.set_markdown_sync(text, None, &NoVault, cx)
    });
}

pub(crate) struct Compact {
    views: HashMap<String, (Entity<ReadingView>, String, Subscription)>,
}

impl Compact {
    pub fn new() -> Compact {
        Compact {
            views: HashMap::new(),
        }
    }

    pub fn element<V: EventEmitter<OpsEvent> + 'static>(
        &mut self,
        key: String,
        text: &str,
        hosts: &[Host],
        cx: &mut Context<V>,
    ) -> AnyElement {
        const MAX_VIEWS: usize = 400;
        let prepared = prepare(text, hosts);
        if self.views.len() > MAX_VIEWS && !self.views.contains_key(&key) {
            self.views.clear();
        }
        if let Some((view, shown, _)) = self.views.get_mut(&key) {
            if *shown != prepared {
                *shown = prepared.clone();
                set_text_now(view, prepared, cx);
            }
            return view.clone().into_any_element();
        }
        let (view, subscription) = rendering_view(480., cx);
        set_text_now(&view, prepared.clone(), cx);
        self.views
            .insert(key, (view.clone(), prepared, subscription));
        view.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_ops::HostStatus;

    fn host(ip: &str, name: &str, note: &str) -> Host {
        Host {
            path: PathBuf::from(note),
            ip: ip.into(),
            hostname: Some(name.into()),
            os: None,
            status: HostStatus::New,
            services: vec![],
            tags: vec![],
            flags: vec![],
            points: None,
        }
    }

    #[test]
    fn links_tags_and_hosts_are_separated_in_reading_order() {
        let hosts = [host("10.0.0.5", "dc01", "Hosts/dc01.md")];
        let parts = segments(
            "Relayed to 10.0.0.5 see [[Kerberoasting|the note]] #ad",
            &hosts,
        );
        assert_eq!(
            parts,
            vec![
                Segment::Text("Relayed to ".into()),
                Segment::Host("10.0.0.5".into(), PathBuf::from("Hosts/dc01.md")),
                Segment::Text(" see ".into()),
                Segment::Wiki {
                    target: "Kerberoasting".into(),
                    label: "the note".into()
                },
                Segment::Text(" ".into()),
                Segment::Tag("ad".into()),
            ]
        );
    }

    #[test]
    fn a_host_inside_a_wiki_link_does_not_split_it() {
        let hosts = [host("10.0.0.5", "dc01", "Hosts/dc01.md")];
        let parts = segments("[[dc01 notes]]", &hosts);
        assert_eq!(
            parts,
            vec![Segment::Wiki {
                target: "dc01 notes".into(),
                label: "dc01 notes".into()
            }]
        );
    }

    #[test]
    fn plain_text_and_non_tags_stay_text() {
        assert_eq!(
            segments("issue #12 and a#b", &[]),
            vec![Segment::Text("issue #12 and a#b".into())]
        );
    }

    #[test]
    fn hosts_become_wiki_links_to_their_notes_in_the_prepared_text() {
        let hosts = [host("10.0.0.5", "dc01", "/e/Hosts/dc01.md")];
        assert_eq!(
            prepare("on 10.0.0.5 and [[Other|x]] #ad", &hosts),
            "on [[/e/Hosts/dc01.md|10.0.0.5]] and [[Other|x]] #ad"
        );
    }

    #[test]
    fn link_events_become_ops_events() {
        let open = |goal| ReadingEvent::OpenLink {
            goal,
            new_tab: false,
        };
        assert_eq!(
            to_event(&open(LinkGoal::Unresolved("/e/Hosts/dc01.md".into()))),
            Some(OpsEvent::OpenNote {
                path: PathBuf::from("/e/Hosts/dc01.md"),
                line: None
            })
        );
        assert_eq!(
            to_event(&open(LinkGoal::Unresolved("Kerberoasting".into()))),
            Some(OpsEvent::OpenLink {
                target: "Kerberoasting".into()
            })
        );
        assert_eq!(
            to_event(&open(LinkGoal::Tag("ad".into()))),
            Some(OpsEvent::SearchTag("ad".into()))
        );
        assert_eq!(to_event(&ReadingEvent::ToggleTask(3)), None);
        let position = gpui::point(gpui::px(4.), gpui::px(8.));
        assert_eq!(
            to_event(&ReadingEvent::Hover {
                goal: Some(LinkGoal::Unresolved("/e/Hosts/dc01.md".into())),
                position
            }),
            Some(OpsEvent::HoverLink {
                target: Some("/e/Hosts/dc01.md".into()),
                position
            })
        );
        assert_eq!(
            to_event(&ReadingEvent::Hover {
                goal: None,
                position
            }),
            Some(OpsEvent::HoverLink {
                target: None,
                position
            })
        );
    }
}
