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
    command!("ui-cmd-toggle-left", "ctrl-b", ToggleLeft, true),
    command!("ui-cmd-toggle-right", "ctrl-alt-b", ToggleRight, true),
    command!("ui-cmd-toggle-bottom", "ctrl-j", ToggleBottom, true),
    command!("ui-cmd-search", "ctrl-shift-f", SearchVault, true),
    command!("ui-cmd-files", "ctrl-shift-e", ShowFiles, true),
    command!("ui-cmd-close-tab", "ctrl-w", CloseTab, true),
    command!("ui-cmd-next-tab", "ctrl-tab", NextTab, true),
    command!("ui-cmd-prev-tab", "ctrl-shift-tab", PrevTab, true),
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
    command!("ui-cmd-task", "ctrl-l", FormatTask, false),
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

pub fn strokes(keys: &str) -> Vec<String> {
    keys.split(' ')
        .map(|stroke| {
            let parts: Vec<&str> = if stroke.ends_with("--") || stroke == "-" {
                vec![stroke.trim_end_matches('-'), "-"]
            } else {
                stroke.split('-').collect()
            };
            parts
                .iter()
                .filter(|p| !p.is_empty())
                .map(|part| match *part {
                    "ctrl" => focus_i18n::t("ui-key-ctrl"),
                    "alt" => focus_i18n::t("ui-key-alt"),
                    "shift" => focus_i18n::t("ui-key-shift"),
                    "cmd" | "super" => focus_i18n::t("ui-key-super"),
                    "escape" => focus_i18n::t("ui-key-escape"),
                    "enter" => focus_i18n::t("ui-key-enter"),
                    "tab" => focus_i18n::t("ui-key-tab"),
                    "left" => focus_i18n::t("ui-key-left"),
                    "right" => focus_i18n::t("ui-key-right"),
                    "up" => focus_i18n::t("ui-key-up"),
                    "down" => focus_i18n::t("ui-key-down"),
                    "space" => focus_i18n::t("ui-key-space"),
                    other => other.to_uppercase(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

pub fn display(keys: &str) -> String {
    strokes(keys).join(" ")
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
        .map(|keys| display(keys))
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

pub fn remap(keystroke: &gpui::Keystroke, editor: bool) -> Option<Remap> {
    let guard = OVERRIDES
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let overrides = guard.as_ref().filter(|o| !o.is_empty())?;
    let tables: &[(&[Command], bool)] = if editor {
        &[(EDITOR_COMMANDS, true), (COMMANDS, false)]
    } else {
        &[(COMMANDS, false)]
    };
    for (table, _) in tables {
        for command in table.iter() {
            if let Some(keys) = overrides.get(command.label)
                && keys.iter().any(|k| same(k, keystroke))
            {
                return Some(Remap::Run((command.action)()));
            }
        }
    }
    for (table, _) in tables {
        for command in table.iter() {
            if !command.keys.is_empty()
                && same(command.keys, keystroke)
                && overrides.contains_key(command.label)
            {
                return Some(Remap::Drop);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::display;

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
        super::set_overrides("{}");
        assert!(super::remap(&old, false).is_none());
    }

    #[test]
    fn readable_shortcuts() {
        focus_i18n::set_language("fr").expect("French is embedded");
        assert_eq!(display("ctrl-shift-f"), "Ctrl Maj F");
        assert_eq!(display("ctrl-,"), "Ctrl ,");
        assert_eq!(display("alt-left"), "Alt Gauche");
        assert_eq!(display("escape"), "Échap");
    }
}
