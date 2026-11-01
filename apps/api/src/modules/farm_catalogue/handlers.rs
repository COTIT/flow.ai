//! HTTP edge for /v1/workspace/{workspace}/farm/equipment-categories and /farm/equipment.

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
    modules::farm_catalogue::{
        dto::{
            EquipmentCategoryRequest, EquipmentCategoryResource, EquipmentRequest,
            EquipmentResource, ListQuery,
        },
        service,
    },
};

/// 201 for a new row, 200 for a replay.
fn created(new: bool) -> StatusCode {
    if new {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    }
}

/// GET /farm/equipment-categories -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/equipment-categories", tag = "farm_equipment",
    operation_id = "farm_equipment_categories_list", summary = "List equipment categories",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<EquipmentCategoryResource>, description = "One page, by English name"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
    ),
    security(("bearer" = []))
)]
pub async fn list_categories(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<EquipmentCategoryResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) =
        service::list_categories(&state, &access, query.active(), page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /farm/equipment-categories -> 201 created, 200 replayed.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/equipment-categories", tag = "farm_equipment",
    operation_id = "farm_equipment_categories_create", summary = "Add an equipment category",
    description = "ops or admin.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = EquipmentCategoryRequest,
    responses(
        (status = 200, body = Envelope<EquipmentCategoryResource>, description = "Replayed: same id, same body"),
        (status = 201, body = Envelope<EquipmentCategoryResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 409, body = MessageBody, description = "The id holds a different category, or is used elsewhere"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn create_category(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<EquipmentCategoryRequest>,
) -> Result<(StatusCode, Data<EquipmentCategoryResource>), AppError> {
    let (row, new) = service::create_category(&state, &access, input).await?;
    Ok((created(new), Data(row.into())))
}

/// GET /farm/equipment -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/equipment", tag = "farm_equipment",
    operation_id = "farm_equipment_list", summary = "List equipment",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<EquipmentResource>, description = "One page, by English name"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
    ),
    security(("bearer" = []))
)]
pub async fn list_equipment(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<EquipmentResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) =
        service::list_equipment(&state, &access, query.active(), page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /farm/equipment/{equipment}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/equipment/{equipment}", tag = "farm_equipment",
    operation_id = "farm_equipment_get", summary = "Get a machine",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("equipment" = Uuid, Path, description = "Equipment id"),
    ),
    responses(
        (status = 200, body = Envelope<EquipmentResource>, description = "The machine"),
        (status = 404, body = MessageBody, description = "Unknown workspace or machine"),
    ),
    security(("bearer" = []))
)]
pub async fn get_equipment(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<EquipmentResource>, AppError> {
    Ok(Data(
        service::get_equipment(&state, &access, &id).await?.into(),
    ))
}

/// POST /farm/equipment -> 201 created, 200 replayed.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/equipment", tag = "farm_equipment",
    operation_id = "farm_equipment_create", summary = "Add a machine",
    description = "ops or admin. Refused with `equipment must be offered to rent or to buy` \
    when both offers are false.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = EquipmentRequest,
    responses(
        (status = 200, body = Envelope<EquipmentResource>, description = "Replayed: same id, same body"),
        (status = 201, body = Envelope<EquipmentResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role"),
        (status = 409, body = MessageBody, description = "The id holds a different machine, or is used elsewhere"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown category or currency, or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn create_equipment(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<EquipmentRequest>,
) -> Result<(StatusCode, Data<EquipmentResource>), AppError> {
    let (row, new) = service::create_equipment(&state, &access, input).await?;
    Ok((created(new), Data(row.into())))
}
