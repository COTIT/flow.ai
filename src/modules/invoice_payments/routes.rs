//! Invoice payment routes, mounted under /v1 (paths match vimdesk-api).

use axum::{
    Router,
    routing::{delete, post},
};

use crate::{AppState, modules::invoice_payments::handlers};

/// Routes for this module; each handler runs the workspace guard.
pub fn private() -> Router<AppState> {
    let base = "/workspace/{workspace}/invoices/{invoice}";
    Router::new()
        .route(&format!("{base}/payments"), post(handlers::create))
        .route(
            &format!("{base}/payments/{{payment}}"),
            delete(handlers::void),
        )
        .route(
            &format!("{base}/mark-as-credited"),
            post(handlers::mark_as_credited),
        )
}
