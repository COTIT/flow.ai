//! Invite routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, modules::invites::handlers};

/// No token needed; lib.rs puts these behind the rate limit.
pub fn public() -> Router<AppState> {
    Router::new()
        .route("/invites/get-invite-status/{code}", get(handlers::status))
        .route("/register", post(handlers::register))
}

/// Bearer token required (checked by `AuthUser` in each handler).
pub fn private() -> Router<AppState> {
    Router::new()
        .route("/workspace/{workspace}/invites", post(handlers::send))
        .route("/invites", post(handlers::answer))
}
