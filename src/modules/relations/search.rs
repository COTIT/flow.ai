//! Relation search input (Laravel ListRelation): `q` parsing, identifier detection, Dutch
//! zipcode form. Pure; repo::list turns the result into bound SQL parameters.

use uuid::Uuid;

use crate::{
    error::AppError,
    http::{json::Rules, list::contains_pattern},
    modules::relations::phone,
};

/// What one `q` asks for. Empty `terms` with `emptied` = nothing left after cleanup.
#[derive(Debug, Default, PartialEq)]
pub struct Query {
    /// `%term%` ILIKE patterns: one per comma-separated phrase.
    pub patterns: Vec<String>,
    /// Per phrase, `%word%` patterns that must all hit a contact name column.
    pub words: Vec<Vec<String>>,
    /// Exact-match candidates for id-like input.
    pub uuid: Option<Uuid>,
    pub text: Option<String>,
    pub number: Option<i64>,
    /// Stored spellings of a phone-like input, exact and as `%..%` patterns.
    pub phones: Vec<String>,
    pub phone_patterns: Vec<String>,
}

/// Split on commas outside quotes, trim spaces and quotes, drop `null`/`undefined` tokens
/// left by template strings (a one-word term stays, so a relation named "null" is findable).
fn terms(raw: &str) -> Vec<String> {
    let mut segments = vec![String::new()];
    let mut quoted = false;
    for c in raw.trim().chars() {
        match c {
            ',' if !quoted => segments.push(String::new()),
            _ => {
                quoted ^= c == '"';
                if let Some(s) = segments.last_mut() {
                    s.push(c);
                }
            }
        }
    }
    segments
        .into_iter()
        .map(|s| {
            s.trim_matches([' ', '\t', '\n', '\r', '\0', '\x0B', '"'])
                .to_owned()
        })
        .filter(|s| !s.is_empty())
        .map(|term| {
            let tokens: Vec<&str> = term.split_whitespace().collect();
            if tokens.len() <= 1 {
                return term.trim().to_owned();
            }
            tokens
                .into_iter()
                .filter(|t| !t.eq_ignore_ascii_case("null") && !t.eq_ignore_ascii_case("undefined"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// Laravel isIdentifierLikeSearch: uuid, 4+ digits, or a code with digits/separators.
fn identifier_like(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    if Uuid::parse_str(s).is_ok() && s.len() == 36 {
        return true;
    }
    if s.len() >= 4 && s.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    let b = s.as_bytes();
    let code_chars = |c: u8| c.is_ascii_alphanumeric() || b"._/-".contains(&c);
    if b.len() < 3 || !b[0].is_ascii_alphanumeric() || !b[1..].iter().all(|c| code_chars(*c)) {
        return false;
    }
    let alnum = b.iter().all(u8::is_ascii_alphanumeric);
    if alnum
        && b.len() >= 5
        && b.iter().any(u8::is_ascii_alphabetic)
        && b.iter().any(u8::is_ascii_digit)
    {
        return true;
    }
    b.iter().any(u8::is_ascii_digit) || s.contains(['-', '_', '/', '.'])
}

/// Laravel relationNumberFromSearch: `12`, `RE-0012`, `re 12 x` -> 12.
fn relation_number(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() || s.contains('@') {
        return None;
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        return s.parse().ok();
    }
    // ^letters [-_/ space]+ 0* digits ([-_/ space]* letters)?$
    let letters = s.bytes().take_while(u8::is_ascii_alphabetic).count();
    let rest = &s[letters..];
    let seps = rest.bytes().take_while(|c| b"-_/ \t".contains(c)).count();
    if letters == 0 || seps == 0 {
        return None;
    }
    let rest = &rest[seps..];
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let tail = rest[digits..].trim_start_matches(['-', '_', '/', ' ', '\t']);
    if !tail.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    rest[..digits].parse().ok()
}

/// Laravel formatDutchZipcode, compacted for comparison: `1339 nm` -> `1339NM`.
pub fn compact_zipcode(zip: &str) -> Option<String> {
    let z: String = zip
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_uppercase();
    (!z.is_empty()).then_some(z)
}

/// `q` -> search parameters; None = nothing searchable left (Laravel answers an empty page).
/// Over 255 chars is a 422: duplicate checks send whole email addresses quoted.
pub fn parse(q: Option<&str>) -> Result<Option<Query>, AppError> {
    let raw = q.map(str::trim).unwrap_or("");
    if raw.is_empty() || raw == "*" {
        return Ok(Some(Query::default()));
    }
    let mut r = Rules::default();
    r.max_chars("q", raw, 255);
    r.into_result()?;
    let exact = raw.contains('"');
    let terms = terms(raw);
    if terms.is_empty() {
        return Ok(None);
    }
    let search = terms.join(" ");
    let phrases = if exact { terms } else { vec![search.clone()] };
    let phones = phone::search_candidates(&search, None);
    let id_like = identifier_like(&search);
    Ok(Some(Query {
        patterns: phrases.iter().map(|p| contains_pattern(p)).collect(),
        words: phrases
            .iter()
            .map(|p| p.split_whitespace().map(contains_pattern).collect())
            .collect(),
        uuid: id_like.then(|| Uuid::parse_str(&search).ok()).flatten(),
        text: id_like.then(|| search.clone()),
        number: id_like.then(|| relation_number(&search)).flatten(),
        phone_patterns: phones.iter().map(|p| contains_pattern(p)).collect(),
        phones,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_split_on_unquoted_commas() {
        assert_eq!(terms(" a, b "), vec!["a", "b"]);
        assert_eq!(terms("\"x, y\", z"), vec!["x, y", "z"]);
        assert_eq!(terms("\"info@cotit.eu\""), vec!["info@cotit.eu"]);
        assert_eq!(terms("john null"), vec!["john"]);
        assert_eq!(terms("null"), vec!["null"]);
        assert!(terms("null undefined").is_empty());
        assert!(terms(" , ").is_empty());
    }

    #[test]
    fn identifier_like_matches_laravel_cases() {
        for yes in [
            "0190a2b4-0000-7000-8000-000000000001",
            "1234",
            "RE-0002",
            "ab12c",
            "doe.nl",
            "x_y_z",
            "a1b",
            "123",
        ] {
            assert!(identifier_like(yes), "{yes}");
        }
        for no in ["", "ab", "john doe", "abc", "a@b.nl", "12", "50%"] {
            assert!(!identifier_like(no), "{no}");
        }
    }

    #[test]
    fn relation_number_from_codes() {
        assert_eq!(relation_number("12"), Some(12));
        assert_eq!(relation_number("RE-0012"), Some(12));
        assert_eq!(relation_number("re 12 x"), Some(12));
        assert_eq!(relation_number("RE-12-X"), Some(12));
        assert_eq!(relation_number("a@1"), None);
        assert_eq!(relation_number("RE12"), None);
        assert_eq!(relation_number("RE-12a1"), None);
    }

    #[test]
    fn zipcode_compacted() {
        assert_eq!(compact_zipcode(" 1339 nm ").as_deref(), Some("1339NM"));
        assert_eq!(compact_zipcode("B-1000").as_deref(), Some("B1000"));
        assert_eq!(compact_zipcode(" - "), None);
    }

    #[test]
    fn parse_builds_bound_parameters() {
        let q = parse(Some("RE-0002")).unwrap().unwrap();
        assert_eq!(q.patterns, vec!["%RE-0002%"]);
        assert_eq!((q.number, q.text.as_deref()), (Some(2), Some("RE-0002")));
        let phrase = parse(Some("\"a b\", c")).unwrap().unwrap();
        assert_eq!(phrase.patterns, vec!["%a b%", "%c%"]);
        assert_eq!(phrase.words, vec![vec!["%a%", "%b%"], vec!["%c%"]]);
        assert_eq!(
            parse(Some("50%")).unwrap().unwrap().patterns,
            vec!["%50\\%%"]
        );
        assert!(parse(Some("*")).unwrap().unwrap().patterns.is_empty());
        assert_eq!(parse(Some("null undefined")).unwrap(), None);
        assert!(parse(Some(&"x".repeat(256))).is_err());
        assert!(
            !parse(Some("06 12345678"))
                .unwrap()
                .unwrap()
                .phones
                .is_empty()
        );
    }
}
