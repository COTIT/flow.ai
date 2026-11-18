//! NetSuite failure classes: transport (sent or not) and `nsError` bodies by HTTP status
//! (facts N5 l.256-257, N6 l.262-290). Details are secret-free.

use crate::{
    integrations::capability::{ErrorClass, ProviderError},
    outbound::Failure,
};

/// Longest provider detail kept.
const DETAIL_MAX: usize = 300;

/// A provider error.
pub fn err(
    class: ErrorClass,
    detail: impl Into<String>,
    code: Option<String>,
    http_status: Option<u16>,
) -> ProviderError {
    ProviderError {
        class,
        detail: detail.into(),
        code,
        http_status,
    }
}

/// Transport failure. A failed connect (DNS, refused, connect timeout) sent nothing → Transient;
/// anything after connect (timeout, reset) may have reached NetSuite → Ambiguous. A refused host
/// is a config error → Validation.
pub fn transport(f: Failure) -> ProviderError {
    match f {
        Failure::Refused(e) => err(
            ErrorClass::Validation,
            format!("NetSuite host refused: {e}"),
            None,
            None,
        ),
        Failure::Http(e) if e.is_connect() => err(
            ErrorClass::Transient,
            "NetSuite could not be reached; nothing was sent.",
            None,
            None,
        ),
        Failure::Http(e) => err(
            ErrorClass::Ambiguous,
            format!(
                "The request was sent but no reply came ({}).",
                if e.is_timeout() {
                    "timeout"
                } else {
                    "connection lost"
                }
            ),
            None,
            None,
        ),
    }
}

/// Class of a non-success reply by status (keyed on status, l.257). 5xx is Ambiguous here;
/// reads turn it into Transient.
pub fn from_status(status: u16, body: &[u8]) -> ProviderError {
    let v: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let first = &v["o:errorDetails"][0];
    let code = first["o:errorCode"]
        .as_str()
        .or(v["o:errorCode"].as_str())
        .map(str::to_owned);
    let class = match status {
        400 => ErrorClass::Validation,
        401 => ErrorClass::Auth,
        // N6: no REST permission code is documented; 403 alone means Permission.
        403 => ErrorClass::Permission,
        404 => ErrorClass::NotFound,
        409 => ErrorClass::Conflict,
        429 => ErrorClass::Transient,
        500.. => ErrorClass::Ambiguous,
        _ => ErrorClass::Unknown,
    };
    let mut detail: String = first["detail"]
        .as_str()
        .or(v["title"].as_str())
        .unwrap_or("NetSuite refused the request.")
        .chars()
        .take(DETAIL_MAX)
        .collect();
    if let Some(path) = first["o:errorPath"].as_str() {
        detail = format!("{detail} ({path})");
    }
    err(class, detail, code, Some(status))
}

/// Reads have no side effect: Ambiguous becomes Transient.
pub fn for_read(mut e: ProviderError) -> ProviderError {
    if e.class == ErrorClass::Ambiguous {
        e.class = ErrorClass::Transient;
    }
    e
}
