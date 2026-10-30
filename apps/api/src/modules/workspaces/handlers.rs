//! HTTP edge for the workspace record: GET /v1/workspace/{workspace}, POST .../update.

use axum::extract::State;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::workspaces::{
        dto::{UpdateWorkspaceRequest, WorkspaceRecordResource},
        service,
    },
};

permissions! {
    // Laravel checked `workspace.view`, which no catalog entry grants.
    WorkspaceGet => "workspace.get",
    WorkspaceUpdate => "workspace.update",
}

/// GET /v1/workspace/{workspace} -> `{data:{...}}`.
pub async fn get(
    State(state): State<AppState>,
    access: Can<WorkspaceGet>,
) -> Result<Data<WorkspaceRecordResource>, AppError> {
    Ok(Data(
        service::record(&state, access.workspace_id).await?.into(),
    ))
}

/// POST /v1/workspace/{workspace}/update -> `{"message":"ok"}`.
pub async fn update(
    State(state): State<AppState>,
    access: Can<WorkspaceUpdate>,
    ValidatedJson(patch): ValidatedJson<UpdateWorkspaceRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, patch).await?;
    Ok(Message("ok"))
}
