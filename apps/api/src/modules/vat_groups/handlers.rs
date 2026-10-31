//! HTTP edge for /v1/workspace/{workspace}/settings/vat-groups: `vatgroup.*` permission
//! (via `Can`), call service, wrap.

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
        list::{Page, Paginated, search},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::vat_groups::{
        dto::{
            CreateVatGroupRequest, ListQuery, UpdateVatGroupRequest, VatGroupCreated,
            VatGroupResource,
        },
        repo::ListFilter,
        service,
    },
};

// Laravel checked `vatgroup.view` for one row; genesy uses the catalog's `get`.
permissions! {
    VatGroupCreate => "vatgroup.create",
    VatGroupList => "vatgroup.list",
    VatGroupGet => "vatgroup.get",
    VatGroupUpdate => "vatgroup.update",
    VatGroupDelete => "vatgroup.delete",
}

/// `{workspace}` + `{vat_group}`; the id stays raw so a malformed one is a 404.
type VatGroupPath = Path<(String, String)>;

/// GET /settings/vat-groups -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings/vat-groups", tag = "vat_groups",
    operation_id = "vat_groups_list", summary = "List VAT groups",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ListQuery,
    ),
    responses(
        (status = 200, body = Paginated<VatGroupResource>, description = "One page of VAT groups"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `vatgroup.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long, or `country` not an ISO code"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<VatGroupList>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<VatGroupResource>>, AppError> {
    let pattern = search(query.q.as_deref())?;
    let country = query.country()?;
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let filter = ListFilter {
        country: country.as_deref(),
        pattern: pattern.as_deref(),
        limit: page.size,
        offset: page.offset(),
    };
    let (rows, total) = service::list(&state, access.workspace_id, &filter).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /settings/vat-groups/{vat_group}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings/vat-groups/{vat_group}", tag = "vat_groups",
    operation_id = "vat_groups_get", summary = "Get a VAT group",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("vat_group" = Uuid, Path, description = "VAT group id"),
    ),
    responses(
        (status = 200, body = Envelope<VatGroupResource>, description = "The VAT group"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `vatgroup.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or VAT group, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<VatGroupGet>,
    Path((_, id)): VatGroupPath,
) -> Result<Data<VatGroupResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /settings/vat-groups -> 201 `{data:{vat_group_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/settings/vat-groups", tag = "vat_groups",
    operation_id = "vat_groups_create", summary = "Create a VAT group",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateVatGroupRequest,
    responses(
        (status = 201, body = Envelope<VatGroupCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `vatgroup.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<VatGroupCreate>,
    ValidatedJson(input): ValidatedJson<CreateVatGroupRequest>,
) -> Result<(StatusCode, Data<VatGroupCreated>), AppError> {
    let vat_group_id = service::create(&state, access.workspace_id, input).await?;
    Ok((StatusCode::CREATED, Data(VatGroupCreated { vat_group_id })))
}

/// POST /settings/vat-groups/{vat_group} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/settings/vat-groups/{vat_group}", tag = "vat_groups",
    operation_id = "vat_groups_update", summary = "Update a VAT group (`name` and `vat_amount` always required)",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("vat_group" = Uuid, Path, description = "VAT group id"),
    ),
    request_body = UpdateVatGroupRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `vatgroup.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or VAT group, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<VatGroupUpdate>,
    Path((_, id)): VatGroupPath,
    ValidatedJson(patch): ValidatedJson<UpdateVatGroupRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /settings/vat-groups/{vat_group} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/settings/vat-groups/{vat_group}", tag = "vat_groups",
    operation_id = "vat_groups_delete", summary = "Delete a VAT group",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("vat_group" = Uuid, Path, description = "VAT group id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `vatgroup.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or VAT group, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<VatGroupDelete>,
    Path((_, id)): VatGroupPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
