//! Scheduled report routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::scheduled_reports::handlers};

/// Member routes; each handler runs the workspace guard and reaches own rows only.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::update, handlers::delete))
}

/// The mail link's unsubscribe, behind the public rate limiter.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::unsubscribe))
}
