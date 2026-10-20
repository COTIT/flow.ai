//! List helpers shared by index endpoints: Laravel paging params + `meta`, ILIKE escaping.

use serde::Serialize;

const DEFAULT_SIZE: i64 = 50;
const MAX_SIZE: i64 = 100;

/// Requested page. Laravel BaseModel: `page_size` default 50, max 100. Junk or missing
/// values fall back instead of failing (vimdesk-web spreads unrelated route query params).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    pub number: i64,
    pub size: i64,
}

impl Page {
    /// From raw `page` / `page_size` query values.
    pub fn parse(page: Option<&str>, size: Option<&str>) -> Self {
        let positive = |v: Option<&str>| {
            v.and_then(|s| s.trim().parse::<i64>().ok())
                .filter(|n| *n > 0)
        };
        Self {
            number: positive(page).unwrap_or(1),
            size: positive(size).map_or(DEFAULT_SIZE, |n| n.min(MAX_SIZE)),
        }
    }

    /// Rows to skip. Saturates, so an absurd page number reads an empty page instead of overflowing.
    pub fn offset(&self) -> i64 {
        (self.number - 1).saturating_mul(self.size)
    }
}

/// `{data, meta}` like Laravel's LengthAwarePaginator (without `links`: vimdesk-web never reads them).
#[derive(Serialize)]
pub struct Paginated<T> {
    data: Vec<T>,
    meta: Meta,
}

#[derive(Serialize, Debug, PartialEq)]
struct Meta {
    current_page: i64,
    /// 1-based index of first row on this page; null when the page is empty.
    from: Option<i64>,
    last_page: i64,
    per_page: i64,
    to: Option<i64>,
    total: i64,
}

impl<T> Paginated<T> {
    /// `total` = rows matching the filter across all pages.
    pub fn new(data: Vec<T>, total: i64, page: Page) -> Self {
        let rows = data.len() as i64;
        let first = page.offset().saturating_add(1);
        Self {
            meta: Meta {
                current_page: page.number,
                from: (rows > 0).then_some(first),
                last_page: (total.max(0) as u64)
                    .div_ceil(page.size.max(1) as u64)
                    .max(1) as i64,
                per_page: page.size,
                to: (rows > 0).then(|| first.saturating_add(rows - 1)),
                total,
            },
            data,
        }
    }
}

/// Laravel `$request->boolean()` on a query param: "1", "true", "on", "yes" (any case) = true.
pub fn flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "on" | "yes"
    )
}

/// `%q%` for `ILIKE`, with `\`, `%` and `_` escaped so user input matches literally.
pub fn contains_pattern(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 2);
    out.push('%');
    for c in q.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_defaults_and_clamps() {
        assert_eq!(
            Page::parse(None, None),
            Page {
                number: 1,
                size: 50
            }
        );
        assert_eq!(
            Page::parse(Some("3"), Some("20")),
            Page {
                number: 3,
                size: 20
            }
        );
        assert_eq!(
            Page::parse(Some("0"), Some("500")),
            Page {
                number: 1,
                size: 100
            }
        );
        assert_eq!(
            Page::parse(Some("-2"), Some("0")),
            Page {
                number: 1,
                size: 50
            }
        );
        assert_eq!(
            Page::parse(Some("abc"), Some("x")),
            Page {
                number: 1,
                size: 50
            }
        );
        assert_eq!(Page::parse(Some("99999999999999999999"), None).number, 1);
    }

    #[test]
    fn offset_is_zero_based() {
        assert_eq!(Page::parse(Some("1"), Some("20")).offset(), 0);
        assert_eq!(Page::parse(Some("3"), Some("20")).offset(), 40);
    }

    #[test]
    fn meta_middle_and_last_page() {
        let p = Paginated::new(vec![(); 20], 45, Page::parse(Some("2"), Some("20")));
        assert_eq!(
            p.meta,
            Meta {
                current_page: 2,
                from: Some(21),
                last_page: 3,
                per_page: 20,
                to: Some(40),
                total: 45
            }
        );
        let p = Paginated::new(vec![(); 5], 45, Page::parse(Some("3"), Some("20")));
        assert_eq!((p.meta.from, p.meta.to), (Some(41), Some(45)));
    }

    #[test]
    fn meta_empty_list_has_one_page_and_null_bounds() {
        let p = Paginated::<()>::new(vec![], 0, Page::parse(None, None));
        assert_eq!(
            p.meta,
            Meta {
                current_page: 1,
                from: None,
                last_page: 1,
                per_page: 50,
                to: None,
                total: 0
            }
        );
    }

    #[test]
    fn absurd_page_number_does_not_overflow() {
        let page = Page::parse(Some("9223372036854775807"), None);

        let p = Paginated::<()>::new(vec![], 3, page);

        assert_eq!((p.meta.from, p.meta.to, p.meta.last_page), (None, None, 1));
    }

    #[test]
    fn flag_follows_php_filter_validate_boolean() {
        for yes in ["1", "true", "TRUE", " on ", "yes"] {
            assert!(flag(yes), "{yes}");
        }
        for no in ["0", "false", "", "2", "no", "off"] {
            assert!(!flag(no), "{no}");
        }
    }

    #[test]
    fn pattern_escapes_wildcards() {
        assert_eq!(contains_pattern("ab"), "%ab%");
        assert_eq!(contains_pattern("50%_off\\"), "%50\\%\\_off\\\\%");
    }
}
