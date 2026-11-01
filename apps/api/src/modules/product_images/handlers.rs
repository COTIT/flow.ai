//! HTTP edge for /v1/workspace/{workspace}/products/{product}/images: members read,
//! `product.update` replaces.

use axum::extract::{Path, State};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
        workspace::{Can, WorkspaceAccess},
    },
    modules::{
        product_images::{
            dto::{ProductImageResource, ReplaceImagesRequest},
            service,
        },
        products::handlers::ProductUpdate,
    },
};

type ProductPath = Path<(String, String)>;

/// GET /products/{product}/images -> `{data: [image]}` in order.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/{product}/images", tag = "products",
    operation_id = "products_images_list", summary = "A product's images, in order",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("product" = Uuid, Path, description = "Product id"),
    ),
    responses(
        (status = 200, body = Envelope<Vec<ProductImageResource>>, description = "The images, with signed links"),
        (status = 403, body = MessageBody, description = "Inactive membership"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: WorkspaceAccess,
    Path((_, product)): ProductPath,
) -> Result<Data<Vec<ProductImageResource>>, AppError> {
    Ok(Data(
        service::list(&state, access.workspace_id, &product).await?,
    ))
}

/// PUT /products/{product}/images -> the new list.
#[utoipa::path(
    put, path = "/workspace/{workspace}/products/{product}/images", tag = "products",
    operation_id = "products_images_replace", summary = "Replace a product's images",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("product" = Uuid, Path, description = "Product id"),
    ),
    request_body = ReplaceImagesRequest,
    responses(
        (status = 200, body = Envelope<Vec<ProductImageResource>>, description = "Saved; the images in order"),
        (status = 403, body = MessageBody, description = "Inactive membership, missing `product.update`, or files sent without `file.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "More than 20, a file twice, not this workspace's image, or another product's variant"),
    ),
    security(("bearer" = []))
)]
pub async fn replace(
    State(state): State<AppState>,
    access: Can<ProductUpdate>,
    Path((_, product)): ProductPath,
    ValidatedJson(images): ValidatedJson<ReplaceImagesRequest>,
) -> Result<Data<Vec<ProductImageResource>>, AppError> {
    Ok(Data(
        service::replace(&state, &access, &product, images).await?,
    ))
}
