//! HTTP edge for /v1/workspace/{workspace}/webhook-endpoints: workspace admins register the
//! URLs this workspace's events are sent to.

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
        workspace::WorkspaceAccess,
    },
    modules::{
        app_keys::service::admin,
        webhooks::{
            dto::{
                CreateEndpointRequest, ListQuery, UpdateEndpointRequest, WebhookEndpointCreated,
                WebhookEndpointResource,
            },
            service,
        },
    },
};

/// `{workspace}` + `{endpoint}`; the id stays raw so a malformed one is a 404.
type EndpointPath = Path<(String, String)>;

/// GET /webhook-endpoints -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/webhook-endpoints", tag = "webhooks",
    operation_id = "webhooks_list", summary = "List webhook endpoints",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<WebhookEndpointResource>, description = "One page of endpoints, newest first"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<WebhookEndpointResource>>, AppError> {
    admin(&access)?;
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) =
        service::list(&state, access.workspace_id, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /webhook-endpoints -> 201 `{data:{..., secret}}`; the only reply with the secret.
#[utoipa::path(
    post, path = "/workspace/{workspace}/webhook-endpoints", tag = "webhooks",
    operation_id = "webhooks_create", summary = "Register a webhook endpoint",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateEndpointRequest,
    responses(
        (status = 201, body = Envelope<WebhookEndpointCreated>, description = "Registered; `secret` is shown only here"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    ValidatedJson(input): ValidatedJson<CreateEndpointRequest>,
) -> Result<(StatusCode, Data<WebhookEndpointCreated>), AppError> {
    admin(&access)?;
    let (row, secret) = service::create(&state, &access, input).await?;
    let body = WebhookEndpointCreated::new(row, secret.expose().to_owned());
    Ok((StatusCode::CREATED, Data(body)))
}

/// GET /webhook-endpoints/{endpoint}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/webhook-endpoints/{endpoint}", tag = "webhooks",
    operation_id = "webhooks_get", summary = "Get a webhook endpoint",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("endpoint" = Uuid, Path, description = "Endpoint id"),
    ),
    responses(
        (status = 200, body = Envelope<WebhookEndpointResource>, description = "The endpoint"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or endpoint, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): EndpointPath,
) -> Result<Data<WebhookEndpointResource>, AppError> {
    admin(&access)?;
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// PATCH /webhook-endpoints/{endpoint} -> the updated endpoint.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/webhook-endpoints/{endpoint}", tag = "webhooks",
    operation_id = "webhooks_update", summary = "Update a webhook endpoint",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("endpoint" = Uuid, Path, description = "Endpoint id"),
    ),
    request_body = UpdateEndpointRequest,
    responses(
        (status = 200, body = Envelope<WebhookEndpointResource>, description = "The updated endpoint"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or endpoint, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): EndpointPath,
    ValidatedJson(patch): ValidatedJson<UpdateEndpointRequest>,
) -> Result<Data<WebhookEndpointResource>, AppError> {
    admin(&access)?;
    let row = service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Data(row.into()))
}

/// DELETE /webhook-endpoints/{endpoint} -> 204.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/webhook-endpoints/{endpoint}", tag = "webhooks",
    operation_id = "webhooks_delete", summary = "Delete a webhook endpoint",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("endpoint" = Uuid, Path, description = "Endpoint id"),
    ),
    responses(
        (status = 204, description = "Deleted, with its delivery history"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or endpoint, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): EndpointPath,
) -> Result<StatusCode, AppError> {
    admin(&access)?;
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
