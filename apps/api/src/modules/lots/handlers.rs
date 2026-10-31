//! HTTP edge for /v1/workspace/{workspace}/lots (members read, `product.update` writes) and
//! the lot's public page at /v1/public/lots/{code} and /v1/storefront/lots/{code}.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        app_key::AppKey,
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, MessageBody},
        workspace::{Can, WorkspaceAccess},
    },
    modules::{
        app_keys::secret::CATALOG_READ,
        lots::{
            dto::{
                CreateLotRequest, ListQuery, LotEventResource, LotResource, PageQuery,
                PassportResource, UpdateLotRequest,
            },
            passport::Passport,
            public::PublicLot,
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

/// GET /lots/{lot}/passport.
#[utoipa::path(
    get, path = "/workspace/{workspace}/lots/{lot}/passport", tag = "lots",
    operation_id = "lots_passport_get", summary = "Get a lot's passport",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
    ),
    responses(
        (status = 200, body = Envelope<PassportResource>, description = "Draft and published copy"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn passport_get(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): LotPath,
) -> Result<Data<PassportResource>, AppError> {
    let row = service::passport(&state, access.workspace_id, &id).await?;
    Ok(Data(PassportResource::try_from(row)?))
}

/// PUT /lots/{lot}/passport -> draft replaced.
#[utoipa::path(
    put, path = "/workspace/{workspace}/lots/{lot}/passport", tag = "lots",
    operation_id = "lots_passport_save", summary = "Replace a lot's passport draft",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
    ),
    request_body = Passport,
    responses(
        (status = 200, body = Envelope<PassportResource>, description = "Draft saved; the published copy is unchanged"),
        (status = 403, body = MessageBody, description = "Inactive membership, missing `product.update`, or files referenced without `file.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Unknown keys, limits, or files that are not this workspace's images/PDFs"),
    ),
    security(("bearer" = []))
)]
pub async fn passport_save(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, id)): LotPath,
    ValidatedJson(draft): ValidatedJson<Passport>,
) -> Result<Data<PassportResource>, AppError> {
    let row = service::save_draft(&state, &access, &id, draft).await?;
    Ok(Data(PassportResource::try_from(row)?))
}

/// POST /lots/{lot}/passport/publish -> next published version.
#[utoipa::path(
    post, path = "/workspace/{workspace}/lots/{lot}/passport/publish", tag = "lots",
    operation_id = "lots_passport_publish", summary = "Publish a lot's passport draft",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
    ),
    responses(
        (status = 200, body = Envelope<PassportResource>, description = "Published; `version` went up by one"),
        (status = 403, body = MessageBody, description = "Inactive membership, missing `product.update`, or files referenced without `file.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "No title, or a file is no longer valid"),
    ),
    security(("bearer" = []))
)]
pub async fn passport_publish(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, id)): LotPath,
) -> Result<Data<PassportResource>, AppError> {
    let row = service::publish(&state, &access, &id).await?;
    Ok(Data(PassportResource::try_from(row)?))
}

/// DELETE /lots/{lot}/passport/publish -> published copy hidden.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/lots/{lot}/passport/publish", tag = "lots",
    operation_id = "lots_passport_unpublish", summary = "Unpublish a lot's passport",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
    ),
    responses(
        (status = 200, body = Envelope<PassportResource>, description = "Hidden (a no-op when not published); draft and version stay"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn passport_unpublish(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, id)): LotPath,
) -> Result<Data<PassportResource>, AppError> {
    let row = service::unpublish(&state, &access, &id).await?;
    Ok(Data(PassportResource::try_from(row)?))
}

/// GET /public/lots/{code}: the page a scanned jar opens. No auth; behind the IP limit.
#[utoipa::path(
    get, path = "/public/lots/{code}", tag = "lots",
    operation_id = "lots_public_get", summary = "Public page of a lot",
    params(("code" = String, Path, description = "The lot's public code (from the QR link)")),
    responses(
        (status = 200, body = Envelope<PublicLot>, description = "Batch, product and published passport"),
        (status = 404, body = MessageBody, description = "No published lot with this code"),
    ),
)]
pub async fn public_get(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Data<PublicLot>, AppError> {
    Ok(Data(service::public(&state, &code, None).await?))
}

/// GET /storefront/lots/{code}: the same page for an app of the lot's workspace (the shop
/// server, which would share one IP bucket on the public route).
#[utoipa::path(
    get, path = "/storefront/lots/{code}", tag = "storefront",
    operation_id = "storefront_lots_get", summary = "Public page of a lot",
    params(("code" = String, Path, description = "The lot's public code (from the QR link)")),
    responses(
        (status = 200, body = Envelope<PublicLot>, description = "Batch, product and published passport"),
        (status = 401, body = MessageBody, description = "Missing, unknown or revoked app key"),
        (status = 403, body = MessageBody, description = "Key lacks `catalog:read`"),
        (status = 404, body = MessageBody, description = "No published lot with this code in the key's workspace"),
    ),
    security(("app_key" = []))
)]
pub async fn storefront_get(
    State(state): State<AppState>,
    key: AppKey,
    Path(code): Path<String>,
) -> Result<Data<PublicLot>, AppError> {
    key.require(CATALOG_READ)?;
    Ok(Data(
        service::public(&state, &code, Some(key.workspace_id)).await?,
    ))
}
