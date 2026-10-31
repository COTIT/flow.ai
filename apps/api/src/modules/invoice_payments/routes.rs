//! Invoice payment routes, mounted under /v1 (paths match vimdesk-api).

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::invoice_payments::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::create))
        .routes(routes!(handlers::void))
        .routes(routes!(handlers::mark_as_credited))
}
