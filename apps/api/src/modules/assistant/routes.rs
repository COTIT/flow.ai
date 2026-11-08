//! Assistant routes; control operations never enter model tool inventory.

use super::{batch_handlers, gaps, handlers};
use crate::AppState;
use utoipa_axum::{router::OpenApiRouter, routes};

/// Ordinary-bearer controls, with extra signature checks on trusted bridge mutations.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(batch_handlers::create))
        .routes(routes!(batch_handlers::get))
        .routes(routes!(batch_handlers::history))
        .routes(routes!(batch_handlers::approve))
        .routes(routes!(batch_handlers::start))
        .routes(routes!(batch_handlers::advance))
        .routes(routes!(batch_handlers::pause))
        .routes(routes!(batch_handlers::cancel))
        .routes(routes!(handlers::create_session))
        .routes(routes!(handlers::get_session, handlers::close_session))
        .routes(routes!(handlers::heartbeat))
        .routes(routes!(handlers::attach))
        .routes(routes!(handlers::propose))
        .routes(routes!(handlers::action))
        .routes(routes!(handlers::approve))
        .routes(routes!(handlers::cancel))
        .routes(routes!(handlers::history))
        .routes(routes!(handlers::reserve, handlers::uploads))
        .routes(routes!(handlers::complete))
        .routes(routes!(handlers::cancel_upload, handlers::upload))
        .routes(routes!(gaps::create))
        .routes(routes!(handlers::price_import))
}

/// Signature-only internal cleanup, independent of user token lifetime.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::cleanup_due))
        .routes(routes!(handlers::cleanup_ack))
        .routes(routes!(gaps::follow_up))
}
