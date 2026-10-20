//! HTTP edge for the stock routes under /v1/workspace/{workspace}/products (Laravel StockPolicy
//! names from the permission catalog).

use axum::extract::{Path, Query, State};

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        response::Message,
        workspace::{Can, permissions},
    },
    modules::stock::{
        dto::{DeleteQuery, MoveBody, MutateBody, SetLevelsBody, SetRequirementsBody},
        service,
    },
};

permissions! {
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
