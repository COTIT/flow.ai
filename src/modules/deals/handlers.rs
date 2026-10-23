//! HTTP edge for /v1/workspace/{workspace}/deals and the board column read: `deal.*`
//! permissions (via `Can`), call service, wrap.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Paginated, search},
        response::{Data, Message},
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
pub async fn get(
    State(state): State<AppState>,
    access: Can<DealGet>,
    Path((_, id)): DealPath,
) -> Result<Data<DealResource>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /deals -> 201 `{data:{deal_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<DealCreate>,
    ValidatedJson(input): ValidatedJson<CreateDealRequest>,
) -> Result<(StatusCode, Data<DealCreated>), AppError> {
    let deal_id = service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((StatusCode::CREATED, Data(DealCreated { deal_id })))
}

/// POST /deals/{deal} -> `{"message":"ok"}`.
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
pub async fn delete(
    State(state): State<AppState>,
    access: Can<DealDelete>,
    Path((_, id)): DealPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}

/// GET /deal-pipelines/{pipeline}/status/{status}/deals -> one board column, pages of 999.
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
