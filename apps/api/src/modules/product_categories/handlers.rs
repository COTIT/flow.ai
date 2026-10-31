//! HTTP edge for /v1/workspace/{workspace}/erp/product-categories: `productcategory.*`
//! permission.
// Laravel checked `.view` for one row; genesy uses the catalog's `get`.

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated, flag, search},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::product_categories::{
        dto::{
            BAD_PARENT, CategoryBody, CategoryCreated, CategoryResource, CreateCategoryRequest,
            ListQuery, UpdateCategoryRequest,
        },
        repo::ListFilter,
        service,
    },
};

permissions! {
    CategoryCreate => "productcategory.create",
    CategoryList => "productcategory.list",
    CategoryGet => "productcategory.get",
    CategoryUpdate => "productcategory.update",
    CategoryDelete => "productcategory.delete",
}

type CategoryPath = Path<(String, String)>;

/// GET /product-categories -> `{data:[...], meta}`; `parent_id` or `root_only=1` filter.
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/product-categories", tag = "product_categories",
    operation_id = "product_categories_list", summary = "List categories",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<CategoryResource>, description = "One page of categories"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productcategory.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Search text too long or malformed `parent_id`"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<CategoryList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<CategoryResource>>, AppError> {
    let pattern = search(q.q.as_deref())?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let parent_id = match q
        .parent_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(raw) => Some(Uuid::parse_str(raw).map_err(|_| {
            AppError::Validation(BTreeMap::from([(
                "parent_id".to_owned(),
                vec![BAD_PARENT.to_owned()],
            )]))
        })?),
    };
    let filter = ListFilter {
        pattern: pattern.as_deref(),
        parent_id,
        root_only: q.root_only.as_deref().is_some_and(flag),
        limit: page.size,
        offset: page.offset(),
    };
    let (rows, total) = service::list(&state, access.workspace_id, &filter).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /product-categories/{category}, with `parent` and `children`.
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/product-categories/{category}", tag = "product_categories",
    operation_id = "product_categories_get", summary = "Get a category",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("category" = Uuid, Path, description = "Category id")),
    responses(
        (status = 200, body = Envelope<CategoryResource>, description = "The category"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productcategory.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or category, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<CategoryGet>,
    Path((_, id)): CategoryPath,
) -> Result<Data<CategoryResource>, AppError> {
    let (c, parent, children) = service::get(&state, access.workspace_id, &id).await?;
    Ok(Data(CategoryResource::with_family(c, parent, children)))
}

/// POST /product-categories -> 201 `{data:{product_category_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/product-categories", tag = "product_categories",
    operation_id = "product_categories_create", summary = "Create a category",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = CategoryBody,
    responses(
        (status = 201, body = Envelope<CategoryCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productcategory.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<CategoryCreate>,
    ValidatedJson(input): ValidatedJson<CreateCategoryRequest>,
) -> Result<(StatusCode, Data<CategoryCreated>), AppError> {
    let product_category_id = service::create(&state, access.workspace_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Data(CategoryCreated {
            product_category_id,
        }),
    ))
}

/// POST /product-categories/{category} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/product-categories/{category}", tag = "product_categories",
    operation_id = "product_categories_update", summary = "Update a category (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("category" = Uuid, Path, description = "Category id")),
    request_body = CategoryBody,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productcategory.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or category, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<CategoryUpdate>,
    Path((_, id)): CategoryPath,
    ValidatedJson(input): ValidatedJson<UpdateCategoryRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /product-categories/{category} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/erp/product-categories/{category}", tag = "product_categories",
    operation_id = "product_categories_delete", summary = "Delete a category",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("category" = Uuid, Path, description = "Category id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `productcategory.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or category, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<CategoryDelete>,
    Path((_, id)): CategoryPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
