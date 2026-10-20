//! Phone normalization (Laravel Infrastructure\Support\PhoneNumberNormalizer): `06 12345678`
//! with an NL address becomes `+31612345678`. Pure string rules, NL/BE/DE only.

use serde_json::Value;

use crate::modules::relations::address::Address;

/// Countries whose national `0` prefix is rewritten, with their calling code.
const SUPPORTED: [(&str, &str); 3] = [("NL", "31"), ("BE", "32"), ("DE", "49")];

fn calling_code(country: Option<&str>) -> Option<&'static str> {
    let country = country?.trim().to_ascii_uppercase();
    SUPPORTED
        .iter()
        .find(|(c, _)| *c == country)
        .map(|(_, code)| *code)
}

/// Canonical form: `00`/`+` prefixes -> `+digits`, a national `0` -> `+<code>` when the
/// country is supported, else bare digits. Blank or digit-free input -> None.
pub fn normalize(number: Option<&str>, country: Option<&str>) -> Option<String> {
    let number = number?.trim();
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    if digits.len() > 2
        && let Some(rest) = digits.strip_prefix("00")
    {
        return Some(format!("+{rest}"));
    }
    if number.starts_with('+') {
        return Some(format!("+{digits}"));
    }
    if let (Some(code), Some(rest)) = (calling_code(country), digits.strip_prefix('0')) {
        return Some(format!("+{code}{rest}"));
    }
    Some(digits)
}

/// `phones[].number` normalized in place; non-object items pass through untouched.
pub fn normalize_phones(phones: &mut [Value], country: Option<&str>) {
    for phone in phones {
        if let Value::Object(p) = phone {
            let number = normalize(p.get("number").and_then(Value::as_str), country);
            p.insert("number".into(), number.map_or(Value::Null, Value::String));
        }
    }
}

/// Country of the main address, else the first address; only supported countries count.
pub fn country_of(addresses: &[Address]) -> Option<&'static str> {
    let address = addresses
        .iter()
        .find(|a| a.main_address)
        .or(addresses.first())?;
    let country = address.country.as_deref()?.trim().to_ascii_uppercase();
    SUPPORTED
        .iter()
        .find(|(c, _)| *c == country)
        .map(|(c, _)| *c)
}

/// Stored spellings a typed phone number may match: bare digits, `+`/`00`/`0` variants for
/// NL/BE/DE. Fewer than 7 digits -> none (too short to search on).
pub fn search_candidates(number: &str, country: Option<&str>) -> Vec<String> {
    let number = number.trim();
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 7 {
        return Vec::new();
    }
    let mut out = vec![digits.clone()];
    push_variants(&mut out, normalize(Some(number), country));
    for (_, code) in SUPPORTED {
        if let Some(rest) = digits.strip_prefix(code).filter(|r| !r.is_empty()) {
            out.push(format!("+{digits}"));
            out.push(format!("00{digits}"));
            out.push(format!("0{rest}"));
        }
    }
    if digits.starts_with('0') {
        for (country, _) in SUPPORTED {
            push_variants(&mut out, normalize(Some(number), Some(country)));
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| c.chars().filter(char::is_ascii_digit).count() >= 7 && seen.insert(c.clone()));
    out
}

/// `+31612…` -> itself, `31612…`, `0031612…`, `0612…`.
fn push_variants(out: &mut Vec<String>, canonical: Option<String>) {
    let Some(canonical) = canonical else { return };
    let Some(digits) = canonical.strip_prefix('+').map(str::to_owned) else {
        out.push(canonical);
        return;
    };
    out.push(canonical);
    out.push(format!("00{digits}"));
    for (_, code) in SUPPORTED {
        if let Some(rest) = digits.strip_prefix(code).filter(|r| !r.is_empty()) {
            out.push(format!("0{rest}"));
        }
    }
    out.push(digits);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn national_number_gets_country_code() {
        assert_eq!(
            normalize(Some("06 12345678"), Some("NL")).as_deref(),
            Some("+31612345678")
        );
        assert_eq!(
            normalize(Some("0471 12 34 56"), Some("be")).as_deref(),
            Some("+32471123456")
        );
        assert_eq!(
            normalize(Some("030-1234567"), Some("DE")).as_deref(),
            Some("+49301234567")
        );
    }

    #[test]
    fn international_prefixes_win_over_country() {
        assert_eq!(
            normalize(Some("0031 6 1234"), Some("DE")).as_deref(),
            Some("+3161234")
        );
        assert_eq!(
            normalize(Some("+31 (0)6-1234"), None).as_deref(),
            Some("+31061234")
        );
        assert_eq!(
            normalize(Some("+44 20 7946"), Some("NL")).as_deref(),
            Some("+44207946")
        );
    }

    #[test]
    fn unsupported_or_missing_country_keeps_digits() {
        assert_eq!(
            normalize(Some("06 12345678"), Some("FR")).as_deref(),
            Some("0612345678")
        );
        assert_eq!(
            normalize(Some("06 12345678"), None).as_deref(),
            Some("0612345678")
        );
        assert_eq!(
            normalize(Some("12 34"), Some("NL")).as_deref(),
            Some("1234")
        );
    }

    #[test]
    fn blank_or_digit_free_is_none() {
        for v in [None, Some(""), Some("   "), Some("abc"), Some("+")] {
            assert_eq!(normalize(v, Some("NL")), None, "{v:?}");
        }
        // "00" alone is two digits: not an international prefix.
        assert_eq!(normalize(Some("00"), Some("NL")).as_deref(), Some("+310"));
    }

    #[test]
    fn phones_normalized_in_place() {
        let mut phones = vec![
            json!({"type": "mobile", "number": "06 12345678"}),
            json!({"type": "home", "number": ""}),
            json!("junk"),
        ];

        normalize_phones(&mut phones, Some("NL"));

        assert_eq!(
            phones,
            vec![
                json!({"type": "mobile", "number": "+31612345678"}),
                json!({"type": "home", "number": null}),
                json!("junk"),
            ]
        );
    }

    #[test]
    fn country_from_main_address_else_first() {
        let addr = |country: &str, main: bool| Address {
            country: Some(country.into()),
            main_address: main,
            ..Address::default()
        };

        assert_eq!(
            country_of(&[addr("de", false), addr("NL", true)]),
            Some("NL")
        );
        assert_eq!(
            country_of(&[addr("be", false), addr("NL", false)]),
            Some("BE")
        );
        assert_eq!(country_of(&[addr("FR", true)]), None);
        assert_eq!(country_of(&[]), None);
    }

    #[test]
    fn search_candidates_cover_stored_spellings() {
        let c = search_candidates("06 12345678", None);

        for want in [
            "0612345678",
            "+31612345678",
            "0031612345678",
            "31612345678",
            "+32612345678",
        ] {
            assert!(c.contains(&want.to_owned()), "{want} missing from {c:?}");
        }
        assert_eq!(
            c.len(),
            c.iter().collect::<std::collections::HashSet<_>>().len()
        );
        assert!(search_candidates("+31612345678", None).contains(&"0612345678".to_owned()));
        assert!(search_candidates("123456", None).is_empty());
    }
}
