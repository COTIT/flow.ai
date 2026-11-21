//! Provider failure → incident category + kind + readable sentence (contracts §11.3 map).

use crate::integrations::capability::ErrorClass;

/// Where a failure lands: `category` and incident `kind` per §11.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classified {
    pub category: &'static str,
    pub kind: &'static str,
}

// shortcut: words of the WF-01 mapped fields (vendor, item, subsidiary, currency, location,
// unit); upgrade when a provider returns a structured field name.
const MAPPED: &[&str] = &[
    "entity",
    "vendor",
    "item",
    "subsidiary",
    "currency",
    "location",
    "unit",
];

/// Map one provider failure. `detail` is the provider's secret-free text.
pub fn classify(class: ErrorClass, http_status: Option<u16>, detail: &str) -> Classified {
    let c = |category, kind| Classified { category, kind };
    match class {
        ErrorClass::Transient if http_status == Some(429) => c("transient", "rate_limited"),
        ErrorClass::Transient => c("transient", "exchange_failed"),
        ErrorClass::Auth => c("authorization", "auth_failure"),
        ErrorClass::Permission => c("authorization", "permission_denied"),
        ErrorClass::Validation => {
            let d = detail.to_lowercase();
            if MAPPED.iter().any(|w| d.contains(w)) {
                c("data", "mapping_invalid")
            } else {
                c("data", "exchange_failed")
            }
        }
        ErrorClass::Conflict => c("conflict", "exchange_failed"),
        ErrorClass::NotFound => c("missing", "exchange_failed"),
        ErrorClass::Ambiguous | ErrorClass::Unknown => c("unknown_outcome", "exchange_failed"),
    }
}

/// Never retried by a job (§4.2): a person must fix something first.
pub fn permanent(class: ErrorClass) -> bool {
    matches!(
        class,
        ErrorClass::Validation | ErrorClass::Permission | ErrorClass::Auth | ErrorClass::Conflict
    )
}

/// Code stored in `diagnosis.code` (never shown).
pub fn code(class: ErrorClass) -> &'static str {
    match class {
        ErrorClass::Transient => "transient",
        ErrorClass::Ambiguous => "ambiguous",
        ErrorClass::Auth => "auth",
        ErrorClass::Permission => "permission",
        ErrorClass::NotFound => "not_found",
        ErrorClass::Validation => "validation",
        ErrorClass::Conflict => "conflict",
        ErrorClass::Unknown => "unknown",
    }
}

/// Readable sentence per incident kind; the UI shows this, never the code.
pub fn sentence(kind: &str, category: &str) -> &'static str {
    match (kind, category) {
        ("exchange_late", "unknown_outcome") => {
            "We sent the record but could not confirm it arrived. Check the other system before trying again."
        }
        ("exchange_late", _) => "The record has not appeared in the other system in time.",
        ("auth_failure", _) => {
            "The connection's sign-in no longer works. Reconnect it in integration settings."
        }
        ("permission_denied", _) => {
            "The connection is not allowed to do this. Ask the system admin to grant access."
        }
        ("rate_limited", _) => "The other system is busy. We will try again shortly.",
        ("mapping_invalid", _) => {
            "The other system refused a mapped value. Check the mappings for this pair."
        }
        ("target_conflict", _) => {
            "The record in the other system differs from what we sent. Nothing was changed there."
        }
        (_, "data") => "The other system refused the data we sent.",
        (_, "conflict") => "The other system reports a conflict with an existing record.",
        (_, "missing") => "The record could not be found in the other system.",
        _ => "We could not tell whether the other system received the record.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wf03_category_map_per_error_class() {
        let cases = [
            (
                ErrorClass::Transient,
                Some(429),
                "",
                "transient",
                "rate_limited",
            ),
            (
                ErrorClass::Transient,
                Some(503),
                "",
                "transient",
                "exchange_failed",
            ),
            (
                ErrorClass::Auth,
                Some(401),
                "",
                "authorization",
                "auth_failure",
            ),
            (
                ErrorClass::Permission,
                Some(403),
                "",
                "authorization",
                "permission_denied",
            ),
            (
                ErrorClass::Validation,
                Some(400),
                "Invalid item reference.",
                "data",
                "mapping_invalid",
            ),
            (
                ErrorClass::Validation,
                Some(400),
                "Memo too long.",
                "data",
                "exchange_failed",
            ),
            (
                ErrorClass::Conflict,
                Some(409),
                "",
                "conflict",
                "exchange_failed",
            ),
            (
                ErrorClass::NotFound,
                Some(404),
                "",
                "missing",
                "exchange_failed",
            ),
            (
                ErrorClass::Ambiguous,
                None,
                "",
                "unknown_outcome",
                "exchange_failed",
            ),
            (
                ErrorClass::Unknown,
                Some(500),
                "",
                "unknown_outcome",
                "exchange_failed",
            ),
        ];
        for (class, status, detail, category, kind) in cases {
            assert_eq!(
                classify(class, status, detail),
                Classified { category, kind },
                "{class:?}"
            );
        }
        assert!(permanent(ErrorClass::Validation) && permanent(ErrorClass::Conflict));
        assert!(!permanent(ErrorClass::Transient) && !permanent(ErrorClass::Ambiguous));
        assert!(!permanent(ErrorClass::NotFound));
    }
}
