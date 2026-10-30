//! Email verification routes, mounted under /v1.

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::email_verification::handlers};

/// No token needed; lib.rs puts these behind the rate limit.
pub fn public() -> Router<AppState> {
    Router::new().route("/verify-email/{token}", get(handlers::verify))
}

/// Bearer token required (checked by `AuthUser` in each handler).
pub fn private() -> Router<AppState> {
    Router::new().route("/email/verification-notification", post(handlers::resend))
}
