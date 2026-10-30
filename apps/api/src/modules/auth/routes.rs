//! Auth routes, mounted under /v1 (paths match vimdesk-api).

use axum::{Router, routing::post};

use crate::{AppState, modules::auth::handlers};

/// No token needed; lib.rs puts these behind the rate limit.
pub fn public() -> Router<AppState> {
    Router::new()
        .route("/create-workspace", post(handlers::create_workspace))
        .route("/login", post(handlers::login))
}

/// Bearer token required (checked by `AuthUser` in each handler).
pub fn private() -> Router<AppState> {
    Router::new().route("/logout", post(handlers::logout))
}
