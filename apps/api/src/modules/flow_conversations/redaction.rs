//! Archive text redaction for native confirmations and recognizable transport credentials.

/// Remove authorization phrases and recognizable secrets before durable storage.
pub fn archive_text(text: &str) -> String {
    let approvals = replace(text, |text, start| {
        if previous_word(text, start, true) {
            return None;
        }
        for prefix in [
            "Confirm action",
            "Confirm batch",
            "Bevestig actie",
            "Bevestig reeks",
            "Thibitisha kitendo",
            "Thibitisha kundi",
        ] {
            let Some(end) = prefix_end(text, start, prefix, true) else {
                continue;
            };
            let digits = whitespace_end(text, end);
            if digits == end {
                continue;
            }
            let Some(code) = text.as_bytes().get(digits..digits + 6) else {
                continue;
            };
            if code.iter().all(u8::is_ascii_digit) && !next_word(text, digits + 6, true) {
                return Some((digits + 6, "[approval redacted]"));
            }
        }
        None
    });
    let bearer = replace(&approvals, |text, start| {
        if previous_word(text, start, true) {
            return None;
        }
        let end = prefix_end(text, start, "Bearer", true)?;
        let token = whitespace_end(text, end);
        if token == end {
            return None;
        }
        let mut end = token;
        for c in text[token..].chars() {
            if word(c, true) || matches!(c, '.' | '~' | '+' | '-') {
                end += c.len_utf8();
            } else {
                break;
            }
        }
        (end > token).then_some((end, "[credential redacted]"))
    });
    replace(&bearer, |text, start| {
        if previous_word(text, start, false) || !text[start..].starts_with("sk-") {
            return None;
        }
        let mut end = start + 3;
        for c in text[end..].chars() {
            if word(c, false) || c == '-' {
                end += c.len_utf8();
            } else {
                break;
            }
        }
        while end > start + 3 && !previous_word(text, end, false) {
            end -= 1;
        }
        (end >= start + 19 && !next_word(text, end, false))
            .then_some((end, "[credential redacted]"))
    })
}
fn replace(text: &str, matcher: impl Fn(&str, usize) -> Option<(usize, &'static str)>) -> String {
    let mut output = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if let Some((end, replacement)) = matcher(text, at) {
            output.push_str(replacement);
            at = end;
        } else if let Some(c) = text[at..].chars().next() {
            output.push(c);
            at += c.len_utf8();
        }
    }
    output
}
fn prefix_end(text: &str, start: usize, prefix: &str, folded: bool) -> Option<usize> {
    let mut end = start;
    let mut chars = text[start..].chars();
    for wanted in prefix.chars() {
        let c = chars.next()?;
        if c != wanted
            && !(folded
                && (c.to_lowercase().eq(wanted.to_lowercase())
                    || c.to_uppercase().eq(wanted.to_uppercase())))
        {
            return None;
        }
        end += c.len_utf8();
    }
    Some(end)
}
fn whitespace_end(text: &str, start: usize) -> usize {
    start
        + text[start..]
            .chars()
            .take_while(|c| js_whitespace(*c))
            .map(char::len_utf8)
            .sum::<usize>()
}
/// JavaScript whitespace set keeps native and webhook normalization identical.
pub(super) fn js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn word(c: char, folded: bool) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || folded && matches!(c, '\u{17f}' | '\u{212a}')
}
fn previous_word(text: &str, at: usize, folded: bool) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_some_and(|c| word(c, folded))
}
fn next_word(text: &str, at: usize, folded: bool) -> bool {
    text[at..].chars().next().is_some_and(|c| word(c, folded))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_approval_and_credentials_match_office_redaction() {
        for phrase in [
            "Confirm action",
            "Confirm batch",
            "Bevestig actie",
            "Bevestig reeks",
            "Thibitisha kitendo",
            "Thibitisha kundi",
        ] {
            assert_eq!(
                archive_text(&format!("Before {phrase} 123456 after")),
                "Before [approval redacted] after"
            );
        }
        assert_eq!(
            archive_text("cOnFiRm AcTiOn\t123456! Bearer AbC-_.~+123 sk-abcdefghijklmnop"),
            "[approval redacted]! [credential redacted] [credential redacted]"
        );
        for text in [
            "Confirm action 1234567",
            "xConfirm action 123456",
            "Confirm action 123456x",
            "sk-short",
            "myBearer abc",
        ] {
            assert_eq!(archive_text(text), text);
        }
        assert_eq!(
            archive_text("漢字 Confirm batch\u{feff}123456 😊"),
            "漢字 [approval redacted] 😊"
        );
    }
}
