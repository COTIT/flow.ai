//! HTTP edge for /v1/workspace/{workspace}/farm/memberships: grant and revoke farm staff roles.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        farm::FarmAccess,
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_memberships::{
        dto::{GrantRequest, ListQuery, MembershipResource},
        service,
    },
};

/// GET /farm/memberships -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/memberships", tag = "farm_memberships",
    operation_id = "farm_memberships_list", summary = "List farm roles",
    description = "ops, admin and workspace admins see every role in the workspace; others their own.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<MembershipResource>, description = "One page, newest first"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<MembershipResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(&state, &access, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /farm/memberships -> 201 new, 200 repeat or restored.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/memberships", tag = "farm_memberships",
    operation_id = "farm_memberships_create", summary = "Grant a farm staff role",
    description = "Farm admin grants any staff role, ops grants field officers; nobody grants \
        themselves, except a workspace admin. A repeat grant returns the existing role; a revoked \
        one is restored.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = GrantRequest,
    responses(
        (status = 200, body = Envelope<MembershipResource>, description = "Already held, or restored"),
        (status = 201, body = Envelope<MembershipResource>, description = "Granted"),
        (status = 403, body = MessageBody, description = "Caller may not grant this role to this user"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown village, or user not an active member"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<GrantRequest>,
) -> Result<(StatusCode, Data<MembershipResource>), AppError> {
    let (row, created) = service::grant(&state, &access, input).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(row.into())))
}

/// DELETE /farm/memberships/{membership} -> 204; the row stays with `revoked_at`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/farm/memberships/{membership}", tag = "farm_memberships",
    operation_id = "farm_memberships_revoke", summary = "Revoke a farm role",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("membership" = Uuid, Path, description = "Membership id"),
    ),
    responses(
        (status = 204, description = "Revoked (also when it already was)"),
        (status = 403, body = MessageBody, description = "Caller may not revoke this role"),
        (status = 404, body = MessageBody, description = "Unknown workspace or membership"),
    ),
    security(("bearer" = []))
)]
pub async fn revoke(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    service::revoke(&state, &access, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
