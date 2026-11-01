//! Product image routes. Paths live in each handler's `#[utoipa::path]`.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{AppState, modules::product_images::handlers};

/// Office routes: members read, `product.update` (+ `file.get` for files) replaces.
pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::list, handlers::replace))
}
