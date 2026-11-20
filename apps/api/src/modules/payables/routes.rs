//! Payables routes, mounted under /v1. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::payables::handlers};

/// Routes for this module; each handler runs the workspace permission guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::bills_list, handlers::bills_create))
        .routes(routes!(handlers::bills_get, handlers::bills_update))
        .routes(routes!(handlers::bills_match))
        .routes(routes!(handlers::bills_match_candidates))
        .routes(routes!(handlers::bills_landed_candidates))
        .routes(routes!(handlers::bills_post_review))
        .routes(routes!(handlers::bills_post))
        .routes(routes!(handlers::bills_reverse_review))
        .routes(routes!(handlers::bills_reverse))
        .routes(routes!(handlers::payments_list, handlers::payments_create))
        .routes(routes!(handlers::payments_create_review))
        .routes(routes!(handlers::payments_get))
        .routes(routes!(handlers::payments_targets))
        .routes(routes!(handlers::payments_allocate_review))
        .routes(routes!(handlers::payments_allocate))
        .routes(routes!(handlers::payments_reverse_review))
        .routes(routes!(handlers::payments_reverse))
        .routes(routes!(handlers::returns_list, handlers::returns_create))
        .routes(routes!(handlers::returns_candidates))
        .routes(routes!(handlers::returns_get))
        .routes(routes!(handlers::returns_post_review))
        .routes(routes!(handlers::returns_post))
        .routes(routes!(handlers::returns_reverse_review))
        .routes(routes!(handlers::returns_reverse))
        .routes(routes!(handlers::costs_list, handlers::costs_create))
        .routes(routes!(handlers::costs_get))
        .routes(routes!(handlers::costs_post_review))
        .routes(routes!(handlers::costs_post))
        .routes(routes!(handlers::costs_reverse_review))
        .routes(routes!(handlers::costs_reverse))
}
