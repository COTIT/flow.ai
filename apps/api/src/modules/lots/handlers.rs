//! HTTP edge for /v1/workspace/{workspace}/lots: members read, `product.update` writes.

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
        workspace::{Can, WorkspaceAccess},
    },
    modules::{
        lots::{
            dto::{
                CreateLotRequest, ListQuery, LotEventResource, LotResource, PageQuery,
                UpdateLotRequest,
            },
            service,
        },
        products::handlers::ProductUpdate,
    },
};

/// `{workspace}` + `{lot}`; the id stays raw so a malformed one is a 404.
type LotPath = Path<(String, String)>;

/// GET /lots -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/lots", tag = "lots",
    operation_id = "lots_list", summary = "List lots",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<LotResource>, description = "One page of lots, newest first"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<LotResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        query.product_id.as_deref(),
        query.status.as_deref(),
        query.q.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /lots -> 201 `{data}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/lots", tag = "lots",
    operation_id = "lots_create", summary = "Create a lot",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateLotRequest,
    responses(
        (status = 201, body = Envelope<LotResource>, description = "Created, with its public code"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown or untracked product, or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    ValidatedJson(input): ValidatedJson<CreateLotRequest>,
) -> Result<(StatusCode, Data<LotResource>), AppError> {
    let lot = service::create(&state, &access, input).await?;
    Ok((StatusCode::CREATED, Data(lot.into())))
}

/// GET /lots/{lot}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/lots/{lot}", tag = "lots",
    operation_id = "lots_get", summary = "Get a lot",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
    ),
    responses(
        (status = 200, body = Envelope<LotResource>, description = "The lot"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): LotPath,
) -> Result<Data<LotResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// PATCH /lots/{lot} -> the updated lot.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/lots/{lot}", tag = "lots",
    operation_id = "lots_update", summary = "Update a lot",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
    ),
    request_body = UpdateLotRequest,
    responses(
        (status = 200, body = Envelope<LotResource>, description = "The updated lot"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, id)): LotPath,
    ValidatedJson(patch): ValidatedJson<UpdateLotRequest>,
) -> Result<Data<LotResource>, AppError> {
    Ok(Data(
        service::update(&state, &access, &id, patch).await?.into(),
    ))
}

/// GET /lots/{lot}/events -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/lots/{lot}/events", tag = "lots",
    operation_id = "lots_events", summary = "List a lot's trail",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
        PageQuery,
    ),
    responses(
        (status = 200, body = Paginated<LotEventResource>, description = "One page of events, newest first"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn events(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): LotPath,
    Query(query): Query<PageQuery>,
) -> Result<Json<Paginated<LotEventResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total) =
        service::events(&state, access.workspace_id, &id, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}
