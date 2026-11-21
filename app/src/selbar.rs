use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Instant;

use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Icon, IconButton, MenuRow, Type, icon, menu, size};
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Context, Entity, EntityId, EntityInputHandler,
    InteractiveElement, IntoElement, KeystrokeEvent, MouseButton, MouseDownEvent, MouseUpEvent,
    ParentElement, Pixels, Point, Size, StatefulInteractiveElement, Styled, WeakEntity, Window,
    div, prelude::FluentBuilder, px,
};
use gpui_component::RopeExt as _;

use crate::editor::{NoteEditor, ViewMode};
use crate::format::{Block, Inline};
use crate::shell::{Overlay, Shell};

const GAP: f32 = 8.;
const BUTTON: f32 = 26.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Type,
    TypeOf(Block),
    Format(Inline),
    Link,
    Ai,
    Extract,
}

impl Press {
    pub fn parse(name: &str) -> Option<Press> {
        Some(match name {
            "type" => Press::Type,
            "bold" => Press::Format(Inline::Bold),
            "italic" => Press::Format(Inline::Italic),
            "strike" => Press::Format(Inline::Strike),
            "highlight" => Press::Format(Inline::Highlight),
            "code" => Press::Format(Inline::Code),
            "link" => Press::Link,
            "ai" => Press::Ai,
            "extract" => Press::Extract,
            other => Press::TypeOf(match other.strip_prefix("type:")? {
                "paragraph" => Block::Paragraph,
                "bullets" => Block::Bullets,
                "numbers" => Block::Numbers,
                "tasks" => Block::Tasks,
                "quote" => Block::Quote,
                "callout" => Block::Callout,
                "codeblock" => Block::Code,
                heading => Block::Heading(
                    heading
                        .strip_prefix('h')?
                        .parse()
                        .ok()
                        .filter(|l| (1..=6).contains(l))?,
                ),
            }),
        })
    }
}

struct Shown {
    editor: EntityId,
    since: Instant,
}

#[derive(Clone)]
struct Facts {
    editor: EntityId,
    range: Range<usize>,
    len: usize,
    block: Block,
    active: [bool; 5],
}

type Lines = (Option<Bounds<Pixels>>, Option<Bounds<Pixels>>);

#[derive(Default)]
pub struct SelectionBar {
    pressed: bool,
    dismissed: Option<(EntityId, Range<usize>)>,
    shown: Option<Shown>,
    menu: Option<Option<usize>>,
    facts: Option<Facts>,
    drawn: Rc<Cell<Option<Bounds<Pixels>>>>,
    menu_drawn: Rc<Cell<Option<Bounds<Pixels>>>>,
    placed: Option<Lines>,
}

impl SelectionBar {
    pub fn showing(&self) -> bool {
        self.shown.is_some()
    }
}

struct Target {
    editor: Entity<NoteEditor>,
    range: Range<usize>,
    first: Option<Bounds<Pixels>>,
    last: Option<Bounds<Pixels>>,
    area: Bounds<Pixels>,
}

fn field_selection(
    field: &Entity<gpui_component::input::InputState>,
    window: &mut Window,
    cx: &mut App,
) -> Range<usize> {
    field.update(cx, |input, cx| {
        let selected = input
            .selected_text_range(true, window, cx)
            .map(|s| s.range)
            .unwrap_or(0..0);
        let text = input.text();
        text.offset_utf16_to_offset(selected.start)..text.offset_utf16_to_offset(selected.end)
    })
}

impl Shell {
    fn bar_target(&self, window: &mut Window, cx: &mut App) -> Option<Target> {
        if !self.prefs.read(cx).bool("editor.selection_bar")
            || !matches!(self.overlay, Overlay::None)
            || self.slash.is_some()
        {
            return None;
        }
        let editor = self.active_editor()?.clone();
        let (view, live, in_place, input, reading) = {
            let e = editor.read(cx);
            (
                e.view,
                e.live.is_some(),
                e.edit_in_place(),
                e.input.clone(),
                e.reading.clone(),
            )
        };
        match view {
            ViewMode::Source => {
                if !gpui::Focusable::focus_handle(input.read(cx), cx).is_focused(window) {
                    return None;
                }
                let range = field_selection(&input, window, cx);
                if range.is_empty() {
                    return None;
                }
                let area = self.editor_bounds.0.get();
                let line = |byte: usize, window: &mut Window, cx: &mut App| {
                    input.update(cx, |input, cx| {
                        let text = input.text();
                        let at = text.offset_to_offset_utf16(byte);
                        let content = input.bounds_for_range(at..at, area, window, cx)?;
                        Some(Bounds {
                            origin: content.origin + input.scroll_offset(),
                            size: content.size,
                        })
                    })
                };
                let first = line(range.start, window, cx);
                let last = line(range.end, window, cx);
                let first = first.map(|f| {
                    let end = match last {
                        Some(l) if (l.top() - f.top()).abs() < px(1.) => l.left(),
                        _ => area.right(),
                    };
                    Bounds::from_corners(f.origin, gpui::point(end.max(f.left()), f.bottom()))
                });
                let visible =
                    |b: &Bounds<Pixels>| b.bottom() > area.top() && b.top() < area.bottom();
                Some(Target {
                    editor,
                    range,
                    first: first.filter(visible),
                    last: last.filter(visible),
                    area,
                })
            }
            ViewMode::Live if in_place && !live => {
                let reading = reading.read(cx);
                if reading.caret_in_property(cx) {
                    return None;
                }
                let range = reading.in_place_selection()?;
                let (first, last) = reading.selection_lines(&range);
                Some(Target {
                    editor,
                    range,
                    first,
                    last,
                    area: reading.viewport(),
                })
            }
            _ => None,
        }
    }

    pub fn selection_bar_keys(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(shown) = self.selbar.shown.as_ref().map(|s| s.editor) else {
            return false;
        };
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        let plain = !m.control && !m.alt && !m.platform && !m.shift;
        if let Some(highlighted) = self.selbar.menu {
            let count = Block::ALL.len();
            match keystroke.key.as_str() {
                "down" if plain => {
                    self.selbar.menu = Some(Some(highlighted.map_or(0, |h| (h + 1) % count)));
                }
                "up" if plain => {
                    self.selbar.menu = Some(Some(
                        highlighted.map_or(count - 1, |h| (h + count - 1) % count),
                    ));
                }
                "enter" if plain => {
                    if let Some(block) = highlighted.and_then(|h| Block::ALL.get(h)) {
                        self.bar_press(Press::TypeOf(*block), window, cx);
                    }
                }
                "escape" => self.selbar.menu = None,
                _ => return false,
            }
            cx.notify();
            return true;
        }
        let range = self.selbar.facts.as_ref().map(|f| f.range.clone());
        if keystroke.key == "escape" {
            self.selbar.dismissed = range.map(|r| (shown, r));
            self.selbar.shown = None;
            cx.notify();
            return true;
        }
        let types = keystroke.key_char.is_some() && !m.control && !m.alt && !m.platform;
        if types {
            self.selbar.dismissed = range.map(|r| (shown, r));
            self.selbar.shown = None;
            cx.notify();
        }
        false
    }

    pub fn bar_press(&mut self, press: Press, window: &mut Window, cx: &mut Context<Self>) {
        use crate::actions::*;
        let action: Box<dyn gpui::Action> = match press {
            Press::Type => {
                self.selbar.menu = match self.selbar.menu {
                    Some(_) => None,
                    None => Some(None),
                };
                cx.notify();
                return;
            }
            Press::TypeOf(block) => {
                self.selbar.menu = None;
                if let Some(editor) = self.active_editor().cloned() {
                    crate::format::set_block_type(&editor, block, window, cx);
                }
                cx.notify();
                return;
            }
            Press::Format(Inline::Bold) => Box::new(FormatBold),
            Press::Format(Inline::Italic) => Box::new(FormatItalic),
            Press::Format(Inline::Strike) => Box::new(FormatStrike),
            Press::Format(Inline::Highlight) => Box::new(FormatHighlight),
            Press::Format(Inline::Code) => Box::new(FormatCode),
            Press::Link => Box::new(FormatLink),
            Press::Ai => Box::new(AskAi),
            Press::Extract => Box::new(ExtractSelection),
        };
        self.selbar.menu = None;
        window.dispatch_action(action, cx);
        cx.notify();
    }

    pub fn script_bar(
        &mut self,
        press: Press,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.selbar.shown.is_none() {
            return Err("the selection bar does not show".into());
        }
        if matches!(press, Press::Ai) && !self.prefs.read(cx).bool("ai.enabled") {
            return Err("the AI functions are off: the bar has no AI button".into());
        }
        if let Press::TypeOf(_) = press
            && self.selbar.menu.is_none()
        {
            self.selbar.menu = Some(None);
        }
        self.bar_press(press, window, cx);
        Ok(())
    }
}

pub fn layer(shell: WeakEntity<Shell>) -> AnyElement {
    let painter = shell.clone();
    gpui::deferred(
        gpui::canvas(
            move |_, window, cx| prepaint(&shell, window, cx),
            move |_, mut drawn, window, cx| paint(&painter, &mut drawn, window, cx),
        )
        .absolute()
        .size_0(),
    )
    .with_priority(1)
    .into_any_element()
}

fn prepaint(shell: &WeakEntity<Shell>, window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let Some(entity) = shell.upgrade() else {
        return Vec::new();
    };
    let target = {
        let held = entity.read(cx).selbar.pressed;
        let target = entity.update(cx, |shell, cx| shell.bar_target(window, cx));
        target.filter(|t| {
            let pointer = t.editor.read(cx).reading.read(cx).pointer_held();
            !held && !pointer
        })
    };
    let Some(target) = target else {
        entity.update(cx, |shell, _| {
            shell.selbar.shown = None;
            shell.selbar.menu = None;
            shell.selbar.placed = None;
            shell.selbar.drawn.set(None);
            shell.selbar.menu_drawn.set(None);
        });
        return Vec::new();
    };
    let id = target.editor.entity_id();
    let dismissed = entity
        .read(cx)
        .selbar
        .dismissed
        .as_ref()
        .is_some_and(|(e, r)| *e == id && *r == target.range);
    if dismissed || (target.first.is_none() && target.last.is_none()) {
        entity.update(cx, |shell, _| {
            shell.selbar.shown = None;
            shell.selbar.menu = None;
            shell.selbar.placed = None;
            shell.selbar.drawn.set(None);
        });
        return Vec::new();
    }
    let facts = facts(&entity, &target, cx);
    let (ai, animate) = {
        let prefs = entity.read(cx).prefs.read(cx);
        (
            prefs.bool("ai.enabled"),
            !entity.read(cx).scripted && !prefs.bool("appearance.reduce_motion"),
        )
    };
    let since = entity.update(cx, |shell, _| {
        let bar = &mut shell.selbar;
        bar.placed = Some((target.first, target.last));
        if bar.shown.as_ref().is_none_or(|s| s.editor != id) {
            bar.shown = Some(Shown {
                editor: id,
                since: Instant::now(),
            });
        }
        bar.dismissed = None;
        bar.shown.as_ref().map_or_else(Instant::now, |s| s.since)
    });
    let opacity = if animate {
        let elapsed = since.elapsed().as_secs_f32() / focus_kit::motion::FADE.as_secs_f32();
        if elapsed < 1. {
            window.request_animation_frame();
        }
        elapsed.clamp(0., 1.)
    } else {
        1.
    };
    let mut bar = render_bar(shell, &facts, ai, opacity, cx);
    let measured = bar.layout_as_root(AvailableSpace::min_size(), window, cx);
    let Some(origin) = place(&target, measured) else {
        entity.update(cx, |shell, _| shell.selbar.drawn.set(None));
        return Vec::new();
    };
    bar.prepaint_at(origin, window, cx);
    let bar_bounds = Bounds::new(origin, measured);
    let mut out = vec![bar];
    let menu_open = entity.read(cx).selbar.menu;
    let mut menu_bounds = None;
    if let Some(highlighted) = menu_open {
        let mut list = render_menu(shell, facts.block, highlighted, cx);
        let size = list.layout_as_root(AvailableSpace::min_size(), window, cx);
        let viewport = window.viewport_size();
        let x = if focus_kit::Dir::current().rtl {
            bar_bounds.right() - size.width
        } else {
            bar_bounds.left()
        };
        let below = bar_bounds.bottom() + px(4.);
        let y = if below + size.height <= viewport.height - px(GAP) {
            below
        } else {
            (bar_bounds.top() - px(4.) - size.height).max(px(GAP))
        };
        let at = gpui::point(x.max(px(0.)), y);
        list.prepaint_at(at, window, cx);
        menu_bounds = Some(Bounds::new(at, size));
        out.push(list);
    }
    entity.update(cx, |shell, _| {
        shell.selbar.drawn.set(Some(bar_bounds));
        shell.selbar.menu_drawn.set(menu_bounds);
    });
    out
}

fn paint(shell: &WeakEntity<Shell>, drawn: &mut [AnyElement], window: &mut Window, cx: &mut App) {
    for element in drawn.iter_mut() {
        element.paint(window, cx);
    }
    let Some(entity) = shell.upgrade() else {
        return;
    };
    if let Some(placed) = entity.read(cx).selbar.placed {
        let now = entity.update(cx, |shell, cx| shell.bar_target(window, cx));
        if now.is_some_and(|t| (t.first, t.last) != placed) {
            window.request_animation_frame();
        }
    }
    let (bar, menu) = {
        let s = &entity.read(cx).selbar;
        (s.drawn.clone(), s.menu_drawn.clone())
    };
    let pressed = shell.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
        if phase != gpui::DispatchPhase::Capture {
            return;
        }
        let on_bar = [bar.get(), menu.get()]
            .iter()
            .flatten()
            .any(|b| b.contains(&event.position));
        if on_bar {
            return;
        }
        let _ = pressed.update(cx, |shell, cx| {
            shell.selbar.pressed = true;
            shell.selbar.menu = None;
            cx.notify();
        });
    });
    let released = shell.clone();
    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
        if phase != gpui::DispatchPhase::Bubble {
            return;
        }
        let _ = released.update(cx, |shell, cx| {
            if std::mem::take(&mut shell.selbar.pressed) {
                cx.notify();
            }
        });
    });
}

fn facts(shell: &Entity<Shell>, target: &Target, cx: &mut App) -> Facts {
    let id = target.editor.entity_id();
    let input = target.editor.read(cx).input.clone();
    let len = input.read(cx).text().len();
    if let Some(facts) = &shell.read(cx).selbar.facts
        && facts.editor == id
        && facts.range == target.range
        && facts.len == len
    {
        return facts.clone();
    }
    let text = input.read(cx).text().to_string();
    let range = target.range.clone();
    let facts = Facts {
        editor: id,
        len,
        block: crate::format::block_at(&text, range.start),
        active: Inline::ALL.map(|kind| crate::format::active(&text, &range, kind)),
        range,
    };
    shell.update(cx, |shell, _| shell.selbar.facts = Some(facts.clone()));
    facts
}

fn place(target: &Target, size: Size<Pixels>) -> Option<Point<Pixels>> {
    let area = target.area;
    let gap = px(GAP);
    let center = target
        .first
        .or(target.last)
        .map(|b| b.left() + b.size.width / 2.)?;
    let x = (center - size.width / 2.)
        .min(area.right() - gap - size.width)
        .max(area.left() + gap);
    let above = target
        .first
        .map(|f| f.top() - gap - size.height)
        .filter(|y| *y >= area.top() + gap);
    let below = target
        .last
        .map(|l| l.bottom() + gap)
        .filter(|y| *y + size.height <= area.bottom() - gap);
    let y = above.or(below)?;
    Some(gpui::point(x, y))
}

pub fn block_label(block: Block) -> String {
    match block {
        Block::Paragraph => focus_i18n::t("ui-selbar-paragraph"),
        Block::Heading(level) => focus_i18n::t_args(
            "ui-slash-heading-label",
            &[("level", u32::from(level).into())],
        ),
        Block::Bullets => focus_i18n::t("ui-slash-bullets-label"),
        Block::Numbers => focus_i18n::t("ui-slash-numbers-label"),
        Block::Tasks => focus_i18n::t("ui-slash-tasks-label"),
        Block::Quote => focus_i18n::t("ui-slash-quote-label"),
        Block::Callout => focus_i18n::t("ui-selbar-callout"),
        Block::Code => focus_i18n::t("ui-slash-codeblock-label"),
    }
}

fn block_icon(block: Block) -> Icon {
    match block {
        Block::Paragraph => Icon::PILCROW,
        Block::Heading(_) => Icon::HEADING,
        Block::Bullets => Icon::LIST,
        Block::Numbers => Icon::LIST_ORDERED,
        Block::Tasks => Icon::LIST_TODO,
        Block::Quote => Icon::TEXT_QUOTE,
        Block::Callout => Icon::MESSAGE,
        Block::Code => Icon::BRACES,
    }
}

fn rule(t: &focus_kit::Tokens) -> gpui::Div {
    div()
        .flex_none()
        .mx(size::S1)
        .w(px(1.))
        .h(px(16.))
        .bg(t.line)
}

fn render_bar(
    shell: &WeakEntity<Shell>,
    facts: &Facts,
    ai: bool,
    opacity: f32,
    cx: &mut App,
) -> AnyElement {
    let t = *cx.tokens();
    let press = |which: Press| {
        let shell = shell.clone();
        move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
            let _ = shell.update(cx, |shell, cx| shell.bar_press(which, window, cx));
        }
    };
    let format =
        |kind: Inline, id: &'static str, glyph: Icon, label: &'static str, active: bool| {
            IconButton::new(id, glyph, focus_i18n::t(label))
                .shortcut(crate::actions::shortcut(label))
                .toggled(active)
                .on_click(press(Press::Format(kind)))
        };
    let kinds = [
        ("selbar-bold", Icon::BOLD, "ui-cmd-bold"),
        ("selbar-italic", Icon::ITALIC, "ui-cmd-italic"),
        ("selbar-strike", Icon::STRIKETHROUGH, "ui-cmd-strike"),
        ("selbar-highlight", Icon::HIGHLIGHTER, "ui-cmd-highlight"),
        ("selbar-code", Icon::CODE, "ui-cmd-code"),
    ];
    let formats: Vec<AnyElement> = Inline::ALL
        .iter()
        .zip(kinds)
        .zip(facts.active)
        .map(|((kind, (id, glyph, label)), active)| {
            format(*kind, id, glyph, label, active).into_any_element()
        })
        .collect();
    let type_button = div()
        .id("selbar-type")
        .flex_none()
        .flow_row()
        .items_center()
        .gap(size::S1)
        .h(px(BUTTON))
        .px(size::S2)
        .rounded(size::RADIUS)
        .ui()
        .text_color(t.ink_2)
        .cursor_pointer()
        .hover(move |s| s.bg(t.card_2).text_color(t.ink))
        .active(move |s| s.bg(t.card_3))
        .on_click(press(Press::Type))
        .child(focus_kit::ellipsis(block_label(facts.block)))
        .child(icon(Icon::CHEVRON_DOWN, size::ICON_DENSE));
    div()
        .id("selection-bar")
        .occlude()
        .flow_row()
        .items_center()
        .p(px(2.))
        .bg(t.card)
        .border_1()
        .border_color(t.float_border)
        .opacity(opacity)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(type_button)
        .child(rule(&t))
        .children(formats)
        .child(rule(&t))
        .child(
            IconButton::new("selbar-link", Icon::LINK, focus_i18n::t("ui-cmd-link"))
                .shortcut(crate::actions::shortcut("ui-cmd-link"))
                .on_click(press(Press::Link)),
        )
        .child(rule(&t))
        .when(ai, |this| {
            this.child(
                IconButton::new("selbar-ai", Icon::SPARKLES, focus_i18n::t("ui-cmd-ask-ai"))
                    .shortcut(crate::actions::shortcut("ui-cmd-ask-ai"))
                    .on_click(press(Press::Ai)),
            )
        })
        .child(
            IconButton::new(
                "selbar-extract",
                Icon::FILE_OUTPUT,
                focus_i18n::t("ui-selbar-extract"),
            )
            .shortcut(crate::actions::shortcut("ui-cmd-extract-selection"))
            .on_click(press(Press::Extract)),
        )
        .into_any_element()
}

fn render_menu(
    shell: &WeakEntity<Shell>,
    current: Block,
    highlighted: Option<usize>,
    cx: &mut App,
) -> AnyElement {
    let t = *cx.tokens();
    let rows: Vec<AnyElement> = Block::ALL
        .iter()
        .enumerate()
        .map(|(rank, block)| {
            let shell = shell.clone();
            let block = *block;
            MenuRow::new(("selbar-type-row", rank), block_label(block))
                .icon(block_icon(block))
                .current(block == current)
                .highlighted(highlighted == Some(rank))
                .on_click(move |_, window, cx| {
                    let _ = shell.update(cx, |shell, cx| {
                        shell.bar_press(Press::TypeOf(block), window, cx)
                    });
                })
                .into_any_element()
        })
        .collect();
    menu(&t)
        .id("selection-bar-menu")
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(rows)
        .into_any_element()
}
