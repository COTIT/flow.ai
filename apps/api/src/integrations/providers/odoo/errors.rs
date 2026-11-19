//! Odoo JSON-2 failure → `ErrorClass` (facts `odoo-19.md` L28, L33-39, L84-87). Every C3a call
//! is a read, so anything that may pass on retry is `Transient`. The `debug` traceback and the
//! key never reach a detail.

use serde_json::Value;

use crate::integrations::capability::{ErrorClass, ProviderError};

const MESSAGE_CHARS: usize = 200;

fn error(
    class: ErrorClass,
    detail: String,
    code: Option<String>,
    status: Option<u16>,
) -> ProviderError {
    ProviderError {
        class,
        detail,
        code,
        http_status: status,
    }
}

/// Request never answered: connect, DNS, address guard or timeout.
pub(super) fn unsent(_reason: String) -> ProviderError {
    // Reason text can name the host; keep the detail generic.
    error(
        ErrorClass::Transient,
        "Odoo could not be reached. It will be tried again.".into(),
        None,
        None,
    )
}

/// 2xx whose body is not the documented shape.
pub(super) fn unreadable(what: &str) -> ProviderError {
    error(
        ErrorClass::Unknown,
        format!("Odoo sent a reply that could not be read ({what})."),
        None,
        None,
    )
}

/// Non-2xx reply.
pub(super) fn from_status(status: u16, body: &[u8]) -> ProviderError {
    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    let field = |k: &str| {
        parsed
            .as_ref()
            .and_then(|v| v.get(k))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let name = field("name");
    let message: String = field("message")
        .unwrap_or_default()
        .chars()
        .take(MESSAGE_CHARS)
        .collect();
    let said = |lead: &str| {
        if message.is_empty() {
            format!("{lead}.")
        } else {
            format!("{lead}: {message}")
        }
    };
    let (class, detail) = match status {
        401 => (
            ErrorClass::Auth,
            "Odoo refused the API key. Save a new key on the connection.".to_owned(),
        ),
        403 => (
            ErrorClass::Permission,
            said("The Odoo user may not do this"),
        ),
        404 => (
            ErrorClass::NotFound,
            said("Odoo does not know this model or method"),
        ),
        409 => (ErrorClass::Conflict, said("The Odoo record is locked")),
        400 | 422 => (ErrorClass::Validation, said("Odoo rejected the request")),
        // L86-87: unknown field or malformed domain; retrying won't fix it.
        500 if name.as_deref() == Some("builtins.ValueError") => {
            (ErrorClass::Validation, said("Odoo rejected the query"))
        }
        429 | 500..=599 => (
            ErrorClass::Transient,
            format!("Odoo is unavailable (HTTP {status}). It will be tried again."),
        ),
        _ => (
            ErrorClass::Unknown,
            said(&format!("Odoo answered HTTP {status}")),
        ),
    };
    error(class, detail, name, Some(status))
}
