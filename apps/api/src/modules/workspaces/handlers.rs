//! HTTP edge for the workspace record: GET /v1/workspace/{workspace}, POST .../update.

use axum::extract::State;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::workspaces::{
        dto::{UpdateWorkspaceRequest, WorkspaceProfilePatch, WorkspaceRecordResource},
        service,
    },
};

permissions! {
    // Laravel checked `workspace.view`, which no catalog entry grants.
    WorkspaceGet => "workspace.get",
    WorkspaceUpdate => "workspace.update",
}

/// GET /v1/workspace/{workspace} -> `{data:{...}}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}", tag = "workspaces",
    operation_id = "workspaces_get", summary = "Get the workspace record",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<WorkspaceRecordResource>, description = "The workspace, with its company profile"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `workspace.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<WorkspaceGet>,
) -> Result<Data<WorkspaceRecordResource>, AppError> {
    Ok(Data(
        service::record(&state, access.workspace_id).await?.into(),
    ))
}

/// POST /v1/workspace/{workspace}/update -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/update", tag = "workspaces",
    operation_id = "workspaces_update", summary = "Update the company profile (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = WorkspaceProfilePatch,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `workspace.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<WorkspaceUpdate>,
    ValidatedJson(patch): ValidatedJson<UpdateWorkspaceRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, patch).await?;
    Ok(Message("ok"))
}
