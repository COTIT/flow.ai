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
    error::AppError,
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Message},
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
pub async fn delete(
    State(state): State<AppState>,
    access: Can<OfferDelete>,
    Path(p): Params,
) -> Result<Message, AppError> {
    let (product, id) = ids(&p);
    service::delete(&state, access.workspace_id, product, id).await?;
    Ok(Message("ok"))
}
