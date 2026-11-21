use std::rc::Rc;

use focus_core::{PropertyType, TypeOrigin, ValueError};
use focus_kit::{Checkbox, Chip, Flow as _, Icon, MenuRow, Tokens, Type, size};
use gpui::{
    AnyElement, App, Context, ElementId, FocusHandle, FontWeight, Global, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, ParentElement, Pixels, Point, SharedString,
    StatefulInteractiveElement, Styled, Window, div, point, prelude::FluentBuilder, px,
};

use super::{ReadingView, Zone};
use crate::model::Prop;
use crate::text::Base;

type ChooseType = dyn Fn(&str, Option<PropertyType>, &mut Window, &mut App);

struct TypeHost(Rc<ChooseType>);

impl Global for TypeHost {}

pub fn set_property_type_host(
    cx: &mut App,
    host: impl Fn(&str, Option<PropertyType>, &mut Window, &mut App) + 'static,
) {
    cx.set_global(TypeHost(Rc::new(host)));
}

pub fn property_checkbox_edit(
    text: &str,
    key: &str,
    checked: bool,
) -> Option<(std::ops::Range<usize>, String)> {
    let word = if checked { "true" } else { "false" };
    let mut at = 0;
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_start_matches('\u{feff}').trim_end() != "---" {
        return None;
    }
    at += first.len();
    for raw in lines {
        let start = at;
        at += raw.len();
        let line = raw.trim_end_matches(['\n', '\r']);
        if matches!(line.trim_end(), "---" | "...") {
            return None;
        }
        if line.starts_with([' ', '\t', '#', '-']) {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        if line[..colon].trim() != key {
            continue;
        }
        let after = &line[colon + 1..];
        let after = match after.find(" #") {
            Some(comment) => &after[..comment],
            None => after,
        };
        let value = after.trim();
        let from = start + colon + 1;
        return Some(if value.is_empty() {
            (from..from + after.len(), format!(" {word}"))
        } else {
            let lead = after.len() - after.trim_start().len();
            (from + lead..from + lead + value.len(), word.to_string())
        });
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyToggled {
    pub key: String,
    pub checked: bool,
}

pub(super) struct PropMenu {
    name: String,
    kind: focus_core::PropertyKind,
    at: Point<Pixels>,
    selected: usize,
    focus: FocusHandle,
}

impl PropMenu {
    fn rows(&self) -> Vec<Option<PropertyType>> {
        let mut rows: Vec<Option<PropertyType>> =
            PropertyType::CHOOSABLE.iter().copied().map(Some).collect();
        if self.kind.origin == TypeOrigin::Focus {
            rows.push(None);
        }
        rows
    }
}

pub(crate) fn type_label(kind: PropertyType) -> String {
    focus_i18n::t(&format!("reading-prop-type-{}", kind.name()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldError {
    Value(ValueError),
    TagSpace,
    TagHash,
    TagChar(char),
    TagNumber,
}

pub(crate) fn error_text(error: FieldError) -> String {
    let key = match error {
        FieldError::Value(ValueError::NotNumber) => "reading-prop-error-number",
        FieldError::Value(ValueError::DateForm) => "reading-prop-error-date",
        FieldError::Value(ValueError::DatetimeForm) => "reading-prop-error-datetime",
        FieldError::Value(ValueError::NoSuchDay) => "reading-prop-error-day",
        FieldError::Value(ValueError::NoSuchTime) => "reading-prop-error-time",
        FieldError::TagSpace => "reading-prop-error-tag-space",
        FieldError::TagHash => "reading-prop-error-tag-hash",
        FieldError::TagChar(c) => {
            return focus_i18n::t_args(
                "reading-prop-error-tag-char",
                &[("char", c.to_string().as_str().into())],
            );
        }
        FieldError::TagNumber => "reading-prop-error-tag-number",
    };
    focus_i18n::t(key)
}

pub(crate) fn has_form(kind: PropertyType) -> bool {
    matches!(
        kind,
        PropertyType::Number | PropertyType::Date | PropertyType::Datetime
    )
}

impl ReadingView {
    fn names_choose_type(&self, cx: &App) -> bool {
        cx.has_global::<TypeHost>() && !(self.live && self.in_place.is_none())
    }

    fn caret_in(&self, prop: &Prop) -> bool {
        let (Some(frame), Some(span)) = (
            self.in_place_frame.as_ref(),
            prop.value.map.as_ref().and_then(|m| m.span()),
        ) else {
            return false;
        };
        span.start <= frame.head && frame.head <= span.end
    }

    fn field_error(&self, prop: &Prop) -> Option<FieldError> {
        if let Some((key, error)) = &self.prop_refused
            && *key == prop.key
        {
            return Some(*error);
        }
        let frame = self.in_place_frame.as_ref()?;
        if prop.kind.kind == PropertyType::Tags {
            return super::inplace::props::tag_error(&frame.text, frame.head);
        }
        let span = prop.value.map.as_ref()?.span()?;
        let written = frame.text.slice(span).to_string();
        let written = written.trim_matches(|c| c == '"' || c == '\'');
        prop.kind.kind.check(written).err().map(FieldError::Value)
    }

    fn tags_as_field(&self, prop: &Prop) -> bool {
        prop.value.text.is_empty() || self.caret_in(prop)
    }

    pub(super) fn prop_chips(&self, t: &Tokens, cx: &App) -> Vec<AnyElement> {
        let Some(built) = &self.built else {
            return Vec::new();
        };
        let menus = self.names_choose_type(cx);
        let small = Base {
            size: px(12.5),
            line_height: px(18.),
            color: t.ink,
            weight: FontWeight::SEMIBOLD,
            ..self.base(t)
        };
        let mut tags_field = false;
        let mut chips: Vec<AnyElement> = built
            .header
            .props
            .iter()
            .enumerate()
            .filter_map(|(index, prop)| {
                let tags = prop.kind.kind == PropertyType::Tags;
                if tags {
                    if !self.tags_as_field(prop) {
                        return None;
                    }
                    tags_field = true;
                }
                let empty = prop.value.text.is_empty() && !prop.is_checkbox();
                let typed = has_form(prop.kind.kind);
                let editing = (typed || empty || tags) && self.caret_in(prop);
                let boxed = (typed && (prop.fits || editing)) || empty;
                let error = editing.then(|| self.field_error(prop)).flatten();
                let name = div()
                    .id(("prop-name", index))
                    .relative()
                    .flex_none()
                    .text_color(t.ink_3)
                    .child(prop.key.clone());
                let fixed = matches!(prop.kind.kind, PropertyType::Tags | PropertyType::Aliases);
                let name = if menus && !fixed {
                    self.zone(
                        name.cursor_pointer().hover(move |s| s.text_color(t.ink)),
                        Zone::PropName(index),
                    )
                } else {
                    name
                };
                let value: AnyElement = if prop.is_checkbox() {
                    let check = div().relative().flex_none().cursor_pointer().child(
                        Checkbox::new(("prop-check", index), prop.checked == Some(true))
                            .forced(focus_kit::Forced::Live),
                    );
                    self.zone(check, Zone::PropCheck(index)).into_any_element()
                } else {
                    let field = if boxed {
                        let border = match (editing, error) {
                            (true, Some(_)) => t.danger,
                            (true, None) => t.ink,
                            (false, _) if empty => t.line_2,
                            (false, _) => t.card_2,
                        };
                        div()
                            .min_w_0()
                            .when(empty, |this| this.min_w(size::S12))
                            .px(size::S1)
                            .rounded(size::RADIUS)
                            .border_1()
                            .border_color(border)
                            .when(editing || empty, |this| this.bg(t.bg_2))
                    } else {
                        div().min_w_0()
                    };
                    let field = field.relative().child(self.text(0, &prop.value, &small, t));
                    if prop.value.map.is_some() {
                        self.zone(field, Zone::PropField(index)).into_any_element()
                    } else {
                        field.into_any_element()
                    }
                };
                let mismatch = (!prop.fits && !editing).then(|| {
                    let label: SharedString = focus_i18n::t_args(
                        "reading-prop-mismatch",
                        &[("type", type_label(prop.kind.kind).as_str().into())],
                    )
                    .into();
                    div()
                        .id(("prop-mismatch", index))
                        .flex_none()
                        .text_color(t.warn)
                        .child(focus_kit::icon(Icon::ALERT, focus_kit::size::ICON_SMALL))
                        .tooltip(move |_, cx| focus_kit::Tooltip::view(label.clone(), None, cx))
                });
                let row = div()
                    .flow_row()
                    .items_center()
                    .gap(size::S1)
                    .max_w_full()
                    .child(name)
                    .child(value)
                    .children(mismatch);
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .flex_none()
                        .max_w_full()
                        .px(px(6.))
                        .py(if boxed { px(1.) } else { px(2.) })
                        .rounded(size::RADIUS)
                        .bg(t.card_2)
                        .ui_sm()
                        .child(row)
                        .when_some(error, |this, error| {
                            let message = error_text(error);
                            this.child(
                                focus_kit::Dir::current()
                                    .align_text(div(), &message)
                                    .pb(px(1.))
                                    .ui_sm()
                                    .text_color(t.danger)
                                    .child(message),
                            )
                        })
                        .into_any_element(),
                )
            })
            .collect();
        if !tags_field {
            chips.extend(built.header.tags.iter().map(|tag| {
                self.zone(
                    div()
                        .relative()
                        .flex_none()
                        .cursor_pointer()
                        .child(Chip::tag(tag.clone())),
                    Zone::Tag(tag.to_string()),
                )
                .into_any_element()
            }));
        }
        chips
    }

    pub(super) fn toggle_prop(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(prop) = self.built.as_ref().and_then(|b| b.header.props.get(index)) else {
            return;
        };
        let (key, checked) = (prop.key.clone(), prop.checked != Some(true));
        let Some(field) = self.in_place.clone() else {
            cx.emit(PropertyToggled { key, checked });
            return;
        };
        let text = field.read(cx).text().to_string();
        let Some((range, written)) = property_checkbox_edit(&text, &key, checked) else {
            return;
        };
        let head = self
            .field_state(window, cx)
            .map_or(range.start, |(_, _, head)| head);
        let caret = if head >= range.end {
            head + written.len() - range.len()
        } else {
            head.min(range.start)
        };
        self.apply(
            super::inplace::Edit {
                range,
                text: written,
                caret,
            },
            window,
            cx,
        );
    }

    pub(super) fn open_prop_menu(
        &mut self,
        index: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(prop) = self.built.as_ref().and_then(|b| b.header.props.get(index)) else {
            return;
        };
        let focus = cx.focus_handle();
        let menu = PropMenu {
            name: prop.key.clone(),
            kind: prop.kind,
            at,
            selected: 0,
            focus: focus.clone(),
        };
        let selected = menu
            .rows()
            .iter()
            .position(|row| *row == Some(prop.kind.kind))
            .unwrap_or(0);
        self.prop_menu = Some(PropMenu { selected, ..menu });
        window.focus(&focus);
        cx.notify();
    }

    pub(in crate::view) fn close_prop_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.prop_menu.take().is_some() {
            if self.caret_shown() {
                self.start_editing(window, cx);
            } else {
                window.focus(&self.focus);
            }
            cx.notify();
        }
    }

    fn choose_prop_type(
        &mut self,
        row: Option<PropertyType>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.prop_menu.as_ref() else {
            return;
        };
        let name = menu.name.clone();
        self.close_prop_menu(window, cx);
        if let Some(host) = cx.try_global::<TypeHost>().map(|h| h.0.clone()) {
            host(&name, row, window, cx);
        }
    }

    fn prop_menu_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.prop_menu.as_mut() else {
            return;
        };
        let rows = menu.rows();
        match event.keystroke.key.as_str() {
            "down" => menu.selected = (menu.selected + 1) % rows.len(),
            "up" => menu.selected = (menu.selected + rows.len() - 1) % rows.len(),
            "enter" | "space" => {
                let row = rows[menu.selected.min(rows.len() - 1)];
                cx.stop_propagation();
                return self.choose_prop_type(row, window, cx);
            }
            "escape" => {
                cx.stop_propagation();
                return self.close_prop_menu(window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn render_prop_menu(
        &self,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.prop_menu.as_ref()?;
        let rows = menu.rows();
        let mut children = Vec::new();
        for (n, row) in rows.iter().enumerate() {
            let label = match row {
                Some(kind) if Some(*kind) == menu.kind.obsidian => focus_i18n::t_args(
                    "reading-prop-type-from-obsidian",
                    &[("type", type_label(*kind).as_str().into())],
                ),
                Some(kind) => type_label(*kind),
                None => {
                    children.push(focus_kit::menu_separator(t).into_any_element());
                    match menu.kind.obsidian {
                        Some(kind) => focus_i18n::t_args(
                            "reading-prop-type-back-obsidian",
                            &[("type", type_label(kind).as_str().into())],
                        ),
                        None => focus_i18n::t("reading-prop-type-back-value"),
                    }
                }
            };
            let chosen = *row;
            children.push(
                MenuRow::new(ElementId::from(("prop-type", n)), label)
                    .checked(chosen.is_some() && chosen == Some(menu.kind.kind))
                    .highlighted(n == menu.selected)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_prop_type(chosen, window, cx)
                    }))
                    .into_any_element(),
            );
        }
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(menu.at + point(px(0.), px(4.)))
                    .snap_to_window()
                    .child(focus_kit::fade_in(
                        "fade-prop-type-menu",
                        focus_kit::menu(t)
                            .id("prop-type-menu")
                            .occlude()
                            .track_focus(&menu.focus)
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_mouse_down_out(
                                cx.listener(|this, _, window, cx| this.close_prop_menu(window, cx)),
                            )
                            .capture_key_down(cx.listener(
                                |this, event: &KeyDownEvent, window, cx| {
                                    this.prop_menu_key(event, window, cx)
                                },
                            ))
                            .child(
                                div()
                                    .px(size::S3)
                                    .pt(size::S1)
                                    .pb(size::S1)
                                    .ui_xs()
                                    .text_color(t.ink_3)
                                    .child(focus_i18n::t_args(
                                        "reading-prop-type-title",
                                        &[("name", menu.name.as_str().into())],
                                    )),
                            )
                            .children(children),
                        cx,
                    )),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
