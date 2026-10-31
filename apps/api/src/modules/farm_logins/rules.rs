//! Farmer password rule (NIST 800-63B style): length, not guessable, no composition rule.
//! Staff passwords keep `auth::password::violations`.

const MIN_CHARS: usize = 8;
const MAX_CHARS: usize = 128;
/// Refused whatever the case. Kept short: the lockout and IP limit carry the rest.
const OBVIOUS: &[&str] = &[
    "password",
    "password1",
    "passw0rd",
    "nenosiri",
    "qwertyui",
    "qwerty123",
    "iloveyou",
    "123456789",
    "1234567890",
    "asdfghjk",
];

/// Why `password` is refused for a farmer with `phone`, or None when it is fine.
pub fn problem(password: &str, phone: Option<&str>) -> Option<&'static str> {
    let chars: Vec<char> = password.to_lowercase().chars().collect();
    if chars.len() < MIN_CHARS {
        return Some("The password must be at least 8 characters.");
    }
    if chars.len() > MAX_CHARS {
        return Some("The password must not be greater than 128 characters.");
    }
    if chars.iter().all(|c| *c == chars[0]) {
        return Some("The password must not be one character repeated.");
    }
    let run = |step: i64| {
        chars
            .windows(2)
            .all(|w| i64::from(u32::from(w[1])) - i64::from(u32::from(w[0])) == step)
    };
    if run(1) || run(-1) {
        return Some("The password must not be a straight run like 12345678 or abcdefgh.");
    }
    let lower: String = chars.iter().collect();
    if OBVIOUS.contains(&lower.as_str()) {
        return Some("The password is too easy to guess.");
    }
    let digits: String = phone
        .unwrap_or("")
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    if digits.len() >= 7 && lower.contains(&digits[digits.len() - 7..]) {
        return Some("The password must not contain your phone number.");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHONE: Option<&str> = Some("+255712000001");

    #[test]
    fn refuses_short_long_repeated_runs_obvious_and_the_phone() {
        let long = "x".repeat(129);
        for bad in [
            "short",
            "1234567",
            &long,
            "aaaaaaaa",
            "AAAAAAAAAA",
            "12345678",
            "87654321",
            "abcdefgh",
            "HGFEDCBA",
            "password",
            "Password",
            "NENOSIRI",
            "mama2000001",
            "0712000001",
        ] {
            assert!(problem(bad, PHONE).is_some(), "{bad}");
        }
    }

    #[test]
    fn accepts_words_and_phrases_without_composition_rules() {
        let long_ok = "y".repeat(127) + "z";
        for good in ["kahawa ya asubuhi", "maji safi 2026", "mahindi9", &long_ok] {
            assert_eq!(problem(good, PHONE), None, "{good}");
        }
        // Without a phone on file the phone rule can't apply.
        assert_eq!(problem("mama2000001", None), None);
    }
}
