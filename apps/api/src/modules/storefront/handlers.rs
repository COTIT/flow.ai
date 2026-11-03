//! HTTP edge for /v1/storefront: apps read the catalog of the workspace their key belongs to.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        app_key::AppKey,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::{
        app_keys::secret::CATALOG_READ,
        product_images, settings,
        storefront::{
            dto::{LOW_STOCK_DEFAULT, ListQuery, StorefrontProduct},
            repo,
        },
    },
};

/// GET /storefront/products -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/storefront/products", tag = "storefront",
    operation_id = "storefront_products_list", summary = "List sellable products",
    params(ListQuery),
    responses(
        (status = 200, body = Paginated<StorefrontProduct>, description = "One page of sellable products"),
        (status = 401, body = MessageBody, description = "Missing, unknown or revoked app key"),
        (status = 403, body = MessageBody, description = "Key lacks `catalog:read`"),
    ),
    security(("app_key" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    key: AppKey,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<StorefrontProduct>>, AppError> {
    key.require(CATALOG_READ)?;
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let rows = repo::products(&state.db, key.workspace_id, None, page.size, page.offset()).await?;
    let total = match rows.first() {
        Some(r) => r.total,
        None if page.offset() > 0 => repo::count(&state.db, key.workspace_id).await?,
        None => 0,
    };
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let images = product_images::service::storefront_links(&state, key.workspace_id, &ids).await?;
    let low = low_stock(&state, key.workspace_id).await?;
    let data = rows
        .into_iter()
        .map(|r| StorefrontProduct::new(r, &images, low))
        .collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /storefront/products/{product}.
#[utoipa::path(
    get, path = "/storefront/products/{product}", tag = "storefront",
    operation_id = "storefront_products_get", summary = "Get a sellable product",
    params(("product" = Uuid, Path, description = "Product id")),
    responses(
        (status = 200, body = Envelope<StorefrontProduct>, description = "The product with its variants"),
        (status = 401, body = MessageBody, description = "Missing, unknown or revoked app key"),
        (status = 403, body = MessageBody, description = "Key lacks `catalog:read`"),
        (status = 404, body = MessageBody, description = "Unknown, unsellable or another workspace's product"),
    ),
    security(("app_key" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    key: AppKey,
    Path(id): Path<String>,
) -> Result<Data<StorefrontProduct>, AppError> {
    key.require(CATALOG_READ)?;
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let row = repo::products(&state.db, key.workspace_id, Some(id), 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    let images = product_images::service::storefront_links(&state, key.workspace_id, &[id]).await?;
    let low = low_stock(&state, key.workspace_id).await?;
    Ok(Data(StorefrontProduct::new(row, &images, low)))
}

/// Setting `product.low_stock_threshold`, else 5.
async fn low_stock(state: &AppState, workspace_id: Uuid) -> Result<i64, AppError> {
    Ok(settings::repo::section(&state.db, workspace_id, "product")
        .await?
        .and_then(|s| s.get("low_stock_threshold")?.as_i64())
        .unwrap_or(LOW_STOCK_DEFAULT))
}
