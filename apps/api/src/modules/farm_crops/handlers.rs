//! HTTP edge for /v1/workspace/{workspace}/farm/crops.

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
    modules::farm_crops::{
        dto::{CreateCropRequest, CropResource, ListQuery, UpdateCropRequest},
        service,
    },
};

/// GET /farm/crops -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/crops", tag = "farm_crops",
    operation_id = "farm_crops_list", summary = "List crops",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<CropResource>, description = "One page, by code"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<CropResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(&state, &access, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /farm/crops -> 201.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/crops", tag = "farm_crops",
    operation_id = "farm_crops_create", summary = "Add a crop",
    description = "ops or admin.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateCropRequest,
    responses(
        (status = 201, body = Envelope<CropResource>, description = "Added"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<CreateCropRequest>,
) -> Result<(StatusCode, Data<CropResource>), AppError> {
    let row = service::create(&state, &access, input).await?;
    Ok((StatusCode::CREATED, Data(row.into())))
}

/// PATCH /farm/crops/{crop}.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/farm/crops/{crop}", tag = "farm_crops",
    operation_id = "farm_crops_update", summary = "Change a crop",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("crop" = Uuid, Path, description = "Crop id"),
    ),
    request_body = UpdateCropRequest,
    responses(
        (status = 200, body = Envelope<CropResource>, description = "Changed"),
        (status = 403, body = MessageBody, description = "Caller is not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace or crop"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(patch): ValidatedJson<UpdateCropRequest>,
) -> Result<Data<CropResource>, AppError> {
    Ok(Data(
        service::update(&state, &access, &id, patch).await?.into(),
    ))
}
