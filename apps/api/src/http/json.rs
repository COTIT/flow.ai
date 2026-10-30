//! ValidatedJson extractor + Rules: request body parse + Laravel-style field validation in one step.

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{FromRequest, Request},
    http::StatusCode,
};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::error::AppError;

/// Request body -> validated input. Every body DTO implements it; ValidatedJson runs it.
pub trait Validate {
    /// Typed input handed to the handler, required fields no longer `Option`.
    type Valid;

    /// Record failures in `r`. Return value is only used when `r` stays empty,
    /// so build it whenever required fields are present.
    fn validate(self, r: &mut Rules) -> Option<Self::Valid>;
}

/// Collects field errors worded like Laravel's validator.
#[derive(Default)]
pub struct Rules {
    errors: BTreeMap<String, Vec<String>>,
}

impl Rules {
    /// Record a custom message.
    pub fn add(&mut self, field: &str, message: impl Into<String>) {
        self.errors
            .entry(field.to_owned())
            .or_default()
            .push(message.into());
    }

    /// Trimmed value if present and non-blank, else records "required" and returns None.
    pub fn required(&mut self, field: &str, value: Option<String>) -> Option<String> {
        let value = optional(value);
        if value.is_none() {
            self.missing(field);
        }
        value
    }

    /// Record Laravel "required" message.
    pub fn missing(&mut self, field: &str) {
        self.add(field, format!("The {} field is required.", human(field)));
    }

    /// Record `message` when `ok` is false.
    pub fn check(&mut self, field: &str, ok: bool, message: impl Into<String>) {
        if !ok {
            self.add(field, message);
        }
    }

    /// Max length in chars (not bytes), like Laravel `max:N` on strings.
    pub fn max_chars(&mut self, field: &str, value: &str, max: usize) {
        if value.chars().count() > max {
            let msg = format!(
                "The {} must not be greater than {max} characters.",
                human(field)
            );
            self.add(field, msg);
        }
    }

    pub fn email(&mut self, field: &str, value: &str) {
        if !is_email(value) {
            let msg = format!("The {} must be a valid email address.", human(field));
            self.add(field, msg);
        }
    }

    /// Required trimmed text, at most `max` chars. Missing gives "" plus the error (a 422
    /// discards the value anyway).
    pub fn text(&mut self, field: &str, value: Option<String>, max: usize) -> String {
        let v = self.required(field, value).unwrap_or_default();
        self.max_chars(field, &v, max);
        v
    }

    /// Optional trimmed text, at most `max` chars; blank = None.
    pub fn opt_text(&mut self, field: &str, value: Option<String>, max: usize) -> Option<String> {
        let v = optional(value);
        if let Some(v) = &v {
            self.max_chars(field, v, max);
        }
        v
    }

    /// Patch of a NOT NULL text column: absent = keep (None), null or blank = "required".
    pub fn patch_text(
        &mut self,
        field: &str,
        value: Option<Option<String>>,
        max: usize,
    ) -> Option<String> {
        value.map(|v| self.text(field, v, max))
    }

    /// Patch of a nullable text column: absent = keep (None), null or blank = clear (Some(None)).
    pub fn patch_opt_text(
        &mut self,
        field: &str,
        value: Option<Option<String>>,
        max: usize,
    ) -> Option<Option<String>> {
        value.map(|v| self.opt_text(field, v, max))
    }

    /// Sent, non-blank value, else "required".
    pub fn required_value(&mut self, field: &str, value: Option<Value>) -> Option<Value> {
        let v = filled(value);
        if v.is_none() {
            self.missing(field);
        }
        v
    }

    /// ISO 3166-1 alpha-2 code (any case) -> uppercase, else Laravel ValidateCountry's
    /// ":attribute is not valid".
    pub fn country(&mut self, field: &str, value: &str) -> Option<String> {
        if is_country(value) {
            Some(value.to_ascii_uppercase())
        } else {
            self.add(field, format!("{} is not valid", human(field)));
            None
        }
    }

    /// Laravel `numeric|min|max` on a sent value: JSON number or plain decimal string -> exact
    /// `Decimal`. Number wording follows vimdesk-api `resources/lang/en/validation.php`.
    /// Stricter than Laravel on purpose: no exponent form, at most 8 decimals (columns are
    /// `numeric(24,8)`; MySQL rounded silently), so float noise is a 422, not a stored value.
    pub fn decimal(
        &mut self,
        field: &str,
        value: &Value,
        min: Decimal,
        max: Decimal,
    ) -> Option<Decimal> {
        let f = human(field);
        let text = match value {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.trim().to_owned(),
            _ => String::new(),
        };
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(&text)),
        };
        let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
        if (int.is_empty() && frac.is_empty())
            || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit())
        {
            self.add(field, format!("The {f} must be a number."));
            return None;
        }
        if frac.len() > 8 {
            self.add(field, format!("The {f} must have 0-8 decimal places."));
            return None;
        }
        let int = int.trim_start_matches('0');
        let parsed = (int.len() <= 16)
            .then(|| {
                let sign = if negative { "-" } else { "" };
                let int = if int.is_empty() { "0" } else { int };
                Decimal::from_str_exact(&format!("{sign}{int}.{frac}0")).ok()
            })
            .flatten();
        let Some(v) = parsed.map(|v| v.normalize()) else {
            // Too many integer digits: past `min` when negative, else past `max`.
            let msg = if negative {
                format!("The {f} must be at least {}.", min.normalize())
            } else {
                format!("The {f} must not be greater than {}.", max.normalize())
            };
            self.add(field, msg);
            return None;
        };
        if v < min {
            let min = min.normalize();
            self.add(field, format!("The {f} must be at least {min}."));
            return None;
        }
        if v > max {
            let max = max.normalize();
            self.add(field, format!("The {f} must not be greater than {max}."));
            return None;
        }
        Some(v)
    }

    /// Laravel `nullable|in:...`: blank = None, else one of `allowed`. `message` replaces
    /// "The selected X is invalid." where Laravel had a custom text.
    pub fn one_of(
        &mut self,
        field: &str,
        value: Option<String>,
        allowed: &[&str],
        message: Option<&str>,
    ) -> Option<String> {
        let v = optional(value)?;
        if allowed.contains(&v.as_str()) {
            return Some(v);
        }
        let msg = message.map_or_else(
            || format!("The selected {} is invalid.", human(field)),
            str::to_owned,
        );
        self.add(field, msg);
        None
    }

    /// Laravel `numeric|min` on a measurement Laravel kept as a double (meters, m3): exact
    /// parse via `decimal`, then a JSON number.
    pub fn number(&mut self, field: &str, value: &Value, min: Decimal) -> Option<f64> {
        self.decimal(field, value, min, crate::money::MAX)
            .and_then(|d| d.to_f64())
    }

    /// Laravel `boolean`: exactly true, false, 1, 0, "1", "0".
    pub fn boolean(&mut self, field: &str, value: &Value) -> Option<bool> {
        let b = match value {
            Value::Bool(b) => Some(*b),
            Value::Number(n) => match n.as_i64() {
                Some(1) => Some(true),
                Some(0) => Some(false),
                _ => None,
            },
            Value::String(s) if s == "1" => Some(true),
            Value::String(s) if s == "0" => Some(false),
            _ => None,
        };
        if b.is_none() {
            let msg = format!("The {} field must be true or false.", human(field));
            self.add(field, msg);
        }
        b
    }

    /// Laravel `integer|min|max`: whole JSON number or digit string.
    pub fn integer(&mut self, field: &str, value: &Value, min: i64, max: i64) -> Option<i64> {
        let f = human(field);
        let n = match value {
            Value::Number(n) => n.as_i64().or_else(|| {
                n.as_f64()
                    .filter(|x| x.fract() == 0.0 && x.abs() < 9e15)
                    .map(|x| x as i64)
            }),
            Value::String(s) => s.trim().parse::<i64>().ok(),
            _ => None,
        };
        match n {
            None => self.add(field, format!("The {f} must be an integer.")),
            Some(n) if n < min => self.add(field, format!("The {f} must be at least {min}.")),
            Some(n) if n > max => {
                self.add(field, format!("The {f} must not be greater than {max}."))
            }
            Some(n) => return Some(n),
        }
        None
    }

    /// `meta`: a JSON object, or null/absent = none (Laravel MetaRule meant a keyed array).
    pub fn meta(&mut self, value: Option<Value>) -> Option<Option<Value>> {
        self.meta_at("meta", value)
    }

    /// `meta` rule for a nested object's meta (`contact.meta`).
    pub fn meta_at(&mut self, key: &str, value: Option<Value>) -> Option<Option<Value>> {
        match value {
            None | Some(Value::Null) => Some(None),
            Some(object @ Value::Object(_)) => Some(Some(object)),
            Some(_) => {
                self.add(key, format!("The {} must be an array.", human(key)));
                None
            }
        }
    }

    /// `custom_sections` / `custom_section_forms`: custom fields aren't ported, so null or a
    /// list is fine, but any item naming a `section_id` is unknown (Laravel's own message).
    // shortcut: custom fields not ported; replace with a real section lookup when they are.
    pub fn custom_sections(&mut self, key: &str, v: Option<&Value>) {
        match v {
            None | Some(Value::Null) => {}
            Some(Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    if filled(item.get("section_id").cloned()).is_some() {
                        self.add(
                            &format!("{key}.{i}.section_id"),
                            "this custom section does not exist for the selected domain",
                        );
                    }
                }
            }
            Some(_) => self.add(key, format!("The {} must be an array.", human(key))),
        }
    }

    /// Move these errors into `into`; true when there were any. Lets a nested value be
    /// checked on its own and stop at the first bad item.
    pub fn merge_into(self, into: &mut Rules) -> bool {
        let had = !self.errors.is_empty();
        for (field, messages) in self.errors {
            into.errors.entry(field).or_default().extend(messages);
        }
        had
    }

    /// `merge_into` with `prefix` before every field key (messages keep the bare name, as
    /// Laravel's nested value validators reported them).
    pub fn merge_into_prefixed(self, into: &mut Rules, prefix: &str) -> bool {
        let had = !self.errors.is_empty();
        for (field, messages) in self.errors {
            into.errors
                .entry(format!("{prefix}{field}"))
                .or_default()
                .extend(messages);
        }
        had
    }

    /// Ok when no rule failed, else 422 with every message.
    pub fn into_result(self) -> Result<(), AppError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation(self.errors))
        }
    }
}

/// `#[serde(default, deserialize_with = "present")]` on an `Option<Option<T>>` field:
/// absent key -> None, `null` -> Some(None), value -> Some(Some(v)). Partial updates need
/// "not sent" apart from "cleared".
pub fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Trimmed value, blank -> None. Laravel TrimStrings + ConvertEmptyStringsToNull.
pub fn optional(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// Sent value with null and blank strings dropped (Laravel ConvertEmptyStringsToNull).
pub fn filled(value: Option<Value>) -> Option<Value> {
    value.filter(|v| !v.is_null() && !v.as_str().is_some_and(|s| s.trim().is_empty()))
}

/// ISO 3166-1 alpha-2 codes (league/iso3166, which Laravel ValidateCountry used), sorted.
pub const COUNTRIES: [&str; 249] = [
    "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AO", "AQ", "AR", "AS", "AT", "AU", "AW", "AX", "AZ",
    "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ", "BR", "BS",
    "BT", "BV", "BW", "BY", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN",
    "CO", "CR", "CU", "CV", "CW", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE",
    "EG", "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FM", "FO", "FR", "GA", "GB", "GD", "GE", "GF",
    "GG", "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT", "GU", "GW", "GY", "HK", "HM",
    "HN", "HR", "HT", "HU", "ID", "IE", "IL", "IM", "IN", "IO", "IQ", "IR", "IS", "IT", "JE", "JM",
    "JO", "JP", "KE", "KG", "KH", "KI", "KM", "KN", "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC",
    "LI", "LK", "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MH", "MK",
    "ML", "MM", "MN", "MO", "MP", "MQ", "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA",
    "NC", "NE", "NF", "NG", "NI", "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG",
    "PH", "PK", "PL", "PM", "PN", "PR", "PS", "PT", "PW", "PY", "QA", "RE", "RO", "RS", "RU", "RW",
    "SA", "SB", "SC", "SD", "SE", "SG", "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "SS",
    "ST", "SV", "SX", "SY", "SZ", "TC", "TD", "TF", "TG", "TH", "TJ", "TK", "TL", "TM", "TN", "TO",
    "TR", "TT", "TV", "TW", "TZ", "UA", "UG", "UM", "US", "UY", "UZ", "VA", "VC", "VE", "VG", "VI",
    "VN", "VU", "WF", "WS", "YE", "YT", "ZA", "ZM", "ZW",
];

/// ISO 3166-1 alpha-2 code, any case.
pub fn is_country(code: &str) -> bool {
    COUNTRIES
        .binary_search(&code.to_ascii_uppercase().as_str())
        .is_ok()
}

/// Stored IBAN form: ASCII spaces dropped, ASCII uppercase.
pub fn normalize_iban(value: &str) -> String {
    value
        .chars()
        .filter(|c| *c != ' ')
        .collect::<String>()
        .to_ascii_uppercase()
}

/// IBAN: ASCII only, 15..=34 alphanumerics (spaces ignored), country letters, ISO 7064
/// mod-97 == 1. ASCII check first: Unicode uppercasing turns `ﬆ` into `ST`.
// shortcut: no per-country length table (Laravel had one); ~1/97 wrong-length typos pass.
pub fn is_iban(value: &str) -> bool {
    if !value.is_ascii() {
        return false;
    }
    let s = normalize_iban(value.trim());
    if !(15..=34).contains(&s.len())
        || !s.chars().all(|c| c.is_ascii_alphanumeric())
        || !s[..2].chars().all(|c| c.is_ascii_alphabetic())
        || !s[2..4].chars().all(|c| c.is_ascii_digit())
    {
        return false;
    }
    // Move the first 4 chars to the end, letters -> 10..35, then mod 97 digit by digit.
    let rotated = s[4..].chars().chain(s[..4].chars());
    let rem = rotated.fold(0u32, |acc, c| {
        let n = c.to_digit(36).unwrap_or(0);
        if n >= 10 {
            (acc * 100 + n) % 97
        } else {
            (acc * 10 + n) % 97
        }
    });
    rem == 1
}

/// BIC/SWIFT: 4 letters bank, 2 letters country, 2 alnum location, optional 3 alnum branch.
pub fn is_bic(value: &str) -> bool {
    let b = value.trim().as_bytes();
    (b.len() == 8 || b.len() == 11)
        && b[..6].iter().all(u8::is_ascii_alphabetic)
        && b[6..].iter().all(u8::is_ascii_alphanumeric)
}

/// International phone number: leading `+`, 8..=15 digits, separators ` -().` only.
// shortcut: shape check only, Laravel ran libphonenumber validity; port it if bad numbers slip in.
pub fn is_phone(value: &str) -> bool {
    let v = value.trim();
    let digits = v.chars().filter(char::is_ascii_digit).count();
    v.starts_with('+')
        && (8..=15).contains(&digits)
        && v[1..]
            .chars()
            .all(|c| c.is_ascii_digit() || " -().".contains(c))
}

/// `http(s)://host[:port][...]` with a dotted host. No userinfo, whitespace, control chars,
/// quotes or angle brackets: these URLs end up on PDFs and in emails.
// shortcut: http(s) only; Laravel `url` also took ftp:// etc. Widen if a client sends them.
pub fn is_url(value: &str) -> bool {
    let v = value.trim();
    if v.chars()
        .any(|c| c.is_whitespace() || c.is_control() || "\"'<>`\\".contains(c))
    {
        return false;
    }
    let rest = v
        .strip_prefix("https://")
        .or_else(|| v.strip_prefix("http://"));
    rest.is_some_and(|r| {
        let authority = r.split(['/', '?', '#']).next().unwrap_or("");
        if authority.contains('@') {
            return false;
        }
        let (host, port) = authority.split_once(':').unwrap_or((authority, ""));
        let port_ok = if authority.contains(':') {
            (1..=5).contains(&port.len()) && port.bytes().all(|b| b.is_ascii_digit())
        } else {
            true
        };
        port_ok && host.contains('.') && !host.starts_with('.') && !host.ends_with('.')
    })
}

/// `first_name` -> `first name`, matching Laravel attribute names in messages.
pub fn human(field: &str) -> String {
    field.replace('_', " ")
}

/// Structural email check: one `@`, local part 1..=64, dotted domain with no empty labels,
/// no whitespace/control chars (blocks header injection in outgoing mail).
// shortcut: no RFC 5322 quoted local parts or DNS MX lookup (Laravel had email:rfc,dns), add if bounces demand.
fn is_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !domain.contains('@')
        && domain.split('.').count() >= 2
        && domain.split('.').all(|label| !label.is_empty())
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
        // Mail sender must accept the exact same string, else invite rows get written for
        // addresses that can't be delivered (or parse to a different address).
        && value.parse::<lettre::Address>().is_ok()
}

/// `Json<T>` + `T::validate` in one extractor, yields `T::Valid`.
/// Body parse failures also become 422, under field `body`.
pub struct ValidatedJson<T: Validate>(pub T::Valid);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, AppError> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rejection| match rejection.status() {
                StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
                _ if crate::http::timeout::is_body_timeout(&rejection) => AppError::BodyTimeout,
                _ => AppError::field("body", rejection.body_text()),
            })?;
        let mut rules = Rules::default();
        let valid = value.validate(&mut rules);
        rules.into_result()?;
        valid.map(Self).ok_or_else(|| {
            AppError::Internal("validate gave no input yet recorded no error".into())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        routing::post,
    };
    use serde::Deserialize;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    #[derive(Deserialize)]
    struct Probe {
        email: Option<String>,
        name: Option<String>,
    }

    impl Validate for Probe {
        type Valid = String;

        fn validate(self, r: &mut Rules) -> Option<String> {
            if let Some(email) = r.required("email", self.email) {
                r.email("email", &email);
            }
            let name = r.required("name", self.name)?;
            r.max_chars("name", &name, 5);
            Some(name)
        }
    }

    async fn post_json(body: &str) -> (StatusCode, Value) {
        let app = Router::new().route(
            "/probe",
            post(|ValidatedJson(name): ValidatedJson<Probe>| async move { name }),
        );
        let res = app
            .oneshot(
                Request::post("/probe")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let value = serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, value)
    }

    #[tokio::test]
    async fn valid_body_reaches_handler() {
        let (status, body) = post_json(r#"{"email":"a@b.co","name":"Ada"}"#).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "Ada");
    }

    #[tokio::test]
    async fn missing_fields_all_reported() {
        let (status, body) = post_json("{}").await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body["errors"],
            json!({
                "email": ["The email field is required."],
                "name": ["The name field is required."],
            })
        );
    }

    #[tokio::test]
    async fn blank_string_counts_as_missing() {
        let (_, body) = post_json(r#"{"email":"   ","name":"Ada"}"#).await;

        assert_eq!(
            body["errors"]["email"],
            json!(["The email field is required."])
        );
    }

    #[tokio::test]
    async fn bad_email_rejected() {
        let (_, body) = post_json(r#"{"email":"not-an-email","name":"Ada"}"#).await;

        assert_eq!(
            body["errors"]["email"],
            json!(["The email must be a valid email address."])
        );
    }

    #[tokio::test]
    async fn max_chars_boundary_counts_chars_not_bytes() {
        let (at_limit, _) = post_json(r#"{"email":"a@b.co","name":"ééééé"}"#).await;
        let (over, body) = post_json(r#"{"email":"a@b.co","name":"ééééé!"}"#).await;

        assert_eq!(at_limit, StatusCode::OK);
        assert_eq!(over, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body["errors"]["name"],
            json!(["The name must not be greater than 5 characters."])
        );
    }

    #[tokio::test]
    async fn malformed_json_is_422_body_error() {
        let (status, body) = post_json(r#"{"email":"#).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["errors"]["body"].is_array(), "{body}");
    }

    #[tokio::test]
    async fn wrong_type_is_422_body_error() {
        let (status, body) = post_json(r#"{"email":"a@b.co","name":5}"#).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["errors"]["body"].is_array(), "{body}");
    }

    #[test]
    fn email_shape_checks() {
        for ok in ["a@b.co", "first.last+tag@sub.example.nl"] {
            assert!(is_email(ok), "rejected {ok}");
        }
        for bad in [
            "ab.co",
            "a@b",
            "@b.co",
            "a@.co",
            "a@b.",
            "a b@c.co",
            "a@b.co\n",
            "a@@b.co",
            "a@b..co",
            // Must also parse as a lettre Address, else mail send fails after the invite row is written.
            "a\"b@x.co",
            "a,b@x.co",
            "x<y@z.com>",
            "<y@z.com>",
        ] {
            assert!(!is_email(bad), "accepted {bad:?}");
        }
    }

    #[test]
    fn field_names_humanized_like_laravel() {
        let mut r = Rules::default();
        r.required("first_name", None);

        assert_eq!(
            r.into_result().unwrap_err().to_string(),
            "The first name field is required."
        );
    }

    #[test]
    fn add_pushes_custom_message() {
        let mut r = Rules::default();
        r.add("email", "Taken.");
        r.add("email", "Also bad.");

        match r.into_result().unwrap_err() {
            AppError::Validation(errors) => {
                assert_eq!(errors["email"], vec!["Taken.", "Also bad."]);
            }
            other => panic!("expected validation, got {other:?}"),
        }
    }

    #[test]
    fn iban_checks_mod97() {
        assert!(is_iban("NL91ABNA0417164300"));
        assert!(is_iban("nl91 abna 0417 1643 00"));
        assert!(is_iban("DE89370400440532013000"));
        assert!(!is_iban("NL00ABNA0417164300"));
        assert!(!is_iban("NL91"));
        assert!(!is_iban("9191ABNA0417164300"));
        // Unicode uppercasing must not turn a ligature into valid ASCII.
        assert!(!is_iban("GB82WE\u{FB06}12345698765432"));
        assert!(!is_iban("NL91\u{A0}ABNA0417164300"));
        assert_eq!(
            normalize_iban(" nl91 abna 0417 1643 00 "),
            "NL91ABNA0417164300"
        );
    }

    #[test]
    fn bic_shape() {
        assert!(is_bic("ABNANL2A") && is_bic("DEUTDEFF500"));
        assert!(!is_bic("AB") && !is_bic("ABNA1L2A") && !is_bic("ABNANL2A5"));
    }

    #[test]
    fn phone_needs_plus_and_8_to_15_digits() {
        assert!(is_phone("+31 20 123 4567") && is_phone("+33123456789"));
        assert!(!is_phone("0201234567") && !is_phone("+12") && !is_phone("+31 abc 1234567"));
    }

    #[test]
    fn url_needs_scheme_and_dotted_host() {
        assert!(is_url("https://acme.nl") && is_url("http://a.io/x?y=1"));
        assert!(!is_url("acme.nl") && !is_url("https://localhost") && !is_url("https://a .io"));
        assert!(!is_url("https://u:p@a.io") && !is_url("https://a.io:abc"));
        assert!(!is_url("https://a.io/\"x") && !is_url("https://a.io/<script>"));
        assert!(is_url("https://a.io:8443/p"));
    }
}

#[cfg(test)]
mod value_rule_tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json::{Value, json};
    use std::str::FromStr;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn errors(r: Rules) -> BTreeMap<String, Vec<String>> {
        match r.into_result() {
            Ok(()) => BTreeMap::new(),
            Err(AppError::Validation(e)) => e,
            Err(other) => panic!("{other:?}"),
        }
    }

    fn decimal(v: Value) -> (Option<Decimal>, BTreeMap<String, Vec<String>>) {
        let mut r = Rules::default();
        let out = r.decimal("vat_amount", &v, dec("0"), dec("100"));
        (out, errors(r))
    }

    #[test]
    fn decimal_takes_numbers_and_numeric_strings() {
        for (v, want) in [
            (json!(21), "21"),
            (json!(12.5), "12.5"),
            (json!("9.25"), "9.25"),
            (json!(" 21 "), "21"),
            (json!("0.00000001"), "0.00000001"),
            (json!(".5"), "0.5"),
            (json!("100"), "100"),
            (json!("+7"), "7"),
        ] {
            let (out, e) = decimal(v.clone());
            assert!(e.is_empty(), "{v}: {e:?}");
            assert_eq!(out, Some(dec(want)), "{v}");
        }
    }

    #[test]
    fn decimal_rejects_junk_with_laravel_wording() {
        for v in [
            json!("abc"),
            json!("1e3"),
            json!("1_000"),
            json!("1.2.3"),
            json!("-"),
            json!(true),
            json!([1]),
        ] {
            let (out, e) = decimal(v.clone());
            assert_eq!(out, None);
            assert_eq!(e["vat_amount"], ["The vat amount must be a number."], "{v}");
        }
    }

    #[test]
    fn decimal_bounds_and_scale() {
        assert_eq!(
            decimal(json!("-0.01")).1["vat_amount"],
            ["The vat amount must be at least 0."]
        );
        assert_eq!(
            decimal(json!("100.00000001")).1["vat_amount"],
            ["The vat amount must not be greater than 100."]
        );
        assert_eq!(
            decimal(json!("1.123456789")).1["vat_amount"],
            ["The vat amount must have 0-8 decimal places."]
        );
        // Wider than numeric(24,8) never reaches Decimal parsing.
        let mut r = Rules::default();
        let huge = r.decimal(
            "price",
            &json!("12345678901234567890123456789012"),
            dec("0"),
            crate::money::MAX,
        );
        assert_eq!(huge, None);
        let mut neg = Rules::default();
        neg.decimal(
            "price",
            &json!("-12345678901234567890"),
            dec("0"),
            crate::money::MAX,
        );
        assert_eq!(errors(neg)["price"], ["The price must be at least 0."]);
        assert_eq!(
            errors(r)["price"],
            ["The price must not be greater than 9999999999999999.99999999."]
        );
    }

    #[test]
    fn boolean_accepts_laravel_set_only() {
        for (v, want) in [
            (json!(true), true),
            (json!(false), false),
            (json!(1), true),
            (json!(0), false),
            (json!("1"), true),
            (json!("0"), false),
        ] {
            let mut r = Rules::default();
            assert_eq!(r.boolean("active", &v), Some(want), "{v}");
            assert!(errors(r).is_empty());
        }
        for v in [json!("true"), json!("yes"), json!(2), json!("")] {
            let mut r = Rules::default();
            assert_eq!(r.boolean("active", &v), None, "{v}");
            assert_eq!(
                errors(r)["active"],
                ["The active field must be true or false."]
            );
        }
    }

    #[test]
    fn integer_whole_numbers_in_range() {
        let mut r = Rules::default();
        assert_eq!(r.integer("decimals", &json!("2"), 0, 8), Some(2));
        assert_eq!(r.integer("decimals", &json!(8), 0, 8), Some(8));
        assert_eq!(r.integer("decimals", &json!(2.0), 0, 8), Some(2));
        assert!(errors(r).is_empty());

        for (v, msg) in [
            (json!(2.5), "The decimals must be an integer."),
            (json!("x"), "The decimals must be an integer."),
            (json!(9), "The decimals must not be greater than 8."),
            (json!(-1), "The decimals must be at least 0."),
        ] {
            let mut r = Rules::default();
            assert_eq!(r.integer("decimals", &v, 0, 8), None, "{v}");
            assert_eq!(errors(r)["decimals"], [msg], "{v}");
        }
    }

    #[test]
    fn meta_object_or_null() {
        let mut r = Rules::default();
        assert_eq!(r.meta(None), Some(None));
        assert_eq!(r.meta(Some(Value::Null)), Some(None));
        assert_eq!(r.meta(Some(json!({"a": 1}))), Some(Some(json!({"a": 1}))));
        assert!(errors(r).is_empty());

        let mut r = Rules::default();
        assert_eq!(r.meta(Some(json!([1]))), None);
        assert_eq!(errors(r)["meta"], ["The meta must be an array."]);
    }

    #[test]
    fn filled_treats_blank_as_missing() {
        assert_eq!(filled(None), None);
        assert_eq!(filled(Some(Value::Null)), None);
        assert_eq!(filled(Some(json!("  "))), None);
        assert_eq!(filled(Some(json!(0))), Some(json!(0)));
        assert_eq!(filled(Some(json!(false))), Some(json!(false)));
    }

    #[test]
    fn country_codes_iso_alpha2_any_case() {
        assert_eq!(COUNTRIES.len(), 249);
        assert!(COUNTRIES.windows(2).all(|w| w[0] < w[1]), "sorted");
        for ok in ["NL", "nl", "Be", "GB", "AX", "ZW"] {
            assert!(is_country(ok), "{ok}");
        }
        for bad in ["NO_LAND", "UK", "EU", "N", "NLD", "", "  "] {
            assert!(!is_country(bad), "{bad}");
        }
    }
}
