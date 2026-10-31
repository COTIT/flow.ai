//! HTTP edge for /v1/workspace/{workspace}/products/price-corrections (`pricecorrection.*`).

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
    modules::price_corrections::{
        dto::{
            CorrectionCreated, CorrectionRequest, CorrectionResource, CreateCorrectionRequest,
            ListQuery, UpdateCorrectionRequest,
        },
        service,
    },
};

permissions! {
    CorrectionList => "pricecorrection.list",
    CorrectionView => "pricecorrection.view",
    CorrectionCreate => "pricecorrection.create",
    CorrectionUpdate => "pricecorrection.update",
    CorrectionDelete => "pricecorrection.delete",
}

type CorrectionPath = Path<(String, String)>;

/// GET /price-corrections.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/price-corrections", tag = "price_corrections",
    operation_id = "price_corrections_list", summary = "List price corrections",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<CorrectionResource>, description = "One page of price corrections"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pricecorrection.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<CorrectionList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<CorrectionResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) =
        service::list(&state, access.workspace_id, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /price-corrections/{id}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/price-corrections/{correction}", tag = "price_corrections",
    operation_id = "price_corrections_get", summary = "Get a price correction",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("correction" = Uuid, Path, description = "Price correction id")),
    responses(
        (status = 200, body = Envelope<CorrectionResource>, description = "The price correction"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pricecorrection.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or price correction, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<CorrectionView>,
    Path((_, id)): CorrectionPath,
) -> Result<Data<CorrectionResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /price-corrections -> 201 `{data:{price_correction_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/price-corrections", tag = "price_corrections",
    operation_id = "price_corrections_create", summary = "Create a price correction",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CorrectionRequest,
    responses(
        (status = 201, body = Envelope<CorrectionCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pricecorrection.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<CorrectionCreate>,
    ValidatedJson(input): ValidatedJson<CreateCorrectionRequest>,
) -> Result<(StatusCode, Data<CorrectionCreated>), AppError> {
    let price_correction_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(CorrectionCreated {
            price_correction_id,
        }),
    ))
}

/// POST /price-corrections/{id}.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/price-corrections/{correction}", tag = "price_corrections",
    operation_id = "price_corrections_update", summary = "Update a price correction (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("correction" = Uuid, Path, description = "Price correction id")),
    request_body = CorrectionRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pricecorrection.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or price correction, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<CorrectionUpdate>,
    Path((_, id)): CorrectionPath,
    ValidatedJson(input): ValidatedJson<UpdateCorrectionRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /price-corrections/{id}.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/products/price-corrections/{correction}", tag = "price_corrections",
    operation_id = "price_corrections_delete", summary = "Delete a price correction",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("correction" = Uuid, Path, description = "Price correction id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `pricecorrection.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or price correction, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<CorrectionDelete>,
    Path((_, id)): CorrectionPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
