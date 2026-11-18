//! Focus: a drag that leaves the window for another application, as a Wayland drag source
//! (`wl_data_device.start_drag`). What it offers, and whether gpui's own drag still runs when
//! the compositor ends it. No Wayland object and only `std`, so that a workspace crate can test
//! it (`crates/focus-view-vault`, through `#[path]`): this crate is not a workspace member.

/// Offered with the caller's types. When the drag comes back over one of our windows, the
/// offer is accepted with this type only: the compositor then sends `drop` there, and nothing
/// reads the file list (a drop on our own window is gpui's drag, never an import).
pub(crate) const PRIVATE_MIME: &str = "application/x-focus-drag";

/// The types and bodies of a drag source, and its end.
pub(crate) struct DragOut {
    types: Vec<(String, Vec<u8>)>,
    ended: bool,
}

impl DragOut {
    /// `None` without a type, or when a type is the private one or given twice.
    pub(crate) fn new(types: Vec<(String, Vec<u8>)>) -> Option<DragOut> {
        let repeated = types
            .iter()
            .enumerate()
            .any(|(i, (mime, _))| types[..i].iter().any(|(earlier, _)| earlier == mime));
        if types.is_empty() || repeated || types.iter().any(|(mime, _)| mime == PRIVATE_MIME) {
            return None;
        }
        Some(DragOut {
            types,
            ended: false,
        })
    }

    /// The types to offer, the private one last.
    pub(crate) fn mime_types(&self) -> impl Iterator<Item = &str> {
        self.types
            .iter()
            .map(|(mime, _)| mime.as_str())
            .chain([PRIVATE_MIME])
    }

    /// What `wl_data_source.send` writes for `mime`. Nothing for the private type, which no
    /// one reads, nor for a type not offered.
    pub(crate) fn body(&self, mime: &str) -> Option<&[u8]> {
        self.types
            .iter()
            .find(|(offered, _)| offered == mime)
            .map(|(_, body)| body.as_slice())
    }

    /// The drag was dropped on one of our windows: gpui's drag gets that drop as a mouse up.
    pub(crate) fn dropped_here(&mut self) {
        self.ended = true;
    }

    /// The compositor ended the drag (`dnd_finished` or `cancelled`). True when gpui's drag
    /// still runs and needs a mouse up away from any element, which ends it without a drop:
    /// once the compositor holds the pointer, the button release never reaches the window.
    pub(crate) fn finish(&mut self) -> bool {
        !std::mem::replace(&mut self.ended, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<(String, Vec<u8>)> {
        vec![
            ("text/uri-list".into(), b"file:///v/a.md\r\n".to_vec()),
            (
                "x-special/gnome-copied-files".into(),
                b"copy\nfile:///v/a.md".to_vec(),
            ),
            ("text/plain;charset=utf-8".into(), b"/v/a.md".to_vec()),
        ]
    }

    #[test]
    fn the_types_are_offered_in_order_with_the_private_one_last() {
        let drag = DragOut::new(files()).expect("a drag");
        assert_eq!(
            drag.mime_types().collect::<Vec<_>>(),
            [
                "text/uri-list",
                "x-special/gnome-copied-files",
                "text/plain;charset=utf-8",
                PRIVATE_MIME
            ]
        );
    }

    #[test]
    fn each_type_sends_its_own_body_and_the_private_one_sends_nothing() {
        let drag = DragOut::new(files()).expect("a drag");
        assert_eq!(drag.body("text/uri-list"), Some(&b"file:///v/a.md\r\n"[..]));
        assert_eq!(
            drag.body("x-special/gnome-copied-files"),
            Some(&b"copy\nfile:///v/a.md"[..])
        );
        assert_eq!(drag.body(PRIVATE_MIME), None);
        assert_eq!(drag.body("image/png"), None, "a type not offered");
    }

    #[test]
    fn no_type_a_repeated_type_or_the_private_type_makes_no_drag() {
        assert!(DragOut::new(Vec::new()).is_none());
        let mut twice = files();
        twice.push(("text/uri-list".into(), Vec::new()));
        assert!(DragOut::new(twice).is_none());
        assert!(DragOut::new(vec![(PRIVATE_MIME.into(), Vec::new())]).is_none());
    }

    #[test]
    fn a_drag_ended_elsewhere_ends_the_window_drag_once() {
        let mut drag = DragOut::new(files()).expect("a drag");
        assert!(
            drag.finish(),
            "dropped in another application, or cancelled"
        );
        assert!(!drag.finish(), "a second end event changes nothing");
    }

    #[test]
    fn a_drag_dropped_on_our_window_already_ended_the_window_drag() {
        let mut drag = DragOut::new(files()).expect("a drag");
        drag.dropped_here();
        assert!(!drag.finish(), "the drop was the window's mouse up");
    }
}
