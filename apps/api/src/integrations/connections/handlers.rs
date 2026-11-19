//! HTTP edge for /v1/workspace/{workspace}/integrations/connections (contracts §5.2).

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
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    integrations::{
        PageQuery,
        connections::{
            dto::{ConnectionCreated, ConnectionInput, ConnectionResource},
            service,
        },
    },
};

permissions! {
    IntegrationView => "integration.view",
    IntegrationManage => "integration.manage",
}

/// `{workspace}` + `{connection}`; the id stays raw so a malformed one is a 404.
type ConnectionPath = Path<(String, String)>;

/// GET /integrations/connections.
#[utoipa::path(
    get, path = "/workspace/{workspace}/integrations/connections", tag = "integrations",
    operation_id = "integrations_connections_list", summary = "List connections",
    params(("workspace" = Uuid, Path, description = "Workspace id"), PageQuery),
    responses(
        (status = 200, body = Paginated<ConnectionResource>, description = "One page of connections"),
        (status = 403, body = MessageBody, description = "No integration.view"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Paginated<ConnectionResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) =
        service::list(&state, access.workspace_id, page.size, page.offset()).await?;
    let data = rows
        .into_iter()
        .map(|(r, s)| ConnectionResource::new(r, s))
        .collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /integrations/connections.
#[utoipa::path(
    post, path = "/workspace/{workspace}/integrations/connections", tag = "integrations",
    operation_id = "integrations_connections_create", summary = "Create a connection (draft)",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = ConnectionInput,
    responses(
        (status = 201, body = Envelope<ConnectionCreated>, description = "Created"),
        (status = 403, body = MessageBody, description = "No integration.manage"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid fields, label taken, inward address, or no credential key"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    ValidatedJson(input): ValidatedJson<ConnectionInput>,
) -> Result<(StatusCode, Data<ConnectionCreated>), AppError> {
    let connection_id = service::create(&state, access.workspace_id, access.user_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(ConnectionCreated { connection_id }),
    ))
}

/// GET /integrations/connections/{connection}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/integrations/connections/{connection}", tag = "integrations",
    operation_id = "integrations_connections_get", summary = "Get a connection",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("connection" = Uuid, Path, description = "Connection id"),
    ),
    responses(
        (status = 200, body = Envelope<ConnectionResource>, description = "The connection"),
        (status = 403, body = MessageBody, description = "No integration.view"),
        (status = 404, body = MessageBody, description = "Unknown workspace or connection"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Path((_, id)): ConnectionPath,
) -> Result<Data<ConnectionResource>, AppError> {
    let (row, stored) = service::get(&state, access.workspace_id, &id).await?;
    Ok(Data(ConnectionResource::new(row, stored)))
}

/// POST /integrations/connections/{connection}.
#[utoipa::path(
    post, path = "/workspace/{workspace}/integrations/connections/{connection}", tag = "integrations",
    operation_id = "integrations_connections_update", summary = "Update a connection (send only changed fields)",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("connection" = Uuid, Path, description = "Connection id"),
    ),
    request_body = ConnectionInput,
    responses(
        (status = 200, body = MessageBody, description = "Saved"),
        (status = 403, body = MessageBody, description = "No integration.manage"),
        (status = 404, body = MessageBody, description = "Unknown workspace or connection"),
        (status = 422, body = ValidationErrorBody, description = "Invalid fields, or the mask sent after the host moved"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, id)): ConnectionPath,
    ValidatedJson(input): ValidatedJson<ConnectionInput>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, access.user_id, &id, input).await?;
    Ok(Message("ok"))
}

/// POST /integrations/connections/{connection}/test.
#[utoipa::path(
    post, path = "/workspace/{workspace}/integrations/connections/{connection}/test", tag = "integrations",
    operation_id = "integrations_connections_test", summary = "Probe a connection and update its status",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("connection" = Uuid, Path, description = "Connection id"),
    ),
    responses(
        (status = 200, body = Envelope<ConnectionResource>, description = "The connection with its new probe"),
        (status = 403, body = MessageBody, description = "No integration.manage"),
        (status = 404, body = MessageBody, description = "Unknown workspace or connection"),
    ),
    security(("bearer" = []))
)]
pub async fn test(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, id)): ConnectionPath,
) -> Result<Data<ConnectionResource>, AppError> {
    let (row, stored) = service::test(&state, access.workspace_id, &id).await?;
    Ok(Data(ConnectionResource::new(row, stored)))
}

/// POST /integrations/connections/{connection}/disconnect.
#[utoipa::path(
    post, path = "/workspace/{workspace}/integrations/connections/{connection}/disconnect", tag = "integrations",
    operation_id = "integrations_connections_disconnect", summary = "Disconnect and drop the stored secret",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("connection" = Uuid, Path, description = "Connection id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Disconnected"),
        (status = 403, body = MessageBody, description = "No integration.manage"),
        (status = 404, body = MessageBody, description = "Unknown workspace or connection"),
    ),
    security(("bearer" = []))
)]
pub async fn disconnect(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, id)): ConnectionPath,
) -> Result<Message, AppError> {
    service::disconnect(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
