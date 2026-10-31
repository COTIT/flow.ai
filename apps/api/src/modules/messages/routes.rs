//! Mail routes, mounted under /v1 (paths match vimdesk-api).

use axum::extract::DefaultBodyLimit;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::messages::handlers};

/// A mail body is the user's HTML (pasted signatures and images included).
const SEND_LIMIT: usize = 1024 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    let send = OpenApiRouter::new()
        .routes(routes!(handlers::send_message))
        .layer(DefaultBodyLimit::max(SEND_LIMIT));
    OpenApiRouter::new()
        .merge(send)
        .routes(routes!(handlers::logs))
        .routes(routes!(handlers::office365_connection))
        .routes(routes!(handlers::office365_send))
        .routes(routes!(handlers::profile_test))
}
