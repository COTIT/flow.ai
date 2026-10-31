//! HTTP edge for /v1/workspace/{workspace}/deal-pipelines: `dealpipeline.*` and
//! `dealpipelinestatus.*` permissions (via `Can`), call service, wrap.

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::Paginated,
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::deal_pipelines::{
        dto::{
            CreatePipelineRequest, CreateStatusRequest, ListQuery, PipelineCreated,
            PipelineResource, StatusCreated, UpdatePipelineRequest, UpdateStatusRequest,
        },
        service,
    },
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
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

/// GET /deal-pipelines -> `{data:[...], meta}`, oldest first, columns included.
#[utoipa::path(
    get, path = "/workspace/{workspace}/deal-pipelines", tag = "deal_pipelines",
    operation_id = "deal_pipelines_list", summary = "List pipelines with their columns",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<PipelineResource>, description = "One page of pipelines, oldest first"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `dealpipeline.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<PipelineList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<PipelineResource>>, AppError> {
    let page = query.page();
    let (data, total) =
        service::list(&state, access.workspace_id, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /deal-pipelines/{pipeline}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/deal-pipelines/{pipeline}", tag = "deal_pipelines",
    operation_id = "deal_pipelines_get", summary = "Get a pipeline",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pipeline" = Uuid, Path, description = "Pipeline id")),
    responses(
        (status = 200, body = Envelope<PipelineResource>, description = "The pipeline"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `dealpipeline.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pipeline, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<PipelineGet>,
    Path((_, id)): PipelinePath,
) -> Result<Data<PipelineResource>, AppError> {
    Ok(Data(service::get(&state, access.workspace_id, &id).await?))
}

/// POST /deal-pipelines -> 201 `{data:{deal_pipeline_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/deal-pipelines", tag = "deal_pipelines",
    operation_id = "deal_pipelines_create", summary = "Create a pipeline",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreatePipelineRequest,
    responses(
        (status = 201, body = Envelope<PipelineCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `dealpipeline.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    post, path = "/workspace/{workspace}/deal-pipelines/{pipeline}", tag = "deal_pipelines",
    operation_id = "deal_pipelines_update", summary = "Update a pipeline (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pipeline" = Uuid, Path, description = "Pipeline id")),
    request_body = UpdatePipelineRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `dealpipeline.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pipeline, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    post, path = "/workspace/{workspace}/deal-pipelines/{pipeline}/status", tag = "deal_pipelines",
    operation_id = "deal_pipelines_create_status", summary = "Add a column to a pipeline",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pipeline" = Uuid, Path, description = "Pipeline id")),
    request_body = CreateStatusRequest,
    responses(
        (status = 201, body = Envelope<StatusCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `dealpipelinestatus.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pipeline, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown custom status, status already on the board, or won/lost flag taken"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    post, path = "/workspace/{workspace}/deal-pipelines/{pipeline}/status/{status}", tag = "deal_pipelines",
    operation_id = "deal_pipelines_update_status", summary = "Update or move a column (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pipeline" = Uuid, Path, description = "Pipeline id"), ("status" = Uuid, Path, description = "Column id")),
    request_body = UpdateStatusRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `dealpipelinestatus.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, pipeline or column, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, or won/lost flag taken"),
    ),
    security(("bearer" = []))
)]
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
#[utoipa::path(
    delete, path = "/workspace/{workspace}/deal-pipelines/{pipeline}/status/{status}", tag = "deal_pipelines",
    operation_id = "deal_pipelines_delete_status", summary = "Delete a column; its deals move to another column",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("pipeline" = Uuid, Path, description = "Pipeline id"), ("status" = Uuid, Path, description = "Column id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `dealpipelinestatus.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, pipeline or column, or caller not a member"),
        (status = 409, body = MessageBody, description = "The column holds deals and the board has no other column"),
    ),
    security(("bearer" = []))
)]
pub async fn delete_status(
    State(state): State<AppState>,
    access: Can<StatusDelete>,
    Path((_, pipeline, id)): StatusPath,
) -> Result<Message, AppError> {
    service::delete_status(&state, access.workspace_id, &pipeline, &id).await?;
    Ok(Message("ok"))
}
