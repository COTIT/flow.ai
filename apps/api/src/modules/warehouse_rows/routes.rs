//! Row routes, mounted under /v1 (paths match vimdesk-api).

use axum::{Router, routing::get};

use crate::{AppState, modules::warehouse_rows::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/warehouses/{warehouse}/sections/{section}/rows";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/{{row}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
}
