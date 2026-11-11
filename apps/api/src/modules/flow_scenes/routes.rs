//! Your Flow scene routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::flow_scenes::handlers};

/// Member routes; each handler runs the workspace guard and reaches own rows only.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list))
        .routes(routes!(handlers::reset))
        .routes(routes!(handlers::update, handlers::delete))
}
