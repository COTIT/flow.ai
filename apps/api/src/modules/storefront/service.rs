//! Public catalog materialization shared by storefront reads and design previews.

use crate::{
    AppError, AppState,
    modules::{
        product_images, settings,
        storefront::{
            dto::{LOW_STOCK_DEFAULT, StorefrontProduct},
            repo,
        },
        tax_profiles::sale,
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
    let mut out: Vec<StorefrontProduct> = rows
        .into_iter()
        .map(|r| StorefrontProduct::new(r, &images, low))
        .collect();
    active_gross(state, workspace_id, &mut out).await?;
    Ok((out, total))
}

/// Active books: one unit's gross from the verified tax path, as checkout charges quantity 1.
async fn active_gross(
    state: &AppState,
    ws: Uuid,
    out: &mut [StorefrontProduct],
) -> Result<(), AppError> {
    let mut conn = state.db.acquire().await?;
    if out.is_empty() || !crate::modules::stock::value::active(&mut conn, ws).await? {
        return Ok(());
    }
    let (mut products, mut amounts) = (Vec::new(), Vec::new());
    for p in out.iter() {
        for net in std::iter::once(p.price_net).chain(p.variants.iter().map(|v| v.price_net)) {
            products.push(Some(p.id));
            amounts.push(net);
        }
    }
    let t = sale::lines(&mut conn, ws, sale::Facts::default(), &products, &amounts).await?;
    let mut gross = t.lines.into_iter().map(|l| l.gross);
    for p in out.iter_mut() {
        p.price_gross = gross.next().unwrap_or(p.price_gross);
        for v in &mut p.variants {
            v.price_gross = gross.next().unwrap_or(v.price_gross);
        }
    }
    Ok(())
}
