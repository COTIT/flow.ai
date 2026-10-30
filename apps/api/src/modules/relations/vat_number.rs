//! VAT number shape check, ported from ibericode/vat 2.1.2 `Validator::validateVatNumberFormat`
//! (what Laravel ValidateVatNumber ran): two-letter prefix + that country's pattern. No VIES call.

/// Pattern alphabet: `d` digit, `A` letter A-Z, `x` digit or letter, `_` optional space, any
/// other char literal. Each country lists its alternatives (ibericode regex, unrolled).
const PATTERNS: [(&str, &[&str]); 29] = [
    ("AT", &["Uxxxxxxxx"]),
    ("BE", &["0ddddddddd", "1ddddddddd"]),
    ("BG", &["ddddddddd", "dddddddddd"]),
    ("CY", &["ddddddddA"]),
    ("CZ", &["dddddddd", "ddddddddd", "dddddddddd"]),
    ("DE", &["ddddddddd"]),
    ("DK", &["dd_dd_dd_dd"]),
    ("EE", &["ddddddddd"]),
    ("EL", &["ddddddddd"]),
    ("ES", &["AdddddddA", "ddddddddA", "Adddddddd"]),
    ("EU", &["ddddddddd"]),
    ("FI", &["dddddddd"]),
    ("FR", &["xxddddddddd"]),
    ("HR", &["ddddddddddd"]),
    ("HU", &["dddddddd"]),
    ("IE", &["dddddddA", "dddddddAA", "dAdddddA"]),
    ("IT", &["ddddddddddd"]),
    ("LT", &["ddddddddd", "dddddddddddd"]),
    ("LU", &["dddddddd"]),
    ("LV", &["ddddddddddd"]),
    ("MT", &["dddddddd"]),
    ("NL", &["dddddddddBdd"]),
    ("PL", &["dddddddddd"]),
    ("PT", &["ddddddddd"]),
    (
        "RO",
        &[
            "dd",
            "ddd",
            "dddd",
            "ddddd",
            "dddddd",
            "ddddddd",
            "dddddddd",
            "ddddddddd",
            "dddddddddd",
        ],
    ),
    ("SE", &["dddddddddddd"]),
    ("SI", &["dddddddd"]),
    ("SK", &["dddddddddd"]),
    ("XI", &["ddd_dddd_dd", "ddd_dddd_dd_ddd", "xxxxx"]),
];

/// ibericode `Countries::getCountryCodesInEU`: the EU states plus territories VAT treats as EU.
const VAT_EU: [&str; 33] = [
    "AT", "AX", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "ES", "FI", "FR", "GF", "GP", "GR", "HR",
    "HU", "IE", "IT", "LT", "LU", "LV", "MQ", "MT", "NL", "PL", "PT", "RE", "RO", "SE", "SI", "SK",
    "YT",
];

fn matches(pattern: &[u8], s: &[u8]) -> bool {
    match (pattern.first(), s.first()) {
        (None, None) => true,
        (Some(b'_'), _) => {
            matches(&pattern[1..], s)
                || (s.first() == Some(&b' ') && matches(&pattern[1..], &s[1..]))
        }
        (None, _) | (_, None) => false,
        (Some(&p), Some(&c)) => {
            let ok = match p {
                b'd' => c.is_ascii_digit(),
                b'A' => c.is_ascii_uppercase(),
                b'x' => c.is_ascii_digit() || c.is_ascii_uppercase(),
                lit => lit == c,
            };
            ok && matches(&pattern[1..], &s[1..])
        }
    }
}

/// `NL123456789B01`, any case: known prefix and the rest fits one of its patterns.
pub fn is_valid_format(vat: &str) -> bool {
    let vat = vat.to_ascii_uppercase();
    let Some((country, number)) = vat.split_at_checked(2) else {
        return false;
    };
    PATTERNS
        .iter()
        .find(|(c, _)| *c == country)
        .is_some_and(|(_, alts)| {
            alts.iter()
                .any(|p| matches(p.as_bytes(), number.as_bytes()))
        })
}

/// ibericode `isCountryCodeInEU` (exact, uppercase codes only, as Laravel passed them).
pub fn in_vat_eu(country: &str) -> bool {
    VAT_EU.binary_search(&country).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_formats_pass_any_case() {
        for ok in [
            "NL123456789B01",
            "nl123456789b01",
            "BE0123456789",
            "DE123456789",
            "DK12 34 56 78",
            "DK12345678",
            "ESX1234567X",
            "ES12345678X",
            "FRAB123456789",
            "IE1234567AB",
            "IE1A12345B",
            "ATU1234567X",
            "RO12",
            "XI123 4567 89",
            "XI123456789012",
            "XIAB123",
            "EU123456789",
        ] {
            assert!(is_valid_format(ok), "rejected {ok}");
        }
    }

    #[test]
    fn wrong_shapes_fail() {
        for bad in [
            "",
            "N",
            "NL",
            "NL123456789B0",
            "NL123456789X01",
            "NL 123456789B01",
            "BE2123456789",
            "DK12  34 56 78",
            "RO1",
            "RO12345678901",
            "GB123456789",
            "US123",
            "NL123456789B01\n",
            "ÑL123456789B01",
        ] {
            assert!(!is_valid_format(bad), "accepted {bad:?}");
        }
    }

    #[test]
    fn vat_eu_list_sorted_and_includes_territories() {
        assert!(VAT_EU.windows(2).all(|w| w[0] < w[1]));
        assert!(in_vat_eu("NL") && in_vat_eu("RE") && in_vat_eu("AX"));
        assert!(!in_vat_eu("GB") && !in_vat_eu("nl") && !in_vat_eu("CH"));
    }
}
