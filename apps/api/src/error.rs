//! AppError: only error type handlers return. Maps to Laravel-compatible JSON bodies.

use std::collections::BTreeMap;

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;

const UNAUTHORIZED: &str = "You are not authorized to perform this action.";
const SERVER_ERROR: &str = "Server error.";
/// Shared with the IP rate limiter so every 429 looks the same.
pub const TOO_MANY: &str = "Too Many Attempts.";

/// 422 wire shape and its OpenAPI schema; one type, so docs and output can't drift.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[schema(description = "Validation failed. `errors` maps each field to its messages.")]
pub struct ValidationErrorBody {
    message: String,
    errors: BTreeMap<String, Vec<String>>,
}

/// 422 schema for routes that can also refuse with `AppError::Unprocessable` (`{message}`
/// only): field errors or a rule's message. Docs only; use as `body = UnprocessableBody`.
pub struct UnprocessableBody;

impl utoipa::PartialSchema for UnprocessableBody {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::{Ref, schema::AnyOfBuilder};
        // anyOf, not oneOf: a validation body also matches MessageBody.
        AnyOfBuilder::new()
            .item(Ref::from_schema_name("ValidationErrorBody"))
            .item(Ref::from_schema_name("MessageBody"))
            .description(Some(
                "Invalid input (`errors` lists fields), or a business rule refused the request (`message` only)",
            ))
            .into()
    }
}

impl utoipa::ToSchema for UnprocessableBody {}

/// Every failure a handler can return. Bodies follow vimdesk-api `app/Exceptions/Handler.php`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 422. Field -> messages, fields sorted by name.
    #[error("{}", first_message(.0))]
    Validation(BTreeMap<String, Vec<String>>),
    /// 401. Missing, malformed, expired or revoked token.
    #[error("unauthenticated")]
    Unauthorized,
    /// 401 on login. Same body for unknown email and wrong password.
    #[error("invalid login details")]
    InvalidCredentials,
    /// 403 with a fixed, client-safe message.
    #[error("{0}")]
    Forbidden(&'static str),
    /// 400 with a client-safe message (Laravel exceptions with `statusCode` 400).
    #[error("{0}")]
    BadRequest(String),
    /// 409 with a client-safe message (Laravel exceptions with `statusCode` 409).
    #[error("{0}")]
    Conflict(&'static str),
    /// 410 with a client-safe message: a one-time thing (a code) that was valid once.
    #[error("{0}")]
    Gone(&'static str),
    /// 422 with only a message (Laravel exceptions with `statusCode` 422, no field errors).
    #[error("{0}")]
    Unprocessable(String),
    /// 503 with a message: a feature whose backing service isn't configured.
    #[error("{0}")]
    Unavailable(&'static str),
    /// 404. Same body for "missing" and "exists but not yours", so ids can't be probed.
    #[error("not found")]
    NotFound,
    /// 413. Body over the router's size limit.
    #[error("payload too large")]
    PayloadTooLarge,
    /// 429. Same body as the IP rate limiter (Laravel ThrottleRequests text).
    #[error("too many requests")]
    TooManyRequests,
    /// 429 as `TooManyRequests`, with `Retry-After`: seconds until a login lock lifts.
    #[error("locked")]
    Locked(u64),
    /// 408: the request body didn't arrive before the body deadline (`http::timeout`).
    #[error("request body timeout")]
    BodyTimeout,
    /// 503 with Retry-After: request slots stayed full (`http::slots`) or the request ran past
    /// the request timeout (`http::timeout`).
    #[error("server busy")]
    Overloaded,
    /// 500. Logged, never sent to client.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// 500 for non-DB failures (hashing, RNG, task join). Logged, never sent.
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    /// A job failure a retry can't fix: bad input, a missing or forbidden target, or a SQL
    /// data or constraint error (SQLSTATE classes 22 and 23).
    pub fn is_permanent(&self) -> bool {
        match self {
            Self::Validation(_)
            | Self::Unauthorized
            | Self::InvalidCredentials
            | Self::Forbidden(_)
            | Self::BadRequest(_)
            | Self::Gone(_)
            | Self::Unprocessable(_)
            | Self::NotFound
            | Self::PayloadTooLarge => true,
            Self::Database(e) => e
                .as_database_error()
                .and_then(|d| d.code())
                .is_some_and(|c| c.starts_with("22") || c.starts_with("23")),
            _ => false,
        }
    }

    /// Postgres chose this transaction as a deadlock victim (40P01): the whole transaction may
    /// be run again.
    pub fn is_deadlock(&self) -> bool {
        matches!(self, Self::Database(e)
            if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("40P01"))
    }

    /// One-field 422 for checks only service can do (e.g. email taken).
    pub fn field(field: &str, message: impl Into<String>) -> Self {
        Self::Validation(BTreeMap::from([(field.to_owned(), vec![message.into()])]))
    }

    /// `e` as the one-field 422 when it violated unique index `constraint` (a race the
    /// service pre-check missed, or no pre-check at all), else a 500.
    pub fn unique(e: sqlx::Error, constraint: &str, field: &str, message: &str) -> Self {
        if e.as_database_error().and_then(|d| d.constraint()) == Some(constraint) {
            Self::field(field, message)
        } else {
            Self::Database(e)
        }
    }
}

const NUL_TEXT: &str = "Text must not contain NUL characters.";
/// 503 body when a row stayed locked past `lock_timeout`, or a deadlock/serialization abort.
const BUSY: &str = "The record is busy, please try again.";
/// 503 body when no pooled connection freed up in time, or a statement hit statement_timeout.
const SERVER_BUSY: &str = "The server is busy, please try again.";

/// lock_timeout (55P03), deadlock (40P01), serialization failure (40001): safe to retry.
fn is_contention(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "55P03" || c == "40P01" || c == "40001")
}

/// Pool exhausted past the acquire timeout, or a statement cancelled by statement_timeout
/// (57014): overload, retry later.
fn is_overload(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::PoolTimedOut)
        || e.as_database_error()
            .and_then(|d| d.code())
            .is_some_and(|c| c == "57014")
}

/// 503 with `Retry-After`, so clients back off instead of re-pinning a connection at once.
fn busy(message: &str) -> Response {
    let mut res = (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "message": message })),
    )
        .into_response();
    res.headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
    res
}

fn is_nul_rejection(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "22021" || c == "22P05")
}

fn first_message(errors: &BTreeMap<String, Vec<String>>) -> &str {
    errors
        .values()
        .flatten()
        .next()
        .map_or("The given data was invalid.", String::as_str)
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            Self::Validation(errors) => {
                // Laravel summary: first message + "(and N more errors)".
                let more = errors
                    .values()
                    .map(Vec::len)
                    .sum::<usize>()
                    .saturating_sub(1);
                let mut message = first_message(&errors).to_owned();
                if more > 0 {
                    let noun = if more == 1 { "error" } else { "errors" };
                    message.push_str(&format!(" (and {more} more {noun})"));
                }
                let body = ValidationErrorBody { message, errors };
                (StatusCode::UNPROCESSABLE_ENTITY, Json(body)).into_response()
            }
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "message": UNAUTHORIZED })),
            )
                .into_response(),
            Self::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                // vimdesk-web matches this exact string to show its own wording.
                Json(json!({ "message": "Invalid login details" })),
            )
                .into_response(),
            Self::Forbidden(message) => {
                (StatusCode::FORBIDDEN, Json(json!({ "message": message }))).into_response()
            }
            Self::BadRequest(message) => {
                (StatusCode::BAD_REQUEST, Json(json!({ "message": message }))).into_response()
            }
            Self::Conflict(message) => {
                (StatusCode::CONFLICT, Json(json!({ "message": message }))).into_response()
            }
            Self::Gone(message) => {
                (StatusCode::GONE, Json(json!({ "message": message }))).into_response()
            }
            Self::Unprocessable(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "message": message })),
            )
                .into_response(),
            Self::Unavailable(message) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "message": message })),
            )
                .into_response(),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                Json(json!({ "message": "The requested resource could not be found." })),
            )
                .into_response(),
            Self::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({ "message": "The request body is too large." })),
            )
                .into_response(),
            Self::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "message": TOO_MANY })),
            )
                .into_response(),
            Self::Locked(secs) => (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, secs.to_string())],
                Json(json!({ "message": TOO_MANY })),
            )
                .into_response(),
            Self::Overloaded => busy(SERVER_BUSY),
            Self::BodyTimeout => (
                StatusCode::REQUEST_TIMEOUT,
                Json(json!({ "message": "The request body took too long to arrive." })),
            )
                .into_response(),
            // Postgres refuses NUL in text (22021) and \u0000 in jsonb (22P05): bad input, not a fault.
            // A row gone under a lock (e.g. a place deleted while a stock write waited).
            Self::Database(sqlx::Error::RowNotFound) => Self::NotFound.into_response(),
            Self::Database(e) if is_nul_rejection(&e) => {
                Self::field("input", NUL_TEXT).into_response()
            }
            Self::Database(e) if is_overload(&e) => {
                // A statement timeout names a slow query to fix; pool waits are load.
                tracing::error!(error = %e, "database overloaded");
                busy(SERVER_BUSY)
            }
            Self::Database(e) if is_contention(&e) => {
                tracing::warn!(error = %e, "lock contention");
                busy(BUSY)
            }
            other => {
                tracing::error!(error = %other, "request failed");
                let body = json!({ "message": SERVER_ERROR });
                (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
    use serde_json::{Value, json};

    async fn render(err: AppError) -> (StatusCode, Value) {
        let res = err.into_response();
        let status = res.status();
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    fn errors(pairs: &[(&str, &[&str])]) -> AppError {
        AppError::Validation(
            pairs
                .iter()
                .map(|(f, msgs)| (f.to_string(), msgs.iter().map(|m| m.to_string()).collect()))
                .collect(),
        )
    }

    #[tokio::test]
    async fn unavailable_is_503_with_a_message() {
        let (status, body) = render(AppError::Unavailable("Off.")).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body, json!({ "message": "Off." }));
    }

    #[tokio::test]
    async fn unprocessable_is_422_with_only_a_message() {
        let (status, body) = render(AppError::Unprocessable("Nope.".into())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body, json!({ "message": "Nope." }));
    }

    #[tokio::test]
    async fn validation_is_422_with_first_message_and_count() {
        let (status, body) = render(errors(&[
            ("email", &["The email field is required."]),
            ("password", &["A.", "B."]),
        ]))
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body,
            json!({
                "message": "The email field is required. (and 2 more errors)",
                "errors": {
                    "email": ["The email field is required."],
                    "password": ["A.", "B."],
                },
            })
        );
    }

    #[tokio::test]
    async fn validation_single_message_has_no_suffix() {
        let (_, body) = render(errors(&[("email", &["Taken."])])).await;

        assert_eq!(body["message"], "Taken.");
    }

    #[tokio::test]
    async fn validation_two_messages_uses_singular_error() {
        let (_, body) = render(errors(&[("email", &["A.", "B."])])).await;

        assert_eq!(body["message"], "A. (and 1 more error)");
    }

    #[tokio::test]
    async fn field_helper_builds_single_error() {
        let (status, body) = render(AppError::field("email", "Taken.")).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["errors"], json!({ "email": ["Taken."] }));
    }

    #[tokio::test]
    async fn unauthorized_is_401_laravel_message() {
        let (status, body) = render(AppError::Unauthorized).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            body,
            json!({ "message": "You are not authorized to perform this action." })
        );
    }

    #[tokio::test]
    async fn invalid_credentials_is_401_exact_frontend_string() {
        let (status, body) = render(AppError::InvalidCredentials).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body, json!({ "message": "Invalid login details" }));
    }

    #[tokio::test]
    async fn forbidden_is_403_with_message() {
        let (status, body) = render(AppError::Forbidden("Nope.")).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body, json!({ "message": "Nope." }));
    }

    #[tokio::test]
    async fn not_found_is_404_laravel_message() {
        let (status, body) = render(AppError::NotFound).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            body,
            json!({ "message": "The requested resource could not be found." })
        );
    }

    #[tokio::test]
    async fn database_error_is_500_without_internals() {
        let err = sqlx::Error::Protocol("secret detail".into());
        let (status, body) = render(AppError::from(err)).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, json!({ "message": "Server error." }));
    }

    #[tokio::test]
    async fn busy_answers_say_when_to_retry() {
        let res = AppError::from(sqlx::Error::PoolTimedOut).into_response();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(res.headers()["retry-after"], "5");
    }

    /// Every pooled connection busy past the acquire timeout: retryable, not a fault.
    #[tokio::test]
    async fn exhausted_pool_is_503() {
        let (status, body) = render(AppError::from(sqlx::Error::PoolTimedOut)).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            json!({ "message": "The server is busy, please try again." })
        );
    }

    #[tokio::test]
    async fn internal_error_is_500_without_internals() {
        let (status, body) = render(AppError::Internal("argon2 blew up".into())).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, json!({ "message": "Server error." }));
    }
}
