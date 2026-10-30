//! Request time limits on `/v1`: the body must arrive within its deadline (a slow upload gets
//! 408), the whole request must finish within the request timeout (503). Either way the
//! request's slots are released and an open transaction rolls back; a statement already sent
//! keeps its pooled connection until it ends (statement and lock timeouts bound that).

use std::time::Duration;

use axum::{
    Router,
    extract::Request,
    middleware::{self, Next},
    response::IntoResponse,
};
use tower_http::timeout::{RequestBodyDeadlineLayer, TimeoutError};

use crate::{AppState, error::AppError};

/// The whole request body of `router`'s routes must arrive within `deadline` (counted from when
/// the request reaches them), else 408.
pub fn body_deadline(router: Router<AppState>, deadline: Duration) -> Router<AppState> {
    router.layer(RequestBodyDeadlineLayer::new(deadline))
}

/// A request must finish within `timeout`, else it is cancelled and answers 503.
pub fn request_timeout(router: Router<AppState>, timeout: Duration) -> Router<AppState> {
    router.layer(middleware::from_fn(
        move |req: Request, next: Next| async move {
            match tokio::time::timeout(timeout, next.run(req)).await {
                Ok(res) => res,
                Err(_) => {
                    tracing::warn!(?timeout, "request timed out");
                    AppError::Overloaded.into_response()
                }
            }
        },
    ))
}

/// True when `e` (or an error under it) is the body deadline firing.
pub fn is_body_timeout(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut cur = Some(e);
    while let Some(err) = cur {
        if err.is::<TimeoutError>() {
            return true;
        }
        cur = err.source();
    }
    false
}
