//! Invite routes, mounted under /v1 (paths match vimdesk-api). Paths live in each handler's
//! `#[utoipa::path]`, so router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::invites::handlers};

/// No token needed; lib.rs puts these behind the rate limit.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::status))
        .routes(routes!(handlers::register))
}

/// Bearer token required (checked by `AuthUser` in each handler).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::send, handlers::list))
        .routes(routes!(handlers::delete))
        .routes(routes!(handlers::answer))
}
