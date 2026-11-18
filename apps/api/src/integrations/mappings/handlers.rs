//! HTTP edge for /v1/workspace/{workspace}/integrations/pairs/{pair}/mappings (contracts §5.2).

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
        workspace::Can,
    },
    integrations::{
        connections::handlers::{IntegrationManage, IntegrationView},
        mappings::{
            dto::{MappingCreated, MappingInput, MappingQuery, MappingResource},
            service,
        },
    },
};

/// GET /integrations/pairs/{pair}/mappings.
#[utoipa::path(
    get, path = "/workspace/{workspace}/integrations/pairs/{pair}/mappings", tag = "integrations",
    operation_id = "integrations_mappings_list", summary = "List a pair's mappings",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("pair" = Uuid, Path, description = "Pair id"),
        MappingQuery,
    ),
    responses(
        (status = 200, body = Paginated<MappingResource>, description = "One page of mappings"),
        (status = 403, body = MessageBody, description = "No integration.view"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pair"),
        (status = 422, body = ValidationErrorBody, description = "Unknown entity"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<IntegrationView>,
    Path((_, pair)): Path<(String, String)>,
    Query(q): Query<MappingQuery>,
) -> Result<Json<Paginated<MappingResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let entity = q.entity.as_deref().map(str::trim).filter(|e| !e.is_empty());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        &pair,
        entity,
        page.size,
        page.offset(),
    )
    .await?;
    Ok(Json(Paginated::new(
        rows.into_iter().map(Into::into).collect(),
        total,
        page,
    )))
}

/// POST /integrations/pairs/{pair}/mappings.
#[utoipa::path(
    post, path = "/workspace/{workspace}/integrations/pairs/{pair}/mappings", tag = "integrations",
    operation_id = "integrations_mappings_upsert", summary = "Create or replace a mapping",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("pair" = Uuid, Path, description = "Pair id"),
    ),
    request_body = MappingInput,
    responses(
        (status = 201, body = Envelope<MappingCreated>, description = "Saved"),
        (status = 403, body = MessageBody, description = "No integration.manage"),
        (status = 404, body = MessageBody, description = "Unknown workspace or pair"),
        (status = 422, body = ValidationErrorBody, description = "Invalid fields, or a user target that isn't an active member"),
    ),
    security(("bearer" = []))
)]
pub async fn upsert(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, pair)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<MappingInput>,
) -> Result<(StatusCode, Data<MappingCreated>), AppError> {
    let mapping_id =
        service::upsert(&state, access.workspace_id, access.user_id, &pair, input).await?;
    Ok((StatusCode::CREATED, Data(MappingCreated { mapping_id })))
}

/// DELETE /integrations/pairs/{pair}/mappings/{mapping}.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/integrations/pairs/{pair}/mappings/{mapping}", tag = "integrations",
    operation_id = "integrations_mappings_delete", summary = "Delete a mapping",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("pair" = Uuid, Path, description = "Pair id"),
        ("mapping" = Uuid, Path, description = "Mapping id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Deleted"),
        (status = 403, body = MessageBody, description = "No integration.manage"),
        (status = 404, body = MessageBody, description = "Unknown workspace, pair or mapping"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<IntegrationManage>,
    Path((_, pair, id)): Path<(String, String, String)>,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &pair, &id).await?;
    Ok(Message("ok"))
}
