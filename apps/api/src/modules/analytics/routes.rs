//! Analytics routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::analytics::handlers};

/// Office routes; each handler runs the workspace guard and its list permission.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::top_customers))
        .routes(routes!(handlers::top_products))
        .routes(routes!(handlers::compare))
        .routes(routes!(handlers::daily))
        .routes(routes!(handlers::invoices_export))
}
