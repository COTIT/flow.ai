//! HTTP edge for /v1/workspace/{workspace}/products/{product}/purchase-units.

use axum::extract::{Path, State};

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    idempotency::{Answer, Keyed},
    modules::purchase_units::{
        dto::{CreatePurchaseUnitRequest, PurchaseUnitResource, UpdatePurchaseUnitRequest},
        service,
    },
};

permissions! {
    ProductView => "product.view",
    ProductUpdate => "product.update",
}

const KEY_REQUIRED: &str = "The Idempotency-Key header is required.";

/// GET /products/{product}/purchase-units -> `{data:[...]}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/{product}/purchase-units", tag = "purchase_units",
    operation_id = "purchase_units_list", summary = "List a product's purchase units",
    description = "Supplier units with their exact ratio to the product's unit, ordered by code (at most 50).",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("product" = Uuid, Path, description = "Product id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<PurchaseUnitResource>>, description = "All units"),
        (status = 403, body = MessageBody, description = "Missing `product.view`"),
        (status = 404, body = MessageBody, description = "Unknown product or workspace"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<ProductView>,
    Path((_, product)): Path<(String, String)>,
) -> Result<Data<Vec<PurchaseUnitResource>>, AppError> {
    Ok(Data(
        service::list(&state, access.workspace_id, &product).await?,
    ))
}

/// POST /products/{product}/purchase-units.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/{product}/purchase-units", tag = "purchase_units",
    operation_id = "purchase_units_create", summary = "Add a purchase unit",
    description = "Requires `Idempotency-Key`. The category must match the product unit's (kg, gram: mass; cm, m: length; m2: area; liter, m3: volume; pc, single: count). Box or unit-less products are refused. The ratio never changes afterwards. The same `client_ref` with the same content replays (200); other content is 409.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("product" = Uuid, Path, description = "Product id"),
    ),
    request_body = CreatePurchaseUnitRequest,
    responses(
        (status = 201, body = Envelope<PurchaseUnitResource>, description = "Created"),
        (status = 200, body = Envelope<PurchaseUnitResource>, description = "Replay of the same client_ref"),
        (status = 403, body = MessageBody, description = "Missing `product.update`"),
        (status = 404, body = MessageBody, description = "Unknown product or workspace"),
        (status = 409, body = MessageBody, description = "client_ref reused with other content"),
        (status = 422, body = UnprocessableBody, description = "Invalid fields, missing Idempotency-Key, or code `purchase_unit_base_unsupported`"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, product)): Path<(String, String)>,
    Keyed { input, key }: Keyed<CreatePurchaseUnitRequest>,
) -> Result<Answer, AppError> {
    let key = key.ok_or_else(|| AppError::Unprocessable(KEY_REQUIRED.into()))?;
    service::create(&state, &access, &product, input, key).await
}

/// PATCH /products/{product}/purchase-units/{unit}.
#[utoipa::path(
    patch, path = "/workspace/{workspace}/products/{product}/purchase-units/{unit}", tag = "purchase_units",
    operation_id = "purchase_units_update", summary = "Turn a purchase unit on or off",
    description = "Requires `Idempotency-Key`. Only `active` changes; code, category and ratio are fixed. `expected_version` must match the stored version (409 otherwise); a change bumps it by one.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("product" = Uuid, Path, description = "Product id"),
        ("unit" = Uuid, Path, description = "Purchase unit id"),
    ),
    request_body = UpdatePurchaseUnitRequest,
    responses(
        (status = 200, body = Envelope<PurchaseUnitResource>, description = "Current unit"),
        (status = 403, body = MessageBody, description = "Missing `product.update`"),
        (status = 404, body = MessageBody, description = "Unknown unit, product or workspace"),
        (status = 409, body = MessageBody, description = "Stale expected_version or client_ref reused with other content"),
        (status = 422, body = UnprocessableBody, description = "Invalid fields or missing Idempotency-Key"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, product, unit)): Path<(String, String, String)>,
    Keyed { input, key }: Keyed<UpdatePurchaseUnitRequest>,
) -> Result<Answer, AppError> {
    let key = key.ok_or_else(|| AppError::Unprocessable(KEY_REQUIRED.into()))?;
    service::update(&state, &access, &product, &unit, input, key).await
}
