//! HTTP edge for /v1/workspace/{workspace}/till/sales: the till sells and looks its sales up by
//! `client_ref`; office lists them (`cashregister.list`).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    modules::{
        till::session::TillSession,
        till_sales::{
            dto::{QuoteRequest, QuoteResource, SaleQuery, SaleRequest, SaleResource},
            service,
        },
    },
};

permissions! {
    SalesList => "cashregister.list",
}

/// POST /till/sales -> 201 `{data: sale}`, or 200 with the same sale on a replay.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/sales", tag = "till",
    operation_id = "till_sales_create", summary = "Book a till sale (or replay it)",
    description = "Lines carry the unit price the till charged; the order holds those amounts. Stock is taken at once from the register's store warehouse and never refused. Cash rounds to the currency's step (`cash_register.cash_rounding`) and needs an open shift. The same `client_ref` and body again answer the stored sale with 200.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = SaleRequest,
    responses(
        (status = 201, body = Envelope<SaleResource>, description = "Booked"),
        (status = 200, body = Envelope<SaleResource>, description = "Already booked under this client_ref: the stored sale"),
        (status = 401, body = MessageBody, description = "No live till session"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
        (status = 409, body = MessageBody, description = "client_ref used for another sale, or a cash sale without an open shift"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown product, or too little cash tendered"),
    ),
    security(("till_session" = []))
)]
pub async fn sell(
    State(state): State<AppState>,
    session: TillSession,
    ValidatedJson(sale): ValidatedJson<SaleRequest>,
) -> Result<(StatusCode, Data<SaleResource>), AppError> {
    let (created, row) = service::sell(&state, &session.row, sale).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let ws = session.row.workspace_id;
    let mut one = service::resources(&state, ws, vec![row]).await?;
    let sale = one
        .pop()
        .ok_or_else(|| AppError::Internal("till sale vanished".into()))?;
    Ok((status, Data(sale)))
}

/// POST /till/sales/quote -> `{data: quote}`: what the sale would book, booking nothing.
#[utoipa::path(
    post, path = "/workspace/{workspace}/till/sales/quote", tag = "till",
    operation_id = "till_sales_quote", summary = "Price a till basket as a sale would (writes nothing)",
    description = "Same lines and payment as `POST /till/sales`, priced and rounded by the same code. Writes nothing: no order, stock or drawer change, and no open shift needed.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = QuoteRequest,
    responses(
        (status = 200, body = Envelope<QuoteResource>, description = "What the sale would book now"),
        (status = 401, body = MessageBody, description = "No live till session"),
        (status = 404, body = MessageBody, description = "Workspace is not the session's"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown product, or too little cash tendered"),
    ),
    security(("till_session" = []))
)]
pub async fn quote(
    State(state): State<AppState>,
    session: TillSession,
    ValidatedJson(q): ValidatedJson<QuoteRequest>,
) -> Result<Data<QuoteResource>, AppError> {
    Ok(Data(service::quote(&state, &session.row, q).await?))
}

/// GET /till/sales/{client_ref} -> `{data: sale}` of this register.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/sales/{client_ref}", tag = "till",
    operation_id = "till_sales_get", summary = "A sale of this register by client_ref",
    description = "For the offline queue: check whether a queued sale already landed before replaying it.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("client_ref" = String, Path, description = "The till's id of the sale"),
    ),
    responses(
        (status = 200, body = Envelope<SaleResource>, description = "Booked"),
        (status = 401, body = MessageBody, description = "No live till session"),
        (status = 404, body = MessageBody, description = "Not booked on this register"),
    ),
    security(("till_session" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    session: TillSession,
    Path((_, client_ref)): Path<(String, String)>,
) -> Result<Data<SaleResource>, AppError> {
    let row = service::get(&state, &session.row, &client_ref).await?;
    let ws = session.row.workspace_id;
    let mut one = service::resources(&state, ws, vec![row]).await?;
    let sale = one
        .pop()
        .ok_or_else(|| AppError::Internal("till sale vanished".into()))?;
    Ok(Data(sale))
}

/// GET /till/sales -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/till/sales", tag = "till",
    operation_id = "till_sales_list", summary = "List till sales",
    params(("workspace" = Uuid, Path, description = "Workspace id"), SaleQuery),
    responses(
        (status = 200, body = Paginated<SaleResource>, description = "One page, newest first"),
        (status = 403, body = MessageBody, description = "Inactive membership, or no `cashregister.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Malformed `register_id`, `from` or `to`"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<SalesList>,
    Query(query): Query<SaleQuery>,
) -> Result<Json<Paginated<SaleResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let f = query.filter()?;
    let (rows, total) =
        service::list(&state, access.workspace_id, &f, page.size, page.offset()).await?;
    let data = service::resources(&state, access.workspace_id, rows).await?;
    Ok(Json(Paginated::new(data, total, page)))
}
