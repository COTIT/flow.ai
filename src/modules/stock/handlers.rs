//! HTTP edge for the stock routes under /v1/workspace/{workspace}/products (Laravel StockPolicy
//! names from the permission catalog).

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::{Value, json};

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::Paginated,
        response::Message,
        workspace::{Can, permissions},
    },
    modules::stock::{
        dto::{
            DeleteQuery, MoveBody, MovementQuery, MutateBody, SetLevelsBody, SetRequirementsBody,
        },
        service,
    },
};

permissions! {
    StockList => "stock.list",
    StockMutate => "stock.mutate",
    StockMove => "stock.move",
    StockSetLevels => "stock.set_stock_levels",
    StockSetRequirements => "stock.set_stock_requirements",
}

type ProductPath = Path<(String, String)>;

/// POST /products/{product}/stocks -> `{"message":"ok"}`.
pub async fn mutate(
    State(state): State<AppState>,
    access: Can<StockMutate>,
    Path((_, p)): ProductPath,
    ValidatedJson(m): ValidatedJson<MutateBody>,
) -> Result<Message, AppError> {
    service::mutate(&state, access.workspace_id, &p, m, access.user_id).await?;
    Ok(Message("ok"))
}

/// POST /products/{product}/set-stock-levels -> `{"message":"ok"}`.
pub async fn set_levels(
    State(state): State<AppState>,
    access: Can<StockSetLevels>,
    Path((_, p)): ProductPath,
    ValidatedJson(entries): ValidatedJson<SetLevelsBody>,
) -> Result<Message, AppError> {
    service::set_levels(&state, access.workspace_id, &p, entries, access.user_id).await?;
    Ok(Message("ok"))
}

/// POST /products/{product}/set-stock-requirements -> `{"message":"ok"}`.
pub async fn set_requirements(
    State(state): State<AppState>,
    access: Can<StockSetRequirements>,
    Path((_, p)): ProductPath,
    ValidatedJson(req): ValidatedJson<SetRequirementsBody>,
) -> Result<Message, AppError> {
    service::set_requirements(&state, access.workspace_id, &p, req, access.user_id).await?;
    Ok(Message("ok"))
}

/// DELETE /products/{product}/stocks?warehouse_id=&bin_id= -> `{"message":"ok"}` (Laravel
/// checks `setStockLevels` here).
pub async fn delete(
    State(state): State<AppState>,
    access: Can<StockSetLevels>,
    Path((_, p)): ProductPath,
    Query(q): Query<DeleteQuery>,
) -> Result<Message, AppError> {
    let (warehouse, bin) = q.parse()?;
    service::delete(&state, access.workspace_id, &p, warehouse, bin).await?;
    Ok(Message("ok"))
}

/// POST /products/{product}/move-stock-between-bins -> `{"message":"ok"}`.
pub async fn move_between_bins(
    State(state): State<AppState>,
    access: Can<StockMove>,
    Path((_, p)): ProductPath,
    ValidatedJson(m): ValidatedJson<MoveBody>,
) -> Result<Message, AppError> {
    service::move_between_bins(&state, access.workspace_id, &p, m, access.user_id).await?;
    Ok(Message("ok"))
}

/// GET /products/{product}/stock-with-bins -> `{data:[...]}`.
pub async fn with_bins(
    State(state): State<AppState>,
    access: Can<StockList>,
    Path((_, p)): ProductPath,
) -> Result<Json<Value>, AppError> {
    let data = service::with_bins(&state, access.workspace_id, &p).await?;
    Ok(Json(json!({ "data": data })))
}

/// GET /products/{product}/configured-bins -> `{data:[...]}`.
pub async fn configured_bins(
    State(state): State<AppState>,
    access: Can<StockList>,
    Path((_, p)): ProductPath,
) -> Result<Json<Value>, AppError> {
    let data = service::configured_bins(&state, access.workspace_id, &p).await?;
    Ok(Json(json!({ "data": data })))
}

/// GET /products/{product}/stock-movements -> `{data, meta}` (Laravel dropped `meta`; the
/// web client reads it).
pub async fn movements(
    State(state): State<AppState>,
    access: Can<StockList>,
    Path((_, p)): ProductPath,
    Query(q): Query<MovementQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let (f, page) = q.filter();
    let (data, total) = service::movements(&state, access.workspace_id, &p, &f).await?;
    Ok(Json(Paginated::new(data, total, page)))
}
