//! HTTP edge for /v1/workspace/{workspace}/products/discounts (`discount.*`).

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
    modules::discounts::{
        dto::{
            CreateDiscountRequest, DiscountCreated, DiscountRequest, DiscountResource, ListQuery,
            UpdateDiscountRequest,
        },
        service,
    },
};

permissions! {
    DiscountList => "discount.list",
    DiscountView => "discount.view",
    DiscountCreate => "discount.create",
    DiscountUpdate => "discount.update",
    DiscountDelete => "discount.delete",
}

type DiscountPath = Path<(String, String)>;

/// GET /discounts (`?discount_group_id=`).
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/discounts", tag = "discounts",
    operation_id = "discounts_list", summary = "List discounts",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<DiscountResource>, description = "One page of discounts"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discount.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Malformed `discount_group_id`"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<DiscountList>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<DiscountResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        q.discount_group_id.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /discounts/{id}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/products/discounts/{discount}", tag = "discounts",
    operation_id = "discounts_get", summary = "Get a discount",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("discount" = Uuid, Path, description = "Discount id")),
    responses(
        (status = 200, body = Envelope<DiscountResource>, description = "The discount"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discount.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or discount, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<DiscountView>,
    Path((_, id)): DiscountPath,
) -> Result<Data<DiscountResource>, AppError> {
    Ok(Data(
        service::get(&state, access.workspace_id, &id).await?.into(),
    ))
}

/// POST /discounts -> 201 `{data:{discount_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/discounts", tag = "discounts",
    operation_id = "discounts_create", summary = "Create a discount",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = DiscountRequest,
    responses(
        (status = 201, body = Envelope<DiscountCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discount.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<DiscountCreate>,
    ValidatedJson(input): ValidatedJson<CreateDiscountRequest>,
) -> Result<(StatusCode, Data<DiscountCreated>), AppError> {
    let discount_id = service::create(&state, access.workspace_id, input).await?;
    Ok((StatusCode::CREATED, Data(DiscountCreated { discount_id })))
}

/// POST /discounts/{id}.
#[utoipa::path(
    post, path = "/workspace/{workspace}/products/discounts/{discount}", tag = "discounts",
    operation_id = "discounts_update", summary = "Update a discount (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("discount" = Uuid, Path, description = "Discount id")),
    request_body = DiscountRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discount.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or discount, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<DiscountUpdate>,
    Path((_, id)): DiscountPath,
    ValidatedJson(input): ValidatedJson<UpdateDiscountRequest>,
) -> Result<Message, AppError> {
    service::update(&state, access.workspace_id, &id, input).await?;
    Ok(Message("ok"))
}

/// DELETE /discounts/{id}.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/products/discounts/{discount}", tag = "discounts",
    operation_id = "discounts_delete", summary = "Delete a discount",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("discount" = Uuid, Path, description = "Discount id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `discount.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or discount, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<DiscountDelete>,
    Path((_, id)): DiscountPath,
) -> Result<Message, AppError> {
    service::delete(&state, access.workspace_id, &id).await?;
    Ok(Message("ok"))
}
