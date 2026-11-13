//! Durable dialogue route wiring and bounded ingestion bodies.

use super::handlers;
use crate::AppState;
use axum::extract::DefaultBodyLimit;
use utoipa_axum::{router::OpenApiRouter, routes};
/// Member controls, with signed immutable source binding.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::create, handlers::list))
        .routes(routes!(handlers::get, handlers::delete))
        .routes(routes!(handlers::end))
        .routes(routes!(handlers::source))
        .routes(routes!(handlers::messages))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(handlers::events))
                .layer(DefaultBodyLimit::max(128 * 1024)),
        )
}
/// Signature-only canonical webhook ingestion independent from expired login.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::ingest))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
}
