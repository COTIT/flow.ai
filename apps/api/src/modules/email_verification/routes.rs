//! Email verification routes, mounted under /v1. Paths live in each handler's
//! `#[utoipa::path]`, so router and OpenAPI spec share one source.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::email_verification::handlers};

/// No token needed; lib.rs puts these behind the rate limit.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::verify))
}

/// Bearer token required (checked by `AuthUser` in each handler).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::resend))
}
