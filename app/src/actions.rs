use std::rc::Rc;

use gpui::{Action, App, KeyBinding, KeyBindingContextPredicate, actions};

use crate::media::{CopyImage, GoToPage, OpenSystem, RotateImage, RotateImageBack};

actions!(
    focus,
    [
        ToggleLeft,
        ToggleRight,
        ToggleBottom,
        OpenPalette,
        OpenCommands,
        OpenSettings,
        CheckUpdates,
        CloseTab,
        NextTab,
        PrevTab,
        SplitRight,
        FocusOtherGroup,
        Back,
        Forward,
        ToggleReading,
        ViewSource,
        ViewLive,
        ViewReading,
        OpenBlockSource,
        SearchVault,
        ShowFiles,
        Dismiss,
        Confirm,
        SelectPrev,
        SelectNext,
        SelectPageUp,
        SelectPageDown,
        SelectFirst,
        SelectLast,
        ConfirmNewTab,
        FormatBold,
        FormatItalic,
        FormatStrike,
        FormatCode,
        FormatHighlight,
        FormatLink,
        FormatHeading,
        FormatBullets,
        FormatTask,
        OpenGraph,
        OpenVaultGraph,
        OpsDashboard,
        OpsHosts,
        OpsFindings,
        OpsCredentials,
        OpsCommands,
        OpsReport,
        OpsNewEngagement,
        AskAi,
        ExportNote,
        ExportVault,
        ImportNotes,
        TidyAttachments,
        OrphanAttachments,
        CopyHtml,
        CopyMarkdown,
        ShowVersions,
        DailyNote,
        QuickCapture,
        NewFromTemplate,
        JournalEntry,
        MergeNote,
        ExtractSelection,
        MoveBlockUp,
        MoveBlockDown,
        FindInNote,
        FindReplace,
        CreateNote,
        SaveNow,
        ReopenTab,
        GoToTab1,
        GoToTab2,
        GoToTab3,
        GoToTab4,
        GoToTab5,
        GoToTab6,
        GoToTab7,
        GoToTab8,
        GoToLastTab,
        Quit,
        ToggleFullScreen,
        ShowShortcuts,
        ZoomIn,
        ZoomOut,
        ResetZoom,
        NextRegion,
        PrevRegion,
        EditorMenu,
        PastePlain,
    ]
);

pub const CONTEXT: &str = "Focus";
pub const LIST_CONTEXT: &str = "FocusList";

pub struct Command {
    pub label: &'static str,
    pub keys: &'static str,
    pub action: fn() -> Box<dyn Action>,
    pub in_palette: bool,
    pub ops: bool,
    pub ai: bool,
}

macro_rules! command {
    ($label:expr, $keys:expr, $action:ident, $palette:expr) => {
        Command {
            label: $label,
            keys: $keys,
            action: || Box::new($action),
            in_palette: $palette,
            ops: false,
            ai: false,
        }
    };
    ($label:expr, $keys:expr, $action:ident, $palette:expr, ops) => {
        Command {
            label: $label,
            keys: $keys,
            action: || Box::new($action),
            in_palette: $palette,
            ops: true,
            ai: false,
        }
    };
    ($label:expr, $keys:expr, $action:ident, $palette:expr, ai) => {
        Command {
            label: $label,
            keys: $keys,
            action: || Box::new($action),
            in_palette: $palette,
            ops: false,
            ai: true,
        }
    };
}

pub const COMMANDS: &[Command] = &[
    command!("ui-cmd-open-note", "ctrl-k", OpenPalette, false),
    command!("ui-cmd-open-note", "ctrl-p", OpenPalette, false),
    command!("ui-cmd-open-note", "ctrl-o", OpenPalette, false),
    command!("ui-cmd-commands", "ctrl-shift-p", OpenCommands, false),
    command!("ui-cmd-settings", "ctrl-,", OpenSettings, true),
    command!("ui-cmd-check-updates", "", CheckUpdates, true),
    command!("ui-cmd-toggle-left", "ctrl-shift-b", ToggleLeft, true),
    command!("ui-cmd-toggle-right", "ctrl-alt-b", ToggleRight, true),
    command!("ui-cmd-toggle-bottom", "ctrl-j", ToggleBottom, true),
    command!("ui-cmd-search", "ctrl-shift-f", SearchVault, true),
    command!("ui-cmd-files", "ctrl-shift-e", ShowFiles, true),
    command!("ui-cmd-find", "ctrl-f", FindInNote, true),
    command!("ui-cmd-find-replace", "ctrl-h", FindReplace, true),
    command!("ui-cmd-new-note", "ctrl-n", CreateNote, true),
    command!("ui-cmd-save", "ctrl-s", SaveNow, true),
    command!("ui-cmd-close-tab", "ctrl-w", CloseTab, true),
    command!("ui-cmd-reopen-tab", "ctrl-shift-t", ReopenTab, true),
    command!("ui-cmd-next-tab", "ctrl-tab", NextTab, true),
    command!("ui-cmd-next-tab", "ctrl-pagedown", NextTab, true),
    command!("ui-cmd-prev-tab", "ctrl-shift-tab", PrevTab, true),
    command!("ui-cmd-prev-tab", "ctrl-pageup", PrevTab, true),
    command!("ui-cmd-tab-1", "ctrl-1", GoToTab1, true),
    command!("ui-cmd-tab-2", "ctrl-2", GoToTab2, true),
    command!("ui-cmd-tab-3", "ctrl-3", GoToTab3, true),
    command!("ui-cmd-tab-4", "ctrl-4", GoToTab4, true),
    command!("ui-cmd-tab-5", "ctrl-5", GoToTab5, true),
    command!("ui-cmd-tab-6", "ctrl-6", GoToTab6, true),
    command!("ui-cmd-tab-7", "ctrl-7", GoToTab7, true),
    command!("ui-cmd-tab-8", "ctrl-8", GoToTab8, true),
    command!("ui-cmd-tab-last", "ctrl-9", GoToLastTab, true),
    command!("ui-cmd-split-right", "ctrl-\\", SplitRight, true),
    command!("ui-cmd-focus-other-group", "", FocusOtherGroup, true),
    command!("ui-cmd-back", "alt-left", Back, true),
    command!("ui-cmd-forward", "alt-right", Forward, true),
    command!("ui-cmd-toggle-reading", "ctrl-e", ToggleReading, true),
    command!("ui-cmd-view-source", "", ViewSource, true),
    command!("ui-cmd-view-live", "", ViewLive, true),
    command!("ui-cmd-view-reading", "", ViewReading, true),
    command!("ui-cmd-block-source", "ctrl-shift-m", OpenBlockSource, true),
    command!("ui-cmd-dismiss", "escape", Dismiss, false),
    command!("ui-cmd-next-region", "f6", NextRegion, true),
    command!("ui-cmd-prev-region", "shift-f6", PrevRegion, true),
    command!("ui-cmd-zoom-in", "ctrl-=", ZoomIn, true),
    command!("ui-cmd-zoom-in", "ctrl-+", ZoomIn, true),
    command!("ui-cmd-zoom-in", "ctrl-add", ZoomIn, true),
    command!("ui-cmd-zoom-out", "ctrl--", ZoomOut, true),
    command!("ui-cmd-zoom-out", "ctrl-subtract", ZoomOut, true),
    command!("ui-cmd-zoom-reset", "ctrl-0", ResetZoom, true),
    command!("ui-cmd-fullscreen", "f11", ToggleFullScreen, true),
    command!("ui-cmd-shortcuts", "f1", ShowShortcuts, true),
    command!("ui-cmd-shortcuts", "ctrl-/", ShowShortcuts, true),
    command!("ui-cmd-quit", "ctrl-q", Quit, true),
    command!("ui-cmd-graph", "ctrl-g", OpenGraph, true),
    command!("ui-cmd-graph-vault", "", OpenVaultGraph, true),
    command!("ui-cmd-ask-ai", "ctrl-shift-a", AskAi, true, ai),
    command!("ui-cmd-ops-dashboard", "", OpsDashboard, true, ops),
    command!("ui-cmd-ops-hosts", "", OpsHosts, true, ops),
    command!("ui-cmd-ops-findings", "", OpsFindings, true, ops),
    command!("ui-cmd-ops-credentials", "", OpsCredentials, true, ops),
    command!("ui-cmd-ops-commands", "", OpsCommands, true, ops),
    command!("ui-cmd-ops-report", "", OpsReport, true, ops),
    command!("ops-new-engagement", "", OpsNewEngagement, true, ops),
    command!(
        "ui-cmd-journal-entry",
        "ctrl-shift-j",
        JournalEntry,
        true,
        ops
    ),
    command!("ui-cmd-export-note", "", ExportNote, true),
    command!("ui-cmd-export-vault", "", ExportVault, true),
    command!("ui-cmd-import", "", ImportNotes, true),
    command!("ui-cmd-tidy-attachments", "", TidyAttachments, true),
    command!("ui-cmd-orphan-attachments", "", OrphanAttachments, true),
    command!("ui-cmd-copy-html", "", CopyHtml, true),
    command!("ui-cmd-copy-markdown", "", CopyMarkdown, true),
    command!("ui-cmd-versions", "", ShowVersions, true),
    command!("ui-cmd-merge-note", "", MergeNote, true),
    command!("ui-cmd-extract-selection", "", ExtractSelection, true),
    command!("ui-cmd-move-block-up", "alt-up", MoveBlockUp, true),
    command!("ui-cmd-move-block-down", "alt-down", MoveBlockDown, true),
    command!("ui-cmd-daily-note", "", DailyNote, true),
    command!("ui-cmd-quick-capture", "", QuickCapture, true),
    command!("ui-cmd-new-from-template", "", NewFromTemplate, true),
    command!("ui-cmd-go-to-page", "", GoToPage, true),
    command!("ui-cmd-open-system", "", OpenSystem, true),
    command!("ui-cmd-rotate-image", "", RotateImage, true),
    command!("ui-cmd-rotate-image-back", "", RotateImageBack, true),
    command!("ui-cmd-copy-image", "", CopyImage, true),
];

pub const EDITOR_CONTEXT: &str = "FocusEditor";

pub const EDITOR_COMMANDS: &[Command] = &[
    command!("ui-cmd-bold", "ctrl-b", FormatBold, false),
    command!("ui-cmd-italic", "ctrl-i", FormatItalic, false),
    command!("ui-cmd-strike", "ctrl-shift-x", FormatStrike, false),
    command!("ui-cmd-code", "ctrl-shift-c", FormatCode, false),
    command!("ui-cmd-highlight", "ctrl-alt-h", FormatHighlight, false),
    command!("ui-cmd-link", "ctrl-shift-k", FormatLink, false),
    command!("ui-cmd-heading", "ctrl-shift-h", FormatHeading, false),
    command!("ui-cmd-bullets", "ctrl-shift-8", FormatBullets, false),
    command!("ui-cmd-bullets", "ctrl-*", FormatBullets, false),
    command!("ui-cmd-task", "ctrl-l", FormatTask, false),
    command!("ui-cmd-paste-plain", "ctrl-shift-v", PastePlain, false),
    command!("ui-cmd-context-menu", "shift-f10", EditorMenu, false),
    command!("ui-cmd-context-menu", "menu", EditorMenu, false),
];

pub struct FixedKey {
    pub label: &'static str,
    pub keys: &'static [&'static str],
    pub everywhere: bool,
}

pub const FIXED_KEYS: &[FixedKey] = &[
    FixedKey {
        label: "ui-fixed-undo",
        keys: &["ctrl-z"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-fixed-redo",
        keys: &["ctrl-shift-z", "ctrl-y"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-editor-cut",
        keys: &["ctrl-x"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-action-copy",
        keys: &["ctrl-c"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-editor-paste",
        keys: &["ctrl-v"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-editor-select-all",
        keys: &["ctrl-a"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-fixed-indent",
        keys: &["tab"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-fixed-outdent",
        keys: &["shift-tab"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-find-next",
        keys: &["enter", "f3"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-find-previous",
        keys: &["shift-enter", "shift-f3"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-fixed-replace-all",
        keys: &["ctrl-enter"],
        everywhere: true,
    },
    FixedKey {
        label: "ui-fixed-tree-rename",
        keys: &["f2"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-tree-trash",
        keys: &["delete"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-tree-new-folder",
        keys: &["ctrl-shift-n"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-tree-duplicate",
        keys: &["ctrl-d"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-tree-move",
        keys: &["ctrl-m"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-tree-copy-path",
        keys: &["ctrl-shift-c"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-tree-copy-link",
        keys: &["ctrl-alt-c"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-tree-filter",
        keys: &["ctrl-f"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-tree-menu",
        keys: &["shift-f10", "menu"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-search-case",
        keys: &["alt-c"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-search-word",
        keys: &["alt-w"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-search-regex",
        keys: &["alt-r"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-search-sort",
        keys: &["alt-s"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-search-replace",
        keys: &["ctrl-shift-h"],
        everywhere: false,
    },
    FixedKey {
        label: "ui-fixed-search-help",
        keys: &["alt-h"],
        everywhere: false,
    },
];

pub fn bind(cx: &mut App) {
    let context: Option<Rc<KeyBindingContextPredicate>> =
        KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    let mapper = cx.keyboard_mapper().clone();
    let mut bindings: Vec<KeyBinding> = COMMANDS
        .iter()
        .filter(|c| !c.keys.is_empty())
        .map(|c| {
            KeyBinding::load(
                c.keys,
                (c.action)(),
                context.clone(),
                false,
                None,
                mapper.as_ref(),
            )
        })
        .filter_map(|binding| match binding {
            Ok(binding) => Some(binding),
            Err(error) => {
                eprintln!("focus: invalid shortcut: {error}");
                None
            }
        })
        .collect();
    let editor_context: Option<Rc<KeyBindingContextPredicate>> =
        KeyBindingContextPredicate::parse(EDITOR_CONTEXT)
            .ok()
            .map(Rc::new);
    for command in EDITOR_COMMANDS {
        match KeyBinding::load(
            command.keys,
            (command.action)(),
            editor_context.clone(),
            false,
            None,
            mapper.as_ref(),
        ) {
            Ok(binding) => bindings.push(binding),
            Err(error) => eprintln!("focus: invalid shortcut: {error}"),
        }
    }
    bindings.extend([
        KeyBinding::new(
            "ctrl-home",
            gpui_component::input::MoveToStart,
            Some("Input"),
        ),
        KeyBinding::new("ctrl-end", gpui_component::input::MoveToEnd, Some("Input")),
        KeyBinding::new(
            "ctrl-shift-home",
            gpui_component::input::SelectToStart,
            Some("Input"),
        ),
        KeyBinding::new(
            "ctrl-shift-end",
            gpui_component::input::SelectToEnd,
            Some("Input"),
        ),
        KeyBinding::new("ctrl-shift-z", gpui_component::input::Redo, Some("Input")),
        KeyBinding::new("up", SelectPrev, Some(LIST_CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(LIST_CONTEXT)),
        KeyBinding::new("pageup", SelectPageUp, Some(LIST_CONTEXT)),
        KeyBinding::new("pagedown", SelectPageDown, Some(LIST_CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(LIST_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(LIST_CONTEXT)),
        KeyBinding::new("ctrl-enter", ConfirmNewTab, Some(LIST_CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
    ]);
    cx.bind_keys(bindings);
}

static OVERRIDES: std::sync::RwLock<Option<std::collections::BTreeMap<String, Vec<String>>>> =
    std::sync::RwLock::new(None);

pub fn set_overrides(json: &str) {
    let parsed: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(json).unwrap_or_default();
    *OVERRIDES
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(parsed);
}

fn overridden(label: &str) -> Option<Vec<String>> {
    OVERRIDES
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()?
        .get(label)
        .cloned()
}

pub fn keys_of(label: &str, editor: bool) -> Vec<String> {
    if let Some(keys) = overridden(label) {
        return keys;
    }
    let table = if editor { EDITOR_COMMANDS } else { COMMANDS };
    table
        .iter()
        .filter(|c| c.label == label && !c.keys.is_empty())
        .map(|c| c.keys.to_string())
        .collect()
}

pub fn shortcut(label: &str) -> String {
    let editor = EDITOR_COMMANDS.iter().any(|c| c.label == label)
        && !COMMANDS.iter().any(|c| c.label == label);
    keys_of(label, editor)
        .first()
        .map(|keys| focus_i18n::keys_text(keys))
        .unwrap_or_default()
}

pub enum Remap {
    Run(Box<dyn Action>),
    Drop,
}

fn same(keys: &str, keystroke: &gpui::Keystroke) -> bool {
    gpui::Keystroke::parse(keys)
        .is_ok_and(|k| k.key == keystroke.key && k.modifiers == keystroke.modifiers)
}

type Overrides = std::collections::BTreeMap<String, Vec<String>>;

pub fn remap(keystroke: &gpui::Keystroke, editor: bool) -> Option<Remap> {
    let guard = OVERRIDES
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let overrides = guard.as_ref().filter(|o| !o.is_empty())?;
    let tables: &[&[Command]] = if editor {
        &[EDITOR_COMMANDS, COMMANDS]
    } else {
        &[COMMANDS]
    };
    remap_in(tables, overrides, keystroke)
}

fn remap_in(
    tables: &[&[Command]],
    overrides: &Overrides,
    keystroke: &gpui::Keystroke,
) -> Option<Remap> {
    let commands = || tables.iter().flat_map(|table| table.iter());
    if let Some(command) = commands().find(|command| {
        overrides
            .get(command.label)
            .is_some_and(|keys| keys.iter().any(|k| same(k, keystroke)))
    }) {
        return Some(Remap::Run((command.action)()));
    }
    let owner =
        commands().find(|command| !command.keys.is_empty() && same(command.keys, keystroke))?;
    overrides.contains_key(owner.label).then_some(Remap::Drop)
}

pub fn pressed(label: &str, keystroke: &gpui::Keystroke) -> bool {
    keys_of(label, false)
        .iter()
        .any(|keys| same(keys, keystroke))
}

#[cfg(test)]
mod tests {
    #[test]
    fn changed_shortcuts_run_their_command_and_free_the_old_key() {
        super::set_overrides(r#"{"ui-cmd-toggle-bottom": ["ctrl-shift-j"]}"#);
        let new = gpui::Keystroke::parse("ctrl-shift-j").expect("valid keys");
        let old = gpui::Keystroke::parse("ctrl-j").expect("valid keys");
        assert!(
            matches!(super::remap(&new, false), Some(super::Remap::Run(action)) if action.name() == "focus::ToggleBottom")
        );
        assert!(matches!(
            super::remap(&old, false),
            Some(super::Remap::Drop)
        ));
        assert_eq!(
            super::keys_of("ui-cmd-toggle-bottom", false),
            vec!["ctrl-shift-j".to_string()]
        );
        super::set_overrides(r#"{"ui-cmd-find": ["ctrl-alt-f"]}"#);
        let find = gpui::Keystroke::parse("ctrl-f").expect("valid keys");
        let changed = gpui::Keystroke::parse("ctrl-alt-f").expect("valid keys");
        assert!(!super::pressed("ui-cmd-find", &find));
        assert!(super::pressed("ui-cmd-find", &changed));
        super::set_overrides("{}");
        assert!(super::remap(&old, false).is_none());
        assert!(super::pressed("ui-cmd-find", &find));
        let shifted = gpui::Keystroke::parse("ctrl-shift-f").expect("valid keys");
        assert!(!super::pressed("ui-cmd-find", &shifted));
    }

    #[test]
    fn a_changed_window_shortcut_leaves_the_editor_command_on_its_key() {
        use super::{Command, FormatBold, Remap, ToggleLeft, remap_in};
        let editor: &[Command] = &[command!("ui-cmd-bold", "ctrl-b", FormatBold, false)];
        let window: &[Command] = &[command!("ui-cmd-toggle-left", "ctrl-b", ToggleLeft, true)];
        let overrides: super::Overrides =
            serde_json::from_str(r#"{"ui-cmd-toggle-left": ["ctrl-alt-l"]}"#).expect("valid json");
        let old = gpui::Keystroke::parse("ctrl-b").expect("valid keys");
        let new = gpui::Keystroke::parse("ctrl-alt-l").expect("valid keys");
        assert!(remap_in(&[editor, window], &overrides, &old).is_none());
        assert!(matches!(
            remap_in(&[window], &overrides, &old),
            Some(Remap::Drop)
        ));
        assert!(matches!(
            remap_in(&[editor, window], &overrides, &new),
            Some(Remap::Run(action)) if action.name() == "focus::ToggleLeft"
        ));
    }
}
