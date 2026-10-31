//! HTTP edge for /v1/workspace/{workspace}/products/{product}/variants. Reads take
//! `product.view`, writes the product's create/update/delete names.
// Laravel: none for the list, an unassignable check for one.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, MessageBody},
        workspace::{Can, permissions},
    },
    modules::{
        product_variants::{
            dto::{
                GenerateBody, GenerateDoc, VariantBody, VariantCreated, VariantDoc, VariantList,
                VariantRequestDoc,
            },
            service,
        },
        products::service::Actor,
    },
};

permissions! {
    VariantView => "product.view",
    VariantCreate => "product.create",
    VariantUpdate => "product.update",
    VariantDelete => "product.delete",
}

type ProductPath = Path<(String, String)>;
type VariantPath = Path<(String, String, String)>;

fn actor(user_id: uuid::Uuid) -> Actor {
    Actor {
        user_id,
        flow: "manual",
    }
}

/// GET /variants -> `{data:[...]}` (not paginated).
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/{product}/variants", tag = "product_variants",
    operation_id = "product_variants_list", summary = "List a product's variants",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id")),
    responses(
        (status = 200, body = VariantList, description = "Every variant; not paged"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<VariantView>,
    Path((_, p)): ProductPath,
) -> Result<Json<Value>, AppError> {
    let data = service::list(&state, access.workspace_id, &p).await?;
    Ok(Json(json!({ "data": data })))
}

/// GET /variants/{variant}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/{product}/variants/{variant}", tag = "product_variants",
    operation_id = "product_variants_get", summary = "Get a variant",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), ("variant" = Uuid, Path, description = "Variant id")),
    responses(
        (status = 200, body = Envelope<VariantDoc>, description = "The variant"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, product or variant, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<VariantView>,
    Path((_, p, id)): VariantPath,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &p, &id).await?,
    ))
}

/// POST /variants -> 201 `{data:{variant_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/{product}/variants", tag = "product_variants",
    operation_id = "product_variants_create", summary = "Create a variant",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id")),
    request_body = VariantRequestDoc,
    responses(
        (status = 201, body = Envelope<VariantCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<VariantCreate>,
    Path((_, p)): ProductPath,
    ValidatedJson(f): ValidatedJson<VariantBody>,
) -> Result<(StatusCode, Data<Value>), AppError> {
    let id = service::create(&state, access.workspace_id, &p, f, actor(access.user_id)).await?;
    Ok((StatusCode::CREATED, Data(json!({ "variant_id": id }))))
}

/// PUT /variants/{variant} -> `{data: variant}`.
#[utoipa::path(
    put, path = "/workspace/{workspace}/products/{product}/variants/{variant}", tag = "product_variants",
    operation_id = "product_variants_update", summary = "Update a variant (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), ("variant" = Uuid, Path, description = "Variant id")),
    request_body = VariantRequestDoc,
    responses(
        (status = 200, body = Envelope<VariantDoc>, description = "The updated variant"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, product or variant, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<VariantUpdate>,
    Path((_, p, id)): VariantPath,
    ValidatedJson(f): ValidatedJson<VariantBody>,
) -> Result<Data<Value>, AppError> {
    Ok(Data(
        service::update(
            &state,
            access.workspace_id,
            &p,
            &id,
            f,
            actor(access.user_id),
        )
        .await?,
    ))
}

/// DELETE /variants/{variant} -> 204.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/products/{product}/variants/{variant}", tag = "product_variants",
    operation_id = "product_variants_delete", summary = "Delete a variant",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), ("variant" = Uuid, Path, description = "Variant id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, product or variant, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "The default variant can't be deleted"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<VariantDelete>,
    Path((_, p, id)): VariantPath,
) -> Result<StatusCode, AppError> {
    service::delete(&state, access.workspace_id, &p, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /variants/generate -> 201 `{data:[variants]}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/{product}/variants/generate", tag = "product_variants",
    operation_id = "product_variants_generate", summary = "Generate variants from option types",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id")),
    request_body = GenerateDoc,
    responses(
        (status = 201, body = Envelope<Vec<VariantDoc>>, description = "The created variants"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `product.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn generate(
    State(state): State<AppState>,
    access: Can<VariantCreate>,
    Path((_, p)): ProductPath,
    ValidatedJson(g): ValidatedJson<GenerateBody>,
) -> Result<(StatusCode, Data<Vec<Value>>), AppError> {
    let made = service::generate(&state, access.workspace_id, &p, g, actor(access.user_id)).await?;
    Ok((StatusCode::CREATED, Data(made)))
}
