//! HTTP edge for /v1/workspace/{workspace}/erp/special-offers and
//! /erp/products/{product}/special-offers (`specialoffer.*`).

use std::collections::HashMap;

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
    modules::special_offers::{
        dto::{ListQuery, OfferCreated, OfferRequest, OfferResource},
        service,
    },
};

permissions! {
    OfferList => "specialoffer.list",
    OfferView => "specialoffer.view",
    OfferCreate => "specialoffer.create",
    OfferUpdate => "specialoffer.update",
    OfferDelete => "specialoffer.delete",
}

/// Path params of both route families: `product` only on the product routes.
type Params = Path<HashMap<String, String>>;

fn ids(p: &HashMap<String, String>) -> (Option<&str>, &str) {
    (
        p.get("product").map(String::as_str),
        p.get("offer").map_or("", String::as_str),
    )
}

/// GET /special-offers (`?discount_rule_type=`, `legacy` = without one).
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/products/{product}/special-offers", tag = "special_offers",
    operation_id = "special_offers_list", summary = "List special offers of a product",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), ListQuery),
    responses(
        (status = 200, body = Paginated<OfferResource>, description = "One page of special offers of a product"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    access: Can<OfferList>,
    Path(p): Params,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<OfferResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = service::list(
        &state,
        access.workspace_id,
        ids(&p).0,
        q.discount_rule_type.as_deref(),
        page.size,
        page.offset(),
    )
    .await?;
    let data = rows.iter().map(Into::into).collect();
    Ok(Json(Paginated::new(data, total, page)))
}

/// GET /special-offers/{id}.
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/products/{product}/special-offers/{offer}", tag = "special_offers",
    operation_id = "special_offers_get", summary = "Get a special offer",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), ("offer" = Uuid, Path, description = "Special offer id")),
    responses(
        (status = 200, body = Envelope<OfferResource>, description = "The special offer"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, product or special offer, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    access: Can<OfferView>,
    Path(p): Params,
) -> Result<Data<OfferResource>, AppError> {
    let (product, id) = ids(&p);
    let row = service::get(&state, access.workspace_id, product, id).await?;
    Ok(Data((&row).into()))
}

/// POST /special-offers -> 201 `{data:{special_offer_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/products/{product}/special-offers", tag = "special_offers",
    operation_id = "special_offers_create", summary = "Create a special offer",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id")),
    request_body = OfferRequest,
    responses(
        (status = 201, body = Envelope<OfferCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or product, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<OfferCreate>,
    Path(p): Params,
    ValidatedJson(patch): ValidatedJson<OfferRequest>,
) -> Result<(StatusCode, Data<OfferCreated>), AppError> {
    let special_offer_id = service::create(&state, access.workspace_id, ids(&p).0, patch).await?;
    Ok((StatusCode::CREATED, Data(OfferCreated { special_offer_id })))
}

/// POST /special-offers/{id}.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/products/{product}/special-offers/{offer}", tag = "special_offers",
    operation_id = "special_offers_update", summary = "Update a special offer (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), ("offer" = Uuid, Path, description = "Special offer id")),
    request_body = OfferRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, product or special offer, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<OfferUpdate>,
    Path(p): Params,
    ValidatedJson(patch): ValidatedJson<OfferRequest>,
) -> Result<Message, AppError> {
    let (product, id) = ids(&p);
    service::update(&state, access.workspace_id, product, id, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /special-offers/{id}.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/erp/products/{product}/special-offers/{offer}", tag = "special_offers",
    operation_id = "special_offers_delete", summary = "Delete a special offer",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("product" = Uuid, Path, description = "Product id"), ("offer" = Uuid, Path, description = "Special offer id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, product or special offer, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<OfferDelete>,
    Path(p): Params,
) -> Result<Message, AppError> {
    let (product, id) = ids(&p);
    service::delete(&state, access.workspace_id, product, id).await?;
    Ok(Message("ok"))
}

// The same handlers on /erp/special-offers: one fn per path, as the spec needs.

/// GET /special-offers: same as the product route, over every offer.
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/special-offers", tag = "special_offers",
    operation_id = "special_offers_list_all", summary = "List every special offer",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<OfferResource>, description = "One page of special offers"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.list`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
// One fn per documented path: utoipa ties each fn to a single path.
pub async fn list_all(
    state: State<AppState>,
    access: Can<OfferList>,
    p: Params,
    q: Query<ListQuery>,
) -> Result<Json<Paginated<OfferResource>>, AppError> {
    list(state, access, p, q).await
}

/// GET /special-offers/{offer}: same as the product route, over every offer.
#[utoipa::path(
    get, path = "/workspace/{workspace}/erp/special-offers/{offer}", tag = "special_offers",
    operation_id = "special_offers_get_all", summary = "Get a special offer",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("offer" = Uuid, Path, description = "Special offer id")),
    responses(
        (status = 200, body = Envelope<OfferResource>, description = "The special offer"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.view`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or special offer, or caller not a member"),
    ),
    security(("bearer" = []))
)]
// One fn per documented path: utoipa ties each fn to a single path.
pub async fn get_all(
    state: State<AppState>,
    access: Can<OfferView>,
    p: Params,
) -> Result<Data<OfferResource>, AppError> {
    get(state, access, p).await
}

/// POST /special-offers: same as the product route, over every offer.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/special-offers", tag = "special_offers",
    operation_id = "special_offers_create_all", summary = "Create a special offer (rule or product)",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = OfferRequest,
    responses(
        (status = 201, body = Envelope<OfferCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
// One fn per documented path: utoipa ties each fn to a single path.
pub async fn create_all(
    state: State<AppState>,
    access: Can<OfferCreate>,
    p: Params,
    body: ValidatedJson<OfferRequest>,
) -> Result<(StatusCode, Data<OfferCreated>), AppError> {
    create(state, access, p, body).await
}

/// POST /special-offers/{offer}: same as the product route, over every offer.
#[utoipa::path(
    post, path = "/workspace/{workspace}/erp/special-offers/{offer}", tag = "special_offers",
    operation_id = "special_offers_update_all", summary = "Update a special offer (send only changed fields)",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("offer" = Uuid, Path, description = "Special offer id")),
    request_body = OfferRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or special offer, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
// One fn per documented path: utoipa ties each fn to a single path.
pub async fn update_all(
    state: State<AppState>,
    access: Can<OfferUpdate>,
    p: Params,
    body: ValidatedJson<OfferRequest>,
) -> Result<Message, AppError> {
    update(state, access, p, body).await
}

/// DELETE /special-offers/{offer}: same as the product route, over every offer.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/erp/special-offers/{offer}", tag = "special_offers",
    operation_id = "special_offers_delete_all", summary = "Delete a special offer",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("offer" = Uuid, Path, description = "Special offer id")),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `specialoffer.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or special offer, or caller not a member"),
    ),
    security(("bearer" = []))
)]
// One fn per documented path: utoipa ties each fn to a single path.
pub async fn delete_all(
    state: State<AppState>,
    access: Can<OfferDelete>,
    p: Params,
) -> Result<Message, AppError> {
    delete(state, access, p).await
}
