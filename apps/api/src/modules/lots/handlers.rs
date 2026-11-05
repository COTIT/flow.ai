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
    idempotency::{Answer, Keyed},
    modules::{
        app_keys::secret::CATALOG_READ,
        lots::{
            dto::{
                CreateLotRequest, FarmerCreditsPage, ListQuery, LotEventResource, LotFreeStock, LotHold,
                LotResource, LotStockLevel, PageQuery, PassportResource, StockMutationRequest,
                UpdateLotRequest,
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
    let data = service::resources(&state, access.workspace_id, rows).await?;
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
    Ok((StatusCode::CREATED, Data(LotResource::new(lot, None))))
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
        service::resource(
            &state,
            access.workspace_id,
            service::get(&state, access.workspace_id, &id).await?,
        )
        .await?,
    ))
}

/// GET /lots/{lot}/stock -> the lot's stock per warehouse.
#[utoipa::path(
    get, path = "/workspace/{workspace}/lots/{lot}/stock", tag = "lots",
    operation_id = "lots_stock", summary = "A lot's free stock per warehouse",
    description = "One row per warehouse holding the lot, by warehouse name: `level`, `reserved` (held by orders) and `available`, as 2-decimal text. Same visibility as the lot.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<LotFreeStock>>, description = "Stock per warehouse; empty when none"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn stock(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): LotPath,
) -> Result<Data<Vec<LotFreeStock>>, AppError> {
    let rows = service::free_stock(&state, access.workspace_id, &id).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
}

/// GET /lots/{lot}/holds -> orders still holding this lot.
#[utoipa::path(
    get, path = "/workspace/{workspace}/lots/{lot}/holds", tag = "lots",
    operation_id = "lots_holds", summary = "Orders still holding a lot",
    description = "Orders whose hold could not move off this lot when it went into quarantine or was recalled (no other sellable lot of the product at that warehouse had room for the whole hold). Oldest first. Cancel such an order, or release the lot, to clear it.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<LotHold>>, description = "Orders holding the lot; empty when none"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn holds(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): LotPath,
) -> Result<Data<Vec<LotHold>>, AppError> {
    let rows = service::holds(&state, access.workspace_id, &id).await?;
    Ok(Data(rows.into_iter().map(Into::into).collect()))
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
        (status = 409, body = MessageBody, description = "`lot_status_final`: a recalled lot keeps its status"),
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
        service::resource(
            &state,
            access.workspace_id,
            service::update(&state, &access, &id, patch).await?,
        )
        .await?,
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

/// GET /lots/{lot}/farmer-credits -> `{data:[...], meta, farmer_groups}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/lots/{lot}/farmer-credits", tag = "lots",
    operation_id = "lots_farmer_credits", summary = "Sales and returns of a lot that credited its farmers",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
        PageQuery,
    ),
    responses(
        (status = 200, body = FarmerCreditsPage, description = "One page, newest first; no farmer data or amounts"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn farmer_credits(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, id)): LotPath,
    Query(query): Query<PageQuery>,
) -> Result<Json<FarmerCreditsPage>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (rows, total, farmers) =
        service::farmer_credits(&state, access.workspace_id, &id, page.size, page.offset()).await?;
    let data = rows.into_iter().map(Into::into).collect();
    let (data, meta) = Paginated::new(data, total, page).into_parts();
    let farmer_groups =
        (!farmers.is_empty()).then(|| i64::try_from(farmers.len()).unwrap_or(i64::MAX));
    Ok(Json(FarmerCreditsPage {
        data,
        meta,
        farmer_groups,
        farmers,
    }))
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

/// POST /lots/{lot}/stock-mutations -> the lot's level at the warehouse.
#[utoipa::path(
    post, path = "/workspace/{workspace}/lots/{lot}/stock-mutations", tag = "lots",
    operation_id = "lots_stock_mutate", summary = "Put stock into or take it out of a lot",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("lot" = Uuid, Path, description = "Lot id"),
        ("Idempotency-Key" = Option<String>, Header, description = "1 to 255 visible characters. A retry with the same key and request replays the first answer and moves nothing again; another request under the key is a 422. Kept for a day."),
    ),
    request_body = StockMutationRequest,
    responses(
        (status = 200, body = Envelope<LotStockLevel>, description = "Stock moved; the lot's level at the warehouse"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or lot, or caller not a member"),
        (status = 409, body = MessageBody, description = "`lot_recalled`: stock can't go into a recalled lot (out stays allowed)"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, unknown warehouse, more out than the lot has free, or Idempotency-Key reused for another request"),
    ),
    security(("bearer" = []))
)]
pub async fn stock_mutate(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, id)): LotPath,
    Keyed { input: m, key }: Keyed<StockMutationRequest>,
) -> Result<Answer, AppError> {
    service::stock_mutation(&state, &access, &id, m, key).await
}
