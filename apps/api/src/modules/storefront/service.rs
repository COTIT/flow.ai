//! Public catalog materialization shared by storefront reads and design previews.

use crate::{
    AppError, AppState,
    modules::{
        product_images, settings,
        storefront::{
            dto::{LOW_STOCK_DEFAULT, StorefrontProduct},
            repo,
        },
    },
};
use uuid::Uuid;

/// Existing catalog query, pricing, images and stock rules; bounded by caller's limit.
pub async fn products(
    state: &AppState,
    workspace_id: Uuid,
    only: Option<Uuid>,
    category: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<StorefrontProduct>, i64), AppError> {
    let rows = repo::products(&state.db, workspace_id, only, category, limit, offset).await?;
    let total = match rows.first() {
        Some(r) => r.total,
        None if offset > 0 => repo::count(&state.db, workspace_id, category).await?,
        None => 0,
    };
    let ids: Vec<_> = rows.iter().map(|r| r.id).collect();
    let images = product_images::service::storefront_links(state, workspace_id, &ids).await?;
    let low = settings::repo::section(&state.db, workspace_id, "product")
        .await?
        .and_then(|s| s.get("low_stock_threshold")?.as_i64())
        .unwrap_or(LOW_STOCK_DEFAULT);
    Ok((
        rows.into_iter()
            .map(|r| StorefrontProduct::new(r, &images, low))
            .collect(),
        total,
    ))
}
