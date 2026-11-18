use focus_ai::Error;
use focus_i18n::{Arg, t_args};

pub(crate) struct Failure {
    pub message: String,
    pub retry_after: Option<u32>,
    pub settings: bool,
}

pub(crate) fn describe(error: &Error, provider: &str) -> Failure {
    let plain = |key: &str| Failure {
        message: t_args(key, &[("provider", Arg::from(provider))]),
        retry_after: None,
        settings: false,
    };
    match error {
        Error::MissingKey => Failure {
            settings: true,
            ..plain("ai-error-missing-key")
        },
        Error::InvalidKey => Failure {
            settings: true,
            ..plain("ai-error-invalid-key")
        },
        Error::NoModel => Failure {
            settings: true,
            ..plain("ai-error-no-model")
        },
        Error::InsufficientBalance => plain("ai-error-balance"),
        Error::RateLimited { retry_after_secs } => Failure {
            message: t_args(
                "ai-error-rate-limit",
                &[
                    ("provider", Arg::from(provider)),
                    ("seconds", Arg::from(*retry_after_secs)),
                ],
            ),
            retry_after: Some(*retry_after_secs),
            settings: false,
        },
        Error::BadRequest { status, message } => Failure {
            message: t_args(
                "ai-error-bad-request",
                &[
                    ("provider", Arg::from(provider)),
                    ("status", Arg::from(*status as i64)),
                    ("message", Arg::from(message.as_str())),
                ],
            ),
            retry_after: None,
            settings: false,
        },
        Error::Server { status } => Failure {
            message: t_args(
                "ai-error-server",
                &[
                    ("provider", Arg::from(provider)),
                    ("status", Arg::from(*status as i64)),
                ],
            ),
            retry_after: None,
            settings: false,
        },
        Error::Network(message) => Failure {
            message: t_args(
                "ai-error-network",
                &[
                    ("provider", Arg::from(provider)),
                    ("message", Arg::from(message.as_str())),
                ],
            ),
            retry_after: None,
            settings: false,
        },
        Error::Protocol(message) => Failure {
            message: t_args(
                "ai-error-protocol",
                &[
                    ("provider", Arg::from(provider)),
                    ("message", Arg::from(message.as_str())),
                ],
            ),
            retry_after: None,
            settings: false,
        },
        Error::Refused { category } => Failure {
            message: t_args(
                "ai-error-refused",
                &[("category", Arg::from(category.as_str()))],
            ),
            retry_after: None,
            settings: false,
        },
        Error::Cancelled => plain("ai-stopped"),
    }
}
