//! HTTP edge for /v1/workspace/{workspace}/deals and the board column read: `deal.*`
//! permissions (via `Can`), call service, wrap.

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
        list::{Paginated, search},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::deals::{
        dto::{
            AddRelationRequest, ColumnQuery, CreateDealRequest, DealCreated, DealResource,
            ListQuery, MoveDealRequest, UpdateDealRequest,
        },
        repo::ListFilter,
        service,
    },
};

// Laravel checked none of these (DealPolicy was never registered); user decision 2026-10-23.
permissions! {
    DealList => "deal.list",
    DealGet => "deal.get",
    DealCreate => "deal.create",
    DealUpdate => "deal.update",
    DealDelete => "deal.delete",
}

/// `{workspace}` + `{deal}`; the id stays raw so a malformed one is a 404.
type DealPath = Path<(String, String)>;

/// GET /deals -> `{data:[...], meta}`, newest first.
#[utoipa::path(
    get, path = "/workspace/{workspace}/deals", tag = "deals",
    operation_id = "deals_list", summary = "List deals, newest first",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<DealResource>, description = "One page of deals"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `deal.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Malformed filter id or search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<DealList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<DealResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let page = query.page();
    let [relation, pipeline, status] = query.filters()?;
    let filter = ListFilter {
        pattern: pattern.as_deref(),
        relation,
        pipeline,
        status,
        limit: page.size,
        offset: page.offset(),
    };
    let (data, total) = service::list(&state, access.workspace_id, &filter).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /deals/{deal}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/deals/{deal}", tag = "deals",
    operation_id = "deals_get", summary = "Get a deal with its pipeline and column",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("deal" = Uuid, Path, description = "Deal id")),
    responses(
        (status = 200, body = Envelope<DealResource>, description = "The deal"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `deal.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or deal, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<DealGet>,
    Path((_, id)): DealPath,
) -> Result<Data<DealResource>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /deals -> 201 `{data:{deal_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/deals", tag = "deals",
    operation_id = "deals_create", summary = "Create a deal at the end of a column",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateDealRequest,
    responses(
        (status = 201, body = Envelope<DealCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `deal.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown pipeline, column, relation, currency or sales agent, or no default currency"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<DealCreate>,
    ValidatedJson(input): ValidatedJson<CreateDealRequest>,
) -> Result<(StatusCode, Data<DealCreated>), AppError> {
    let deal_id = service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((StatusCode::CREATED, Data(DealCreated { deal_id })))
}

/// POST /deals/{deal} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/deals/{deal}", tag = "deals",
    operation_id = "deals_update", summary = "Update a deal (send only changed fields); a new column appends it there",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("deal" = Uuid, Path, description = "Deal id")),
    request_body = UpdateDealRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `deal.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or deal, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or unknown pipeline, column or sales agent"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<DealUpdate>,
    Path((_, id)): DealPath,
    ValidatedJson(patch): ValidatedJson<UpdateDealRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// POST /deals/{deal}/update-status (kanban drag) -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/deals/{deal}/update-status", tag = "deals",
    operation_id = "deals_move", summary = "Move a deal on the board (kanban drag)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("deal" = Uuid, Path, description = "Deal id")),
    request_body = MoveDealRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `deal.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or deal, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or unknown pipeline or column"),
    ),
    security(("bearer" = []))
)]
pub async fn move_to(
    State(state): State<AppState>,
    access: Can<DealUpdate>,
    Path((_, id)): DealPath,
    ValidatedJson(m): ValidatedJson<MoveDealRequest>,
) -> Result<Message, AppError> {
    service::move_to(&state, access.workspace_id, &id, m).await?;
    Ok(Message("ok"))
}

/// POST /deals/{deal}/add-relation -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/deals/{deal}/add-relation", tag = "deals",
    operation_id = "deals_add_relation", summary = "Attach a relation to a deal that has none",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("deal" = Uuid, Path, description = "Deal id")),
    request_body = AddRelationRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 400, body = MessageBody, description = "The deal already has a relation"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `deal.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or deal, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid or unknown relation id"),
    ),
    security(("bearer" = []))
)]
pub async fn add_relation(
    State(state): State<AppState>,
    access: Can<DealUpdate>,
    Path((_, id)): DealPath,
    ValidatedJson(relation): ValidatedJson<AddRelationRequest>,
) -> Result<Message, AppError> {
    service::add_relation(&state, access.workspace_id, &id, relation).await?;
    Ok(Message("ok"))
}

/// DELETE /deals/{deal} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/deals/{deal}", tag = "deals",
    operation_id = "deals_delete", summary = "Delete a deal",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("deal" = Uuid, Path, description = "Deal id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `deal.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or deal, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<DealDelete>,
    Path((_, id)): DealPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

/// GET /deal-pipelines/{pipeline}/status/{status}/deals -> one board column, pages of 999.
#[utoipa::path(
    get, path = "/workspace/{workspace}/deal-pipelines/{pipeline}/status/{status}/deals", tag = "deals",
    operation_id = "deals_column", summary = "One board column of deals (pages of 999)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pipeline" = Uuid, Path, description = "Pipeline id"), ("status" = Uuid, Path, description = "Column id"), ColumnQuery),
    responses(
        (status = 200, body = Paginated<DealResource>, description = "One page of the column, in board order"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `deal.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, pipeline or column, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn column(
    State(state): State<AppState>,
    access: Can<DealList>,
    Path((_, pipeline, status)): Path<(String, String, String)>,
    Query(query): Query<ColumnQuery>,
) -> Result<Json<Paginated<DealResource>>, AppError> {
    let page = query.page();
    let (data, total) = service::column(
        &state,
        access.workspace_id,
        &pipeline,
        &status,
        page.size,
        page.offset(),
    )
    .await?;
    Ok(Json(Paginated::new(data, total, page)))
}
