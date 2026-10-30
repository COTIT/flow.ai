//! HTTP edge for /v1/workspace/{workspace}/currencies. Reads are open to every active member
//! (vimdesk-web and the POS load the list at boot to format money); writes need `currency.*`.

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
        list::{Page, Paginated, search},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    modules::currencies::{
        dto::{
            CreateCurrencyRequest, CurrencyCreated, CurrencyResource, ListQuery,
            UpdateCurrencyRequest,
        },
        service,
    },
};

permissions! {
    CurrencyCreate => "currency.create",
    CurrencyUpdate => "currency.update",
    CurrencyDelete => "currency.delete",
}

/// `{workspace}` + `{currency}`; the id stays raw so a malformed one is a 404.
type CurrencyPath = Path<(String, String)>;

/// GET /currencies -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/currencies", tag = "currencies",
    operation_id = "currencies_list", summary = "List currencies",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<CurrencyResource>, description = "One page of currencies"),
        (status = 403, body = MessageBody, description = "Membership is inactive"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<CurrencyResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        pattern.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /currencies/{currency}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/currencies/{currency}", tag = "currencies",
    operation_id = "currencies_get", summary = "Get a currency",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("currency" = Uuid, Path, description = "Currency id"),
    ),
    responses(
        (status = 200, body = Envelope<CurrencyResource>, description = "The currency"),
        (status = 403, body = MessageBody, description = "Membership is inactive"),
        (status = 404, body = MessageBody, description = "Unknown workspace or currency, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): CurrencyPath,
) -> Result<Data<CurrencyResource>, AppError> {
    let row = service::get(&state, access.workspace_id, &id).await?;
    Ok(Data(row.into()))
}

/// POST /currencies -> 201 `{data:{currency_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/currencies", tag = "currencies",
    operation_id = "currencies_create", summary = "Create a currency",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateCurrencyRequest,
    responses(
        (status = 201, body = Envelope<CurrencyCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `currency.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<CurrencyCreate>,
    ValidatedJson(input): ValidatedJson<CreateCurrencyRequest>,
) -> Result<(StatusCode, Data<CurrencyCreated>), AppError> {
    let currency_id = service::create(&state, access.workspace_id, input).await?;
    Ok((StatusCode::CREATED, Data(CurrencyCreated { currency_id })))
}

/// POST /currencies/{currency} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/currencies/{currency}", tag = "currencies",
    operation_id = "currencies_update", summary = "Update a currency (send only changed fields)",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("currency" = Uuid, Path, description = "Currency id"),
    ),
    request_body = UpdateCurrencyRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `currency.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or currency, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<CurrencyUpdate>,
    Path((_, id)): CurrencyPath,
    ValidatedJson(patch): ValidatedJson<UpdateCurrencyRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /currencies/{currency} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/currencies/{currency}", tag = "currencies",
    operation_id = "currencies_delete", summary = "Delete a currency",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("currency" = Uuid, Path, description = "Currency id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `currency.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or currency, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<CurrencyDelete>,
    Path((_, id)): CurrencyPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
