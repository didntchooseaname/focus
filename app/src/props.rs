use std::collections::BTreeMap;

use focus_core::PropertyType;
use gpui::{Context, EntityInputHandler as _, Window};
use gpui_component::RopeExt as _;

use crate::editor::NoteEditor;
use crate::shell::Shell;

impl Shell {
    pub fn mount_property_types(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let vault = library.vault().clone();
        vault.set_chosen_property_types(self.store.property_types(library.root()));
        let shell = cx.entity().downgrade();
        focus_view_reading::set_property_type_host(cx, move |name, kind, _, cx| {
            let _ = shell.update(cx, |shell, cx| shell.choose_property_type(name, kind, cx));
        });
        let (tx, rx) = smol::channel::unbounded();
        let source = vault.subscribe_config();
        if let Err(error) = std::thread::Builder::new()
            .name("focus-vault-config".into())
            .spawn(move || {
                while let Ok(event) = source.recv() {
                    if tx.send_blocking(event).is_err() {
                        break;
                    }
                }
            })
        {
            eprintln!("focus: vault configuration thread: {error}");
        }
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                if event != focus_core::ConfigEvent::PropertyTypes {
                    continue;
                }
                if this
                    .update_in(cx, |this, _, cx| this.property_types_changed(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn choose_property_type(
        &mut self,
        name: &str,
        kind: Option<PropertyType>,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let mut types: BTreeMap<String, PropertyType> = library.vault().chosen_property_types();
        match kind {
            Some(kind) => types.insert(name.to_string(), kind),
            None => types.remove(name),
        };
        self.store.set_property_types(library.root(), &types);
        self.schedule_save(cx);
        library.vault().set_chosen_property_types(types);
    }

    fn property_types_changed(&mut self, cx: &mut Context<Self>) {
        for tab in self.all_tabs() {
            tab.update(cx, |editor, cx| {
                if !editor.parked {
                    editor.schedule_parse(std::time::Duration::ZERO, cx)
                }
            });
        }
    }
}

impl NoteEditor {
    pub(crate) fn toggle_property(
        &mut self,
        toggled: &focus_view_reading::PropertyToggled,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.parked {
            return;
        }
        self.close_block(true, window, cx);
        self.input.update(cx, |input, cx| {
            let text = input.text().clone();
            let Some((range, word)) = focus_view_reading::property_checkbox_edit(
                &text.to_string(),
                &toggled.key,
                toggled.checked,
            ) else {
                return;
            };
            let range =
                text.offset_to_offset_utf16(range.start)..text.offset_to_offset_utf16(range.end);
            input.replace_text_in_range(Some(range), &word, window, cx);
        });
    }
}
