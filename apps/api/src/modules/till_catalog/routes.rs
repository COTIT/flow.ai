//! Till catalog routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::till_catalog::handlers};

/// Till routes (session token).
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::list))
}
