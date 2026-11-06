//! HTTP edge for the stock routes under /v1/workspace/{workspace}/products (`stock.*`).
// Laravel StockPolicy names from the permission catalog.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::{Value, json};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, search},
        response::{Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Keyed},
    modules::stock::{
        dto::{
            DeleteQuery, ListQuery, MoveBody, MoveDoc, MovementQuery, MutateBody, MutateDoc,
            MutatedDoc, SetLevelsBody, SetLevelsDoc, SetRequirementsBody, SetRequirementsDoc,
        },
        repo::ListFilter,
        resource::{ConfiguredDoc, MovementDoc, StockDoc, WithBinsDoc},
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

/// POST /products/{product}/stocks -> `{"message":"ok", before, after, warehouse}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/{product}/stocks", tag = "stock",
    operation_id = "stock_mutate", summary = "Add to or take from stock",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer and moves nothing again; another request under the key is a 422. Kept for a day."),),
    request_body = MutateDoc,
    responses(
        (status = 200, body = MutatedDoc, description = "Done: `ok`, plus `before`/`after`/`warehouse` when stock moved"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.mutate`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or Idempotency-Key reused for another request"),
    ),
    security(("bearer" = []))
)]
pub async fn mutate(
    State(state): State<AppState>,
    access: Can<StockMutate>,
    Path((_, p)): ProductPath,
    Keyed { input: m, key }: Keyed<MutateBody>,
) -> Result<Answer, AppError> {
    service::mutate(&state, access.workspace_id, &p, m, access.user_id, key).await
}

/// POST /products/{product}/set-stock-levels -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/{product}/set-stock-levels", tag = "stock",
    operation_id = "stock_set_levels", summary = "Set stock levels per warehouse",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id")),
    request_body = SetLevelsDoc,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.set_stock_levels`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/{product}/set-stock-requirements", tag = "stock",
    operation_id = "stock_set_requirements", summary = "Set minimum and maximum stock per warehouse",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id")),
    request_body = SetRequirementsDoc,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.set_stock_requirements`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    delete, path = "/workspace/{workspace}/products/{product}/stocks", tag = "stock",
    operation_id = "stock_delete", summary = "Delete an empty stock row",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), DeleteQuery),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 400, body = MessageBody, description = "No such stock row, or its level is not 0"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.mutate`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Missing or malformed ids"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/{product}/move-stock-between-bins", tag = "stock",
    operation_id = "stock_move_between_bins", summary = "Move stock between bins",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id")),
    request_body = MoveDoc,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.move`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/{product}/stock-with-bins", tag = "stock",
    operation_id = "stock_with_bins", summary = "List a product's stock with bin paths",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id")),
    responses(
        (status = 200, body = Envelope<Vec<WithBinsDoc>>, description = "Every stock row; not paged"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn with_bins(
    State(state): State<AppState>,
    access: Can<StockList>,
    Path((_, p)): ProductPath,
) -> Result<Json<Value>, AppError> {
    let data = service::with_bins(&state, access.workspace_id, &p).await?;
    Ok(Json(json!({ "data": data })))
}

/// GET /products/{product}/configured-bins -> `{data:[...]}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/{product}/configured-bins", tag = "stock",
    operation_id = "stock_configured_bins", summary = "List bins holding the product, by warehouse",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id")),
    responses(
        (status = 200, body = Envelope<Vec<ConfiguredDoc>>, description = "Bins grouped by warehouse; not paged"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn configured_bins(
    State(state): State<AppState>,
    access: Can<StockList>,
    Path((_, p)): ProductPath,
) -> Result<Json<Value>, AppError> {
    let data = service::configured_bins(&state, access.workspace_id, &p).await?;
    Ok(Json(json!({ "data": data })))
}

/// GET /products/{product}/stock-movements -> `{data, meta}`, newest first.
// Laravel dropped `meta`; the web client reads it.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/{product}/stock-movements", tag = "stock",
    operation_id = "stock_movements", summary = "List a product's stock movements",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), MovementQuery),
    responses(
        (status = 200, body = Paginated<MovementDoc>, description = "One page of movements"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown movement `type`"),
    ),
    security(("bearer" = []))
)]
pub async fn movements(
    State(state): State<AppState>,
    access: Can<StockList>,
    Path((_, p)): ProductPath,
    Query(q): Query<MovementQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let (f, page) = q.filter()?;
    let (data, total) = service::movements(&state, access.workspace_id, &p, &f).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /products/stocks -> `{data, meta}`, oldest first.
// Laravel ListStock.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/stocks", tag = "stock",
    operation_id = "stock_list", summary = "List stock rows",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<StockDoc>, description = "One page of stock rows"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<StockList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    page_of(&state, access.workspace_id, &q, false).await
}

/// GET /products/stocks/negative -> store rows below 0, lowest first, `q` on product codes
/// and warehouse name.
// Laravel ListNegativeStock; `meta` gains `total`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/stocks/negative", tag = "stock",
    operation_id = "stock_negative", summary = "List store stock below zero",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<StockDoc>, description = "One page of negative rows, lowest first"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `stock.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn negative(
    State(state): State<AppState>,
    access: Can<StockList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    page_of(&state, access.workspace_id, &q, true).await
}

async fn page_of(
    state: &AppState,
    workspace_id: uuid::Uuid,
    q: &ListQuery,
    negative: bool,
) -> Result<Json<Paginated<Value>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let pattern = if negative {
        search(q.q.as_deref())?
    } else {
        None
    };
    let f = ListFilter {
        negative,
        pattern: pattern.as_deref(),
        limit: page.size,
        offset: page.offset(),
    };
    let (data, total) = service::list(state, workspace_id, &f).await?;
    Ok(Json(Paginated::new(data, total, page)))
}
