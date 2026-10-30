//! Contact routes, nested under their relation (paths match vimdesk-api).

use axum::{Router, routing::post};

use crate::{AppState, modules::contacts::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/relations/{relation}/contacts";
    Router::new().route(base, post(handlers::create)).route(
        &format!("{base}/{{contact}}"),
        post(handlers::update).delete(handlers::delete),
    )
}
