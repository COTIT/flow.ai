//! Cash routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::cash::handlers};

/// Till routes (session token) for the drawer, office routes (`cashregister.*`) for vaults
/// and shift history.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::current))
        .routes(routes!(handlers::open))
        .routes(routes!(handlers::close))
        .routes(routes!(handlers::transfer))
        .routes(routes!(handlers::list_vaults, handlers::create_vault))
        .routes(routes!(handlers::list_shifts))
}
