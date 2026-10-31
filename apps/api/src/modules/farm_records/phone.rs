//! Tanzanian mobile numbers (RUAHA `app_normalize_phone`): any common spelling -> `+255` and
//! nine digits. A farmer's phone is their login name, so one person has one spelling.

/// Refusal text, shown as written.
pub const NOT_MOBILE: &str =
    "phone number must be a Tanzanian mobile number, for example +255 712 345 678";

/// `+255 712 345 678`, `255712345678`, `0712-345-678`, `(0712) 345.678` -> `+255712345678`.
/// Mobile numbers start 6 or 7 after the country code; landlines and other lengths are refused.
pub fn normalize_phone(raw: &str) -> Result<String, &'static str> {
    let d: String = raw
        .chars()
        .filter(|c| !matches!(c, ' ' | '\t' | '(' | ')' | '.' | '-'))
        .collect();
    let digits = |s: &str| s.len() == 12 && s.bytes().all(|b| b.is_ascii_digit());
    let local = if let Some(rest) = d
        .strip_prefix('+')
        .filter(|r| digits(r) && r.starts_with("255"))
    {
        &rest[3..]
    } else if digits(&d) && d.starts_with("255") {
        &d[3..]
    } else if d.len() == 10 && d.starts_with('0') && d.bytes().all(|b| b.is_ascii_digit()) {
        &d[1..]
    } else {
        &d
    };
    let ok = local.len() == 9
        && local.bytes().all(|b| b.is_ascii_digit())
        && matches!(local.as_bytes()[0], b'6' | b'7');
    if ok {
        Ok(format!("+255{local}"))
    } else {
        Err(NOT_MOBILE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_common_spelling_normalises() {
        for raw in [
            "+255712345678",
            "+255 712 345 678",
            "255712345678",
            "0712345678",
            "0712-345-678",
            "(0712) 345.678",
            "712345678",
            " 0712 345 678 ",
        ] {
            assert_eq!(normalize_phone(raw), Ok("+255712345678".into()), "{raw}");
        }
        assert_eq!(normalize_phone("0621 000 111"), Ok("+255621000111".into()));
    }

    #[test]
    fn landlines_foreign_and_malformed_numbers_are_refused() {
        for raw in [
            "+255222112345", // Dar es Salaam landline
            "0222112345",
            "+254712345678", // Kenya
            "+3161234567",
            "07123456",    // too short
            "07123456789", // too long
            "+2557123456789",
            "0712a45678",
            "",
            "+",
            "++255712345678",
        ] {
            assert_eq!(normalize_phone(raw), Err(NOT_MOBILE), "{raw}");
        }
    }
}
