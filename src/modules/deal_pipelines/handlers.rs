//! HTTP edge for /v1/workspace/{workspace}/deal-pipelines: `dealpipeline.*` and
//! `dealpipelinestatus.*` permissions (via `Can`), call service, wrap.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::deal_pipelines::{
        dto::{
            CreatePipelineRequest, CreateStatusRequest, PipelineCreated, PipelineResource,
            StatusCreated, UpdatePipelineRequest, UpdateStatusRequest,
        },
        service,
    },
};

// Laravel checked only `dealpipeline.create/list` and `dealpipelinestatus.create`; the other
// routes were open to every member.
permissions! {
    PipelineList => "dealpipeline.list",
    PipelineGet => "dealpipeline.get",
    PipelineCreate => "dealpipeline.create",
    PipelineUpdate => "dealpipeline.update",
    StatusCreate => "dealpipelinestatus.create",
    StatusUpdate => "dealpipelinestatus.update",
    StatusDelete => "dealpipelinestatus.delete",
}

/// `{workspace}` + `{pipeline}`; ids stay raw so a malformed one is a 404.
type PipelinePath = Path<(String, String)>;
/// `{workspace}` + `{pipeline}` + `{status}`.
type StatusPath = Path<(String, String, String)>;

/// Paging params; unknown ones ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    page: Option<String>,
    page_size: Option<String>,
}

/// GET /deal-pipelines -> `{data:[...], meta}`, oldest first, columns included.
pub async fn list(
    State(state): State<AppState>,
    access: Can<PipelineList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<PipelineResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (data, total) =
        service::list(&state, access.workspace_id, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /deal-pipelines/{pipeline}.
pub async fn get(
    State(state): State<AppState>,
    access: Can<PipelineGet>,
    Path((_, id)): PipelinePath,
) -> Result<Data<PipelineResource>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /deal-pipelines -> 201 `{data:{deal_pipeline_id}}`.
pub async fn create(
    State(state): State<AppState>,
    access: Can<PipelineCreate>,
    ValidatedJson(input): ValidatedJson<CreatePipelineRequest>,
) -> Result<(StatusCode, Data<PipelineCreated>), AppError> {
    let deal_pipeline_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(PipelineCreated { deal_pipeline_id }),
    ))
}

/// POST /deal-pipelines/{pipeline} -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<PipelineUpdate>,
    Path((_, id)): PipelinePath,
    ValidatedJson(patch): ValidatedJson<UpdatePipelineRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// POST /deal-pipelines/{pipeline}/status -> 201 `{data:{deal_pipeline_status_id}}`.
pub async fn create_status(
    State(state): State<AppState>,
    access: Can<StatusCreate>,
    Path((_, pipeline)): PipelinePath,
    ValidatedJson(input): ValidatedJson<CreateStatusRequest>,
) -> Result<(StatusCode, Data<StatusCreated>), AppError> {
    let deal_pipeline_status_id =
        service::create_status(&state, access.workspace_id, &pipeline, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(StatusCreated {
            deal_pipeline_status_id,
        }),
    ))
}

/// POST /deal-pipelines/{pipeline}/status/{status} -> `{"message":"ok"}`.
pub async fn update_status(
    State(state): State<AppState>,
    access: Can<StatusUpdate>,
    Path((_, pipeline, id)): StatusPath,
    ValidatedJson(patch): ValidatedJson<UpdateStatusRequest>,
) -> Result<Message, AppError> {
    service::update_status(&state, access.workspace_id, &pipeline, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /deal-pipelines/{pipeline}/status/{status} -> `{"message":"ok"}`.
pub async fn delete_status(
    State(state): State<AppState>,
    access: Can<StatusDelete>,
    Path((_, pipeline, id)): StatusPath,
) -> Result<Message, AppError> {
    service::delete_status(&state, access.workspace_id, &pipeline, &id).await?;
    Ok(Message("ok"))
}
