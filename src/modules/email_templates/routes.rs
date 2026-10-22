//! Email template routes, mounted under /v1 (paths match vimdesk-api; update is POST).

use axum::{Router, routing::get};

use crate::{AppState, modules::email_templates::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/email-templates";
    Router::new()
        .route(base, get(handlers::list).post(handlers::create))
        .route(
            &format!("{base}/variables/{{domain}}"),
            get(handlers::variables),
        )
        .route(
            &format!("{base}/{{template}}"),
            get(handlers::get)
                .post(handlers::update)
                .delete(handlers::delete),
        )
}
