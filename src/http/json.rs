//! ValidatedJson extractor + Rules: request body parse + Laravel-style field validation in one step.

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{FromRequest, Request},
    http::StatusCode,
};
use serde::{Deserialize, de::DeserializeOwned};

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
                "The {} field must not be greater than {max} characters.",
                human(field)
            );
            self.add(field, msg);
        }
    }

    pub fn email(&mut self, field: &str, value: &str) {
        if !is_email(value) {
            let msg = format!("The {} field must be a valid email address.", human(field));
            self.add(field, msg);
        }
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
            json!(["The email field must be a valid email address."])
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
            json!(["The name field must not be greater than 5 characters."])
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
}
