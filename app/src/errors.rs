use focus_core::VaultError;
use focus_i18n::t_args;

pub fn vault(error: &VaultError) -> String {
    match error {
        VaultError::Io { path, source } => t_args(
            "errors-core-io",
            &[
                ("path", path.display().to_string().as_str().into()),
                ("detail", source.to_string().as_str().into()),
            ],
        ),
        VaultError::OutsideVault(path) => t_args(
            "errors-core-outside-vault",
            &[("path", path.display().to_string().as_str().into())],
        ),
        VaultError::AlreadyExists(path) => t_args(
            "errors-core-already-exists",
            &[("path", path.display().to_string().as_str().into())],
        ),
        VaultError::UnknownNote(_) => focus_i18n::t("errors-core-unknown-note"),
        VaultError::Conflict(path) => t_args(
            "errors-core-conflict",
            &[("path", path.display().to_string().as_str().into())],
        ),
        VaultError::Other(message) => {
            t_args("errors-core-other", &[("message", message.as_str().into())])
        }
    }
}

pub fn clipboard(error: &focus_kit::clipboard::Error) -> String {
    use focus_kit::clipboard::Error;
    match error {
        Error::Timeout { .. } => focus_i18n::t("clipboard-timeout"),
        Error::TooLarge { limit } => t_args(
            "clipboard-too-large",
            &[(
                "size",
                focus_i18n::format_bytes(*limit as u64).as_str().into(),
            )],
        ),
        Error::Start { tool, .. } => t_args("clipboard-start", &[("tool", tool.as_str().into())]),
        Error::Failed { tool, .. } => t_args("clipboard-failed", &[("tool", tool.as_str().into())]),
        Error::NoTool => t_args("clipboard-failed", &[("tool", "wl-paste, xclip".into())]),
    }
}
