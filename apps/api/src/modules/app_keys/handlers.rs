//! HTTP edge for /v1/workspace/{workspace}/app-keys: workspace admins issue, list and revoke
//! the keys apps (shop, till, field, partner) call the API with.

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
    modules::app_keys::{
        dto::{AppKeyCreated, AppKeyResource, CreateAppKeyRequest, ListQuery},
        service,
    },
};

permissions! {
    WorkspaceAdmin => "workspace_admin",
}

/// GET /app-keys -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/app-keys", tag = "app_keys",
    operation_id = "app_keys_list", summary = "List app keys",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<AppKeyResource>, description = "One page of keys, newest first"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<AppKeyResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) =
        service::list(&state, access.workspace_id, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /app-keys -> 201 `{data:{..., key}}`; the only reply that carries the secret.
#[utoipa::path(
    post, path = "/workspace/{workspace}/app-keys", tag = "app_keys",
    operation_id = "app_keys_create", summary = "Issue an app key",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateAppKeyRequest,
    responses(
        (status = 201, body = Envelope<AppKeyCreated>, description = "Issued; `key` is shown only here"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    ValidatedJson(input): ValidatedJson<CreateAppKeyRequest>,
) -> Result<(StatusCode, Data<AppKeyCreated>), AppError> {
    let (row, plain) = service::create(&state, &access, input).await?;
    let body = AppKeyCreated::new(row, plain.expose().to_owned());
    Ok((StatusCode::CREATED, Data(body)))
}

/// DELETE /app-keys/{key} -> 204. Revoked keys stay listed with `revoked_at`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/app-keys/{key}", tag = "app_keys",
    operation_id = "app_keys_revoke", summary = "Revoke an app key",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("key" = Uuid, Path, description = "App key id"),
    ),
    responses(
        (status = 204, description = "Revoked (also when it already was)"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or key, or caller not a member"),
        (status = 409, body = MessageBody, description = "Managed Shop actor: disconnect its Shop connection instead"),
    ),
    security(("bearer" = []))
)]
pub async fn revoke(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Path((_, id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    service::revoke(&state, access.workspace_id, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
