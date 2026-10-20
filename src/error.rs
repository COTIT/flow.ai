//! AppError: only error type handlers return. Maps to Laravel-compatible JSON bodies.

use std::collections::BTreeMap;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

const UNAUTHORIZED: &str = "You are not authorized to perform this action.";
const SERVER_ERROR: &str = "Server error.";
/// Shared with the IP rate limiter so every 429 looks the same.
pub const TOO_MANY: &str = "Too Many Attempts.";

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
    /// 404. Same body for "missing" and "exists but not yours", so ids can't be probed.
    #[error("not found")]
    NotFound,
    /// 413. Body over the router's size limit.
    #[error("payload too large")]
    PayloadTooLarge,
    /// 429. Same body as the IP rate limiter (Laravel ThrottleRequests text).
    #[error("too many requests")]
    TooManyRequests,
    /// 500. Logged, never sent to client.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// 500 for non-DB failures (hashing, RNG, task join). Logged, never sent.
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    /// One-field 422 for checks only service can do (e.g. email taken).
    pub fn field(field: &str, message: impl Into<String>) -> Self {
        Self::Validation(BTreeMap::from([(field.to_owned(), vec![message.into()])]))
    }
}

const NUL_TEXT: &str = "Text must not contain NUL characters.";

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
                let body = json!({ "message": message, "errors": errors });
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
            // Postgres refuses NUL in text (22021) and \u0000 in jsonb (22P05): bad input, not a fault.
            Self::Database(e) if is_nul_rejection(&e) => {
                Self::field("input", NUL_TEXT).into_response()
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
        let (status, body) = render(AppError::from(sqlx::Error::PoolTimedOut)).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, json!({ "message": "Server error." }));
    }

    #[tokio::test]
    async fn internal_error_is_500_without_internals() {
        let (status, body) = render(AppError::Internal("argon2 blew up".into())).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, json!({ "message": "Server error." }));
    }
}
