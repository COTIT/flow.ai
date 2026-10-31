//! Packing and transfer routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::supply_chain::handlers};

/// Writes need `product.update`; the transfer list needs membership.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::pack))
        .routes(routes!(handlers::dispatch))
        .routes(routes!(handlers::list))
        .routes(routes!(handlers::receive))
}
