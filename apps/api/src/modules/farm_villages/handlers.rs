//! HTTP edge for /v1/workspace/{workspace}/farm/villages.

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
    modules::farm_villages::{
        dto::{
            CapacityRequest, CapacityResource, CreateVillageRequest, ListQuery,
            UpdateVillageRequest, VillageResource,
        },
        service,
    },
};

/// GET /farm/villages -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/villages", tag = "farm_villages",
    operation_id = "farm_villages_list", summary = "List villages",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<VillageResource>, description = "One page, by code"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<VillageResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(&state, &access, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /farm/villages -> 201 created, 200 replayed.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/villages", tag = "farm_villages",
    operation_id = "farm_villages_create", summary = "Create a village",
    description = "ops or admin. The village is created with its first capacity in one \
    transaction; a missing capacity is refused with `a planned capacity is required`. A repeat \
    with the same `id` returns the village already made.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateVillageRequest,
    responses(
        (status = 200, body = Envelope<VillageResource>, description = "Replayed: this id already exists"),
        (status = 201, body = Envelope<VillageResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 409, body = MessageBody, description = "The id is used outside this workspace"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<CreateVillageRequest>,
) -> Result<(StatusCode, Data<VillageResource>), AppError> {
    let (row, created) = service::create(&state, &access, input).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(row.into())))
}

/// PATCH /farm/villages/{village}.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/farm/villages/{village}", tag = "farm_villages",
    operation_id = "farm_villages_update", summary = "Change a village",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("village" = Uuid, Path, description = "Village id"),
    ),
    request_body = UpdateVillageRequest,
    responses(
        (status = 200, body = Envelope<VillageResource>, description = "Changed"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or village"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(patch): ValidatedJson<UpdateVillageRequest>,
) -> Result<Data<VillageResource>, AppError> {
    Ok(Data(
        service::update(&state, &access, &id, patch).await?.into(),
    ))
}

/// POST /farm/villages/{village}/capacity -> 201.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/villages/{village}/capacity", tag = "farm_villages",
    operation_id = "farm_villages_capacity_create", summary = "Record a new village capacity",
    description = "ops or admin. The new figure becomes current; the previous one is kept.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("village" = Uuid, Path, description = "Village id"),
    ),
    request_body = CapacityRequest,
    responses(
        (status = 200, body = Envelope<CapacityResource>, description = "Replayed: same id, same figure"),
        (status = 201, body = Envelope<CapacityResource>, description = "Recorded"),
        (status = 409, body = MessageBody, description = "The id holds another figure or village, or is used elsewhere"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or village"),
        (status = 422, body = ValidationErrorBody, description = "Invalid figure (e.g. basis `measured`)"),
    ),
    security(("bearer" = []))
)]
pub async fn add_capacity(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, village)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<CapacityRequest>,
) -> Result<(StatusCode, Data<CapacityResource>), AppError> {
    let (row, new) = service::add_capacity(&state, &access, &village, input).await?;
    let status = if new {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(row.into())))
}
