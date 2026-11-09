//! HTTP edge for /v1/workspace/{workspace}/farm/supply-batches, dispatch options and quiet farmers.

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
    modules::farm_supply::{
        dto::{
            BatchResource, CreateBatchRequest, DispatchOptions, ListQuery, OptionsQuery,
            OptionsResource, QuietFarmer, QuietQuery, RawProductCreated, RawProductRequest,
        },
        service,
    },
};

/// GET /farm/supply-batches -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/supply-batches", tag = "farm_supply",
    operation_id = "farm_supply_list", summary = "List supply batches",
    description = "Staff: batches of their villages (ops and admin: all), newest first.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<BatchResource>, description = "One page"),
        (status = 403, body = MessageBody, description = "Caller is a farmer, not staff"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<Paginated<BatchResource>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (data, total) = service::list(&state, &access, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(data, total, page)))
}

/// POST /farm/supply-batches -> 201 created, 200 replayed.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/supply-batches", tag = "farm_supply",
    operation_id = "farm_supply_create", summary = "Buy harvests into a raw lot",
    description = "ops or admin who verified none of the harvests. Each line is a current, verified, actual harvest of the village and crop; kg over all batches never pass the harvest. Makes the raw lot (`lot.created`) and the frozen batch in one transaction. The same `client_ref` and body replay (200); another body or member under it is 409.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CreateBatchRequest,
    responses(
        (status = 200, body = Envelope<BatchResource>, description = "Replayed: this client_ref was used with this body"),
        (status = 201, body = Envelope<BatchResource>, description = "Created"),
        (status = 403, body = MessageBody, description = "Not ops or admin, or the caller verified one of the harvests"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 409, body = MessageBody, description = "client_ref used with another body or by another member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input, a harvest that can't be bought, over-commitment, or code taken"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<CreateBatchRequest>,
) -> Result<(StatusCode, Data<BatchResource>), AppError> {
    let (batch, created) = service::create(&state, &access, input).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(batch)))
}

/// GET /farm/supply-batches/{batch}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/supply-batches/{batch}", tag = "farm_supply",
    operation_id = "farm_supply_get", summary = "Get a supply batch",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("batch" = Uuid, Path, description = "Batch id"),
    ),
    responses(
        (status = 200, body = Envelope<BatchResource>, description = "The batch with its lines"),
        (status = 403, body = MessageBody, description = "Caller is a farmer, not staff"),
        (status = 404, body = MessageBody, description = "Unknown batch, or outside the caller's villages or workspace"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<BatchResource>, AppError> {
    Ok(Data(service::get(&state, &access, &id).await?))
}

/// GET /farm/supply-batches/options -> what a new batch can be made of.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/supply-batches/options", tag = "farm_supply",
    operation_id = "farm_supply_options", summary = "What a new batch can buy",
    description = "ops or admin. For one village and crop: the harvests a batch would accept as lines (current, verified, actual, kg left, a primary manager, not verified by the caller), the workspace's stock-tracked products with `suggested_for_crop` (a hint; create accepts any of them), and `can_create_product`.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), OptionsQuery),
    responses(
        (status = 200, body = Envelope<OptionsResource>, description = "Harvests, raw products, product-create right"),
        (status = 403, body = MessageBody, description = "Not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 422, body = ValidationErrorBody, description = "Missing or unknown village or crop"),
    ),
    security(("bearer" = []))
)]
pub async fn options(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<OptionsQuery>,
) -> Result<Data<OptionsResource>, AppError> {
    let (village, crop) = query.ids()?;
    Ok(Data(
        service::options(&state, &access, village, crop).await?,
    ))
}

/// POST /farm/supply-batches/raw-product -> 201 with the new product.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/supply-batches/raw-product", tag = "farm_supply",
    operation_id = "farm_supply_raw_product", summary = "Add a raw product for batches",
    description = "ops or admin, no `product.create` needed. Makes a physical, stock-tracked product in kg with net price 0, the workspace's default VAT group and status `disabled` (sold nowhere: the storefront and till catalogue list only active products), through the products service. Its id is a valid `raw_product_id`.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = RawProductRequest,
    responses(
        (status = 201, body = Envelope<RawProductCreated>, description = "Created"),
        (status = 403, body = MessageBody, description = "Not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 422, body = ValidationErrorBody, description = "Missing or too long name"),
    ),
    security(("bearer" = []))
)]
pub async fn raw_product(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(fields): ValidatedJson<RawProductRequest>,
) -> Result<(StatusCode, Data<RawProductCreated>), AppError> {
    let p = service::create_raw_product(&state, &access, fields).await?;
    Ok((StatusCode::CREATED, Data(p)))
}

/// GET /farm/dispatch/options -> what a dispatch may send, without office rights.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/dispatch/options", tag = "farm_supply",
    operation_id = "farm_supply_dispatch_options", summary = "What a dispatch may send",
    description = "ops or admin. The live supply links this workspace granted (with the shop's name), the raw lots of its supply batches, and the warehouses holding their free stock. The Field dispatch form reads this instead of the office's link and warehouse lists.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    responses(
        (status = 200, body = Envelope<DispatchOptions>, description = "Links, lots, warehouses"),
        (status = 403, body = MessageBody, description = "Not ops or admin"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn dispatch_options(
    State(state): State<AppState>,
    access: FarmAccess,
) -> Result<Data<DispatchOptions>, AppError> {
    Ok(Data(service::dispatch_options(&state, &access).await?))
}

/// GET /farm/quiet-farmers -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/quiet-farmers", tag = "farm_supply",
    operation_id = "farm_supply_quiet_farmers", summary = "Farmers who have not delivered",
    description = "Staff: registered persons in their villages (ops and admin: all) with no kg bought in a supply batch on or after Tanzania day `since` (default 30 days ago), or never. Never delivered first, then the oldest last delivery, with `last_delivery_at`. `village_id` narrows to one village. A delivery is a supply batch line; harvest figures don't count.",
    params(("workspace" = Uuid, Path, description = "Workspace id"), QuietQuery),
    responses(
        (status = 200, body = Paginated<QuietFarmer>, description = "One page"),
        (status = 403, body = MessageBody, description = "Caller is a farmer, not staff"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 422, body = ValidationErrorBody, description = "Bad `since`, malformed village id"),
    ),
    security(("bearer" = []))
)]
pub async fn quiet_farmers(
    State(state): State<AppState>,
    access: FarmAccess,
    Query(query): Query<QuietQuery>,
) -> Result<Json<Paginated<QuietFarmer>>, AppError> {
    let page = Page::parse(query.page.as_deref(), query.page_size.as_deref());
    let (since, village) = (query.since.as_deref(), query.village_id.as_deref());
    let (data, total) =
        service::quiet_farmers(&state, &access, since, village, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(data, total, page)))
}
