//! Approval routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::approvals::handlers};

/// Member routes; row rules (four-eyes, approve vs approve_high) run in the service.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list))
        .routes(routes!(handlers::get))
        .routes(routes!(handlers::approve))
        .routes(routes!(handlers::reject))
        .routes(routes!(handlers::return_request))
        .routes(routes!(handlers::resubmit))
        .routes(routes!(handlers::policies, handlers::create_policy))
        .routes(routes!(handlers::update_policy))
}
