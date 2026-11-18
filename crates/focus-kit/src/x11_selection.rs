use x11rb::connection::{Connection, RequestConnection as _};
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode,
    SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::clipboard::Error;
use crate::file_list::Offer;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Answer<'a> {
    Targets(Vec<&'a str>),
    Data(&'a [u8]),
    Refused,
}

pub(crate) struct Selection {
    targets: Vec<(String, Vec<u8>)>,
}

impl Selection {
    pub(crate) fn new(offer: &Offer) -> Selection {
        let mut targets = Vec::new();
        for (mime, body) in &offer.types {
            targets.push((mime.to_string(), body.clone()));
            if mime.starts_with("text/plain") {
                targets.push(("UTF8_STRING".to_string(), body.clone()));
            }
        }
        Selection { targets }
    }

    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.targets
            .iter()
            .map(|(name, _)| name.as_str())
            .chain(["TARGETS", "TIMESTAMP"])
    }

    pub(crate) fn answer(&self, target: &str, limit: usize) -> Answer<'_> {
        if target == "TARGETS" {
            return Answer::Targets(self.names().collect());
        }
        match self.targets.iter().find(|(name, _)| name == target) {
            Some((_, body)) if body.len() <= limit => Answer::Data(body),
            _ => Answer::Refused,
        }
    }
}

fn failure(detail: impl ToString) -> Error {
    Error::Failed {
        tool: "x11 selection".into(),
        detail: detail.to_string(),
    }
}

pub(crate) fn serve(offer: &Offer) -> Result<(), Error> {
    let selection = Selection::new(offer);
    let (conn, screen) = RustConnection::connect(None).map_err(failure)?;
    let root = conn.setup().roots[screen].root;
    let window = conn.generate_id().map_err(failure)?;
    conn.create_window(
        0,
        window,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )
    .map_err(failure)?;
    let atom = |name: &str| -> Result<Atom, Error> {
        Ok(conn
            .intern_atom(false, name.as_bytes())
            .map_err(failure)?
            .reply()
            .map_err(failure)?
            .atom)
    };
    let clipboard = atom("CLIPBOARD")?;
    let named: Vec<(Atom, String)> = selection
        .names()
        .map(|name| atom(name).map(|a| (a, name.to_string())))
        .collect::<Result<_, _>>()?;
    conn.set_selection_owner(window, clipboard, x11rb::CURRENT_TIME)
        .map_err(failure)?;
    conn.flush().map_err(failure)?;
    if conn
        .get_selection_owner(clipboard)
        .map_err(failure)?
        .reply()
        .map_err(failure)?
        .owner
        != window
    {
        return Err(failure("could not take the CLIPBOARD selection"));
    }
    let limit = conn.maximum_request_bytes().saturating_sub(64);
    std::thread::spawn(move || {
        while let Ok(event) = conn.wait_for_event() {
            match event {
                Event::SelectionClear(clear) if clear.selection == clipboard => break,
                Event::SelectionRequest(request) => {
                    let property = if request.property == x11rb::NONE {
                        request.target
                    } else {
                        request.property
                    };
                    let name = named
                        .iter()
                        .find(|(a, _)| *a == request.target)
                        .map(|(_, n)| n.as_str());
                    let answered = request.selection == clipboard
                        && match name.map(|n| selection.answer(n, limit)) {
                            Some(Answer::Targets(list)) => {
                                let atoms: Vec<Atom> = list
                                    .iter()
                                    .filter_map(|l| {
                                        named.iter().find(|(_, n)| n == l).map(|(a, _)| *a)
                                    })
                                    .collect();
                                conn.change_property32(
                                    PropMode::REPLACE,
                                    request.requestor,
                                    property,
                                    AtomEnum::ATOM,
                                    &atoms,
                                )
                                .is_ok()
                            }
                            Some(Answer::Data(body)) => conn
                                .change_property8(
                                    PropMode::REPLACE,
                                    request.requestor,
                                    property,
                                    request.target,
                                    body,
                                )
                                .is_ok(),
                            _ => false,
                        };
                    let notify = SelectionNotifyEvent {
                        response_type: SELECTION_NOTIFY_EVENT,
                        sequence: 0,
                        time: request.time,
                        requestor: request.requestor,
                        selection: request.selection,
                        target: request.target,
                        property: if answered { property } else { x11rb::NONE },
                    };
                    conn.send_event(false, request.requestor, EventMask::NO_EVENT, notify)
                        .ok();
                    conn.flush().ok();
                }
                _ => {}
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(cut: bool) -> Selection {
        Selection::new(&Offer::new(&[std::path::PathBuf::from("/v/a b.md")], cut).unwrap())
    }

    #[test]
    fn targets_lists_every_type_with_the_text_aliases_and_the_two_standard_ones() {
        let s = selection(false);
        let names: Vec<&str> = match s.answer("TARGETS", 1000) {
            Answer::Targets(list) => list,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            names,
            [
                "text/uri-list",
                "x-special/gnome-copied-files",
                "text/plain;charset=utf-8",
                "UTF8_STRING",
                "TARGETS",
                "TIMESTAMP"
            ]
        );
        assert!(
            selection(true)
                .names()
                .any(|n| n == "application/x-kde-cutselection")
        );
    }

    #[test]
    fn each_target_gets_its_body_and_the_text_alias_the_same_one() {
        let s = selection(true);
        assert_eq!(
            s.answer("x-special/gnome-copied-files", 1000),
            Answer::Data(b"cut\nfile:///v/a%20b.md")
        );
        assert_eq!(
            s.answer("text/uri-list", 1000),
            Answer::Data(b"file:///v/a%20b.md\r\n")
        );
        assert_eq!(s.answer("UTF8_STRING", 1000), Answer::Data(b"/v/a b.md"));
        assert_eq!(
            s.answer("application/x-kde-cutselection", 1000),
            Answer::Data(b"1")
        );
    }

    #[test]
    fn an_unknown_target_or_a_value_over_the_limit_is_refused() {
        let s = selection(false);
        assert_eq!(s.answer("image/png", 1000), Answer::Refused);
        assert_eq!(s.answer("text/uri-list", 5), Answer::Refused);
    }
}
