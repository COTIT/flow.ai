//! Document numbers as Laravel DomainNumber printed them: prefix + zero-padded number +
//! suffix. The appendix is snapshotted on each document when it gets its number.

use serde_json::{Value, json};

/// `{prefix, digits, suffix}` of a numbering series (a settings section's numbering fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appendix {
    pub prefix: Option<String>,
    pub digits: usize,
    pub suffix: Option<String>,
}

/// Most digits a series pads to (settings validation: `digits` max 10).
const MAX_DIGITS: usize = 10;

impl Appendix {
    /// Numbering fields of a settings section; Laravel DomainNumber::from used 1 digit when unset.
    pub fn from_section(section: &Value) -> Self {
        let text = |key: &str| section.get(key).and_then(Value::as_str).map(str::to_owned);
        let digits = section
            .get("digits")
            .and_then(Value::as_u64)
            .map_or(1, |d| {
                usize::try_from(d)
                    .unwrap_or(MAX_DIGITS)
                    .clamp(1, MAX_DIGITS)
            });
        Self {
            prefix: text("prefix"),
            digits,
            suffix: text("suffix"),
        }
    }

    /// The snapshot stored on a document (`*_appendix`).
    pub fn to_json(&self) -> Value {
        json!({ "prefix": self.prefix, "digits": self.digits, "suffix": self.suffix })
    }

    /// Laravel `prefix . str_pad(number, digits, '0', STR_PAD_LEFT) . suffix`.
    pub fn format(&self, number: i64) -> String {
        format!(
            "{}{number:0>width$}{}",
            self.prefix.as_deref().unwrap_or(""),
            self.suffix.as_deref().unwrap_or(""),
            width = self.digits
        )
    }
}

/// `start_number` of a settings section, at least 1; Laravel GetHighestNumber fell back to 1.
pub fn start_number(section: &Value) -> i64 {
    section
        .get("start_number")
        .and_then(Value::as_i64)
        .map_or(1, |n| n.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_domain_number() {
        let a = Appendix::from_section(&json!({ "prefix": "INV-", "digits": 5, "suffix": "-B" }));
        assert_eq!(a.format(42), "INV-00042-B");
        assert_eq!(
            a.format(123_456),
            "INV-123456-B",
            "longer than digits: no cut"
        );
        assert_eq!(
            a.to_json(),
            json!({ "prefix": "INV-", "digits": 5, "suffix": "-B" })
        );
    }

    #[test]
    fn missing_fields_fall_back_like_laravel() {
        let a = Appendix::from_section(&json!({ "prefix": null, "digits": null }));
        assert_eq!(a.format(7), "7");
        assert_eq!(
            a.to_json(),
            json!({ "prefix": null, "digits": 1, "suffix": null })
        );
        assert_eq!(start_number(&json!({ "start_number": null })), 1);
        assert_eq!(start_number(&json!({ "start_number": 1000 })), 1000);
    }

    #[test]
    fn junk_and_bounds_are_tamed() {
        // Validation caps digits at 10; an older or hand-edited row must not blow up.
        let a = Appendix::from_section(&json!({ "prefix": 5, "digits": 999, "suffix": [] }));
        assert_eq!(a.format(1), "0000000001");
        assert_eq!(start_number(&json!({ "start_number": -3 })), 1);
        assert_eq!(start_number(&json!("not an object")), 1);
    }
}
