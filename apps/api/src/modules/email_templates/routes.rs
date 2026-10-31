//! Email template routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::email_templates::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list, handlers::create))
        .routes(routes!(handlers::variables))
        .routes(routes!(handlers::get, handlers::update, handlers::delete))
}
