//! HTTP edge for /v1/workspace/{workspace}/erp/product-brands: `productbrand.*` permission.
// Laravel checked `.view` for one row; genesy uses the catalog's `get`.

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
    modules::product_brands::{
        dto::{
            BrandBody, BrandCreated, BrandResource, CreateBrandRequest, ListQuery,
            UpdateBrandRequest,
        },
        service,
    },
};

permissions! {
    BrandCreate => "productbrand.create",
    BrandList => "productbrand.list",
    BrandGet => "productbrand.get",
    BrandUpdate => "productbrand.update",
    BrandDelete => "productbrand.delete",
}

type BrandPath = Path<(String, String)>;

/// GET /product-brands -> `{data:[...], meta}`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/product-brands", tag = "product_brands",
    operation_id = "product_brands_list", summary = "List brands",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<BrandResource>, description = "One page of brands"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productbrand.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<BrandList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<BrandResource>>, AppError> {
    let pattern = search(q.q.as_deref())?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        pattern.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /product-brands/{brand}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/product-brands/{brand}", tag = "product_brands",
    operation_id = "product_brands_get", summary = "Get a brand",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("brand" = Uuid, Path, description = "Brand id")),
    responses(
        (status = 200, body = Envelope<BrandResource>, description = "The brand"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productbrand.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or brand, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<BrandGet>,
    Path((_, id)): BrandPath,
) -> Result<Data<BrandResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /product-brands -> 201 `{data:{product_brand_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/product-brands", tag = "product_brands",
    operation_id = "product_brands_create", summary = "Create a brand",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = BrandBody,
    responses(
        (status = 201, body = Envelope<BrandCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productbrand.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<BrandCreate>,
    ValidatedJson(input): ValidatedJson<CreateBrandRequest>,
) -> Result<(StatusCode, Data<BrandCreated>), AppError> {
    let product_brand_id = service::create(&state, access.workspace_id, input).await?;
    Ok((StatusCode::CREATED, Data(BrandCreated { product_brand_id })))
}

/// POST /product-brands/{brand} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/product-brands/{brand}", tag = "product_brands",
    operation_id = "product_brands_update", summary = "Update a brand (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("brand" = Uuid, Path, description = "Brand id")),
    request_body = BrandBody,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productbrand.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or brand, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<BrandUpdate>,
    Path((_, id)): BrandPath,
    ValidatedJson(input): ValidatedJson<UpdateBrandRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /product-brands/{brand} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/erp/product-brands/{brand}", tag = "product_brands",
    operation_id = "product_brands_delete", summary = "Delete a brand",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("brand" = Uuid, Path, description = "Brand id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productbrand.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or brand, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<BrandDelete>,
    Path((_, id)): BrandPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
