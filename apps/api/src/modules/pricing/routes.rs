//! Pricing routes, mounted under /v1 (paths match vimdesk-api Shared/Item routes).

use axum::extract::DefaultBodyLimit;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::pricing::handlers};

/// vimdesk-web posts whole item rows (UI state, nested product) on every edit; 50 lines pass
/// the app-wide 64 KiB. 2 MiB covers `body::MAX_ITEMS` full rows; the parse keeps
/// only the fields a line needs.
const BODY_LIMIT: usize = 2 * 1024 * 1024;

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::calculate_items))
        .routes(routes!(handlers::calculate_marge))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}
