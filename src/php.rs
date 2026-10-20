//! PHP string semantics ported Laravel code relies on: truthiness and `ucfirst`.

/// PHP truthiness of an optional string: set, non-empty and not "0".
pub fn truthy(s: Option<&str>) -> bool {
    s.is_some_and(|s| !s.is_empty() && s != "0")
}

/// PHP `ucfirst`: uppercase the first char when it is an ASCII letter.
pub fn ucfirst(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_truthiness_and_ucfirst() {
        assert!(truthy(Some("a")) && truthy(Some("00")) && truthy(Some(" ")));
        assert!(!truthy(None) && !truthy(Some("")) && !truthy(Some("0")));
        assert_eq!(ucfirst("john"), "John");
        assert_eq!(ucfirst("élan"), "élan");
        assert_eq!(ucfirst(""), "");
    }
}
