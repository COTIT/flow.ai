//! HTTP edge for /v1/workspace/{workspace}/supply-links: workspace admins grant, accept, list
//! and revoke.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Keyed},
    modules::supply_links::{
        dto::{GrantLinkRequest, LinkResource, ListQuery},
        repo::Filter,
        service,
    },
};

permissions! {
    WorkspaceAdmin => "workspace_admin",
}

/// GET /supply-links -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/supply-links", tag = "supply_links",
    operation_id = "supply_links_list", summary = "List supply links",
    description = "Links this workspace granted (`outgoing`) or received (`incoming`), newest first.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<LinkResource>, description = "One page"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown `status` or `direction`"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<LinkResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (status, direction) = query.filter()?;
    let f = Filter {
        id: None,
        status: status.as_deref(),
        direction: direction.as_deref(),
    };
    let (rows, total) = service::list(&state, &access, f, page.size, page.offset()).await?;
    let ws = access.workspace_id;
    let data = rows.into_iter().map(|l| LinkResource::new(l, ws)).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /supply-links -> 201 pending link.
#[utoipa::path(
    post, path = "/workspace/{workspace}/supply-links", tag = "supply_links",
    operation_id = "supply_links_grant", summary = "Grant a supply link",
    description = "From this (farm) workspace to a shop workspace, with the public farmer fields it may show. Names and photos are refused until the data-protection gate opens. One pending or live link per pair.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer; another request under the key is a 422. Kept for a day."),
    ),
    request_body = GrantLinkRequest,
    responses(
        (status = 201, body = Envelope<LinkResource>, description = "Granted, pending acceptance (or the first answer again, for a retried Idempotency-Key)"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 409, body = MessageBody, description = "A pending or live link to that workspace exists"),
        (status = 422, body = ValidationErrorBody, description = "Unknown or gated field, bad target workspace, or Idempotency-Key reused for another request"),
    ),
    security(("bearer" = []))
)]
pub async fn grant(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Keyed { input, key }: Keyed<GrantLinkRequest>,
) -> Result<Answer, AppError> {
    service::grant(&state, &access, input, key).await
}

/// POST /supply-links/{link}/accept.
#[utoipa::path(
    post, path = "/workspace/{workspace}/supply-links/{link}/accept", tag = "supply_links",
    operation_id = "supply_links_accept", summary = "Accept a supply link",
    description = "The receiving workspace's admin accepts a pending link; it is then live.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("link" = Uuid, Path, description = "Link id"),
    ),
    responses(
        (status = 200, body = Envelope<LinkResource>, description = "Live"),
        (status = 403, body = MessageBody, description = "Not a workspace admin, or this workspace granted the link"),
        (status = 404, body = MessageBody, description = "Unknown link, or this workspace is no side of it"),
        (status = 409, body = MessageBody, description = "Not pending (live or revoked)"),
    ),
    security(("bearer" = []))
)]
pub async fn accept(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<LinkResource>, AppError> {
    let link = service::accept(&state, &access, &id).await?;
    Ok(Data(LinkResource::new(link, access.workspace_id)))
}

/// DELETE /supply-links/{link}.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/supply-links/{link}", tag = "supply_links",
    operation_id = "supply_links_revoke", summary = "Revoke a supply link",
    description = "Either side's admin. Revoking again is also 204; a revoked link never comes back.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("link" = Uuid, Path, description = "Link id"),
    ),
    responses(
        (status = 204, description = "Revoked"),
        (status = 403, body = MessageBody, description = "Inactive membership, or caller not a workspace admin"),
        (status = 404, body = MessageBody, description = "Unknown link, or this workspace is no side of it"),
    ),
    security(("bearer" = []))
)]
pub async fn revoke(
    State(state): State<AppState>,
    access: Can<WorkspaceAdmin>,
    Path((_, id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    service::revoke(&state, &access, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
