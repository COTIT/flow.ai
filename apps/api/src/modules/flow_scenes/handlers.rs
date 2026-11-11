//! HTTP edge for /v1/workspace/{workspace}/flow/scenes: any member, own rows only.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
    modules::flow_scenes::{
        dto::{FlowScene, UpdateFlowSceneRequest},
        service,
    },
};

/// GET /flow/scenes -> `{data:[...]}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/flow/scenes", tag = "flow_scenes",
    operation_id = "flow_scenes_list",
    summary = "My Flow scenes (Your Flow layout: which cards and charts, order, hidden)",
    description = "The caller's own Flow scenes here: built-in defaults merged with what the caller saved, sorted by position then key. Display settings only, no business values. `restricted` lists component ids the caller's role can't read.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<Vec<FlowScene>>, description = "The caller's scenes"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
) -> Result<Data<Vec<FlowScene>>, AppError> {
    Ok(Data(service::list(&state, &access).await?))
}

/// PUT /flow/scenes/{scene} -> `{data}`.
#[utoipa::path(
    put, path = "/workspace/{workspace}/flow/scenes/{scene}", tag = "flow_scenes",
    operation_id = "flow_scenes_update",
    summary = "Change one of my Flow scenes (reorder, hide, pick cards and charts)",
    description = "Saves the caller's settings for one scene. Omitted fields keep the saved value, else the default. `config` replaces the component list. A new custom scene (`sales_*`, `stock_*`) needs `config`. At most 20 saved scenes per member per workspace. Sending the same body again changes nothing.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("scene" = String, Path, description = "Built-in scene (pulse, stock, noticed, orders, sales, routines, studio) or custom `sales_*` / `stock_*`"),
    ),
    request_body = UpdateFlowSceneRequest,
    responses(
        (status = 200, body = Envelope<FlowScene>, description = "Saved scene"),
        (status = 403, body = MessageBody, description = "Inactive membership, or config adds a component the caller's role can't read"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown scene, type, metric or period; bad or repeated component id; more than 8 components; config over 8 KiB; position outside 0..19; empty body; unknown field; new custom scene without config; 20 scenes already"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, scene)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<UpdateFlowSceneRequest>,
) -> Result<Data<FlowScene>, AppError> {
    Ok(Data(service::save(&state, &access, &scene, input).await?))
}

/// DELETE /flow/scenes/{scene} -> 204.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/flow/scenes/{scene}", tag = "flow_scenes",
    operation_id = "flow_scenes_delete",
    summary = "Restore one of my Flow scenes to its default (or remove a custom scene)",
    description = "Drops the caller's saved settings for one scene: a built-in goes back to its default, a custom scene disappears. Nothing saved is fine (204).",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("scene" = String, Path, description = "Scene key"),
    ),
    responses(
        (status = 204, description = "Default restored"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown scene key"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, scene)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    service::delete(&state, &access, &scene).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /flow/scenes/reset -> 204.
#[utoipa::path(
    post, path = "/workspace/{workspace}/flow/scenes/reset", tag = "flow_scenes",
    operation_id = "flow_scenes_reset",
    summary = "Reset my Flow to the defaults",
    description = "Drops every scene setting the caller saved in this workspace. Other members' settings stay.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 204, description = "Defaults restored"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn reset(
    State(state): State<AppState>,
    access: WorkspaceAccess,
) -> Result<StatusCode, AppError> {
    service::reset(&state, &access).await?;
    Ok(StatusCode::NO_CONTENT)
}
