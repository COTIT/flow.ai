//! Supply chain RUAHA → AFM: a revoked link, a live one and a pending link the other way; three
//! coffee batches bought from verified harvests, packed into 250 g bags and transferred. Two
//! transfers are received into published lots at AFM, one is still on the road.

use axum::http::Method;
use serde_json::json;
use uuid::Uuid;

use super::{Shop, farm::Ruaha};
use crate::{error::AppError, farm_demo::text};

/// Verified coffee harvests of Ilundo the seed leaves unbought.
const UNBOUGHT: usize = 4;

/// What the sales need: the AFM coffee product and its published passport.
pub struct Supplied {
    pub coffee_id: String,
    pub coffee_cents: i64,
    pub public_code: String,
}

pub async fn seed(
    shop: &Shop<'_>,
    r: &Ruaha,
    store: &str,
    depot: &str,
) -> Result<Supplied, AppError> {
    let api = shop.api;
    let ws_r = |tail: &str| format!("/workspace/{}{tail}", r.ws);
    let fields = json!(["village", "farmer_count", "harvest_window"]);

    // A first link, accepted then revoked; the live one replaces it.
    let mut link_id = String::new();
    for revoke in [true, false] {
        let link = api
            .post(
                &ws_r("/supply-links"),
                json!({ "to_workspace_id": shop.ws, "fields": fields }),
                &r.owner,
            )
            .await?;
        link_id = text(&link, "id")?;
        api.post(
            &shop.at(&format!("/supply-links/{link_id}/accept")),
            json!({}),
            &shop.owner,
        )
        .await?;
        if revoke {
            api.call(
                Method::DELETE,
                &ws_r(&format!("/supply-links/{link_id}")),
                None,
                Some(&r.owner),
            )
            .await?;
        }
    }
    // AFM offers RUAHA a link back; RUAHA hasn't answered.
    api.post(
        &shop.at("/supply-links"),
        json!({ "to_workspace_id": r.ws, "fields": ["village"] }),
        &shop.owner,
    )
    .await?;

    let raw = api
        .post(
            &ws_r("/products"),
            json!({ "name": "Coffee cherries (raw)", "price": "1", "unit": "kg" }),
            &r.owner,
        )
        .await?;
    let bags = api
        .post(
            &ws_r("/products"),
            json!({ "name": "Ruaha coffee 250 g", "price": "8" }),
            &r.owner,
        )
        .await?;
    for (name, price) in [("Maize grain, bulk", "0.40"), ("Beans, bulk", "1.10")] {
        api.post(
            &ws_r("/products"),
            json!({ "name": name, "price": price, "unit": "kg" }),
            &r.owner,
        )
        .await?;
    }
    let r_store = api
        .post(
            &ws_r("/warehouses"),
            json!({ "name": "Ilundo store", "default_warehouse": true, "is_store": true }),
            &r.owner,
        )
        .await?;
    let r_store = text(&r_store, "warehouse_id")?;

    let coffee_cents = 1450;
    let coffee = api
        .post(
            &shop.at("/products"),
            json!({ "name": "Ruaha coffee 250 g, whole bean", "price": super::dollars(coffee_cents),
                    "brand": "Ruaha Coffee Co-op", "ean13": super::ean13(999),
                    "description": "Arabica from smallholders in Ilundo, Kitowo and Lugoda, Tanzania. Every bag traces to its village lot." }),
            &shop.owner,
        )
        .await?;
    let coffee_id = text(&coffee, "product_id")?;

    let mut public_code = String::new();
    for (n, (village_id, code, harvests)) in r.coffee.iter().enumerate() {
        if harvests.is_empty() {
            continue;
        }
        let n = n + 1;
        // Ilundo leaves its last few unbought (at least one bought), so Buy harvest has
        // something to buy on staging; the other villages sell every harvest.
        let keep = match *code {
            "ILUNDO" => harvests.len().saturating_sub(1).min(UNBOUGHT),
            _ => 0,
        };
        let harvests = &harvests[..harvests.len() - keep];
        let lines: Vec<_> = harvests
            .iter()
            .map(|(h, kg)| json!({ "harvest_report_id": h, "kg": kg.to_string() }))
            .collect();
        let batch = api
            .post(
                &format!("/workspace/{}/farm/supply-batches", r.ws),
                json!({ "client_ref": Uuid::now_v7(), "village_id": village_id, "crop_id": r.coffee_crop,
                        "code": format!("CF-2026-{n:02}"), "raw_product_id": raw["product_id"],
                        "price_per_kg": 800_000, "currency_id": r.tzs, "lines": lines }),
                &r.ops,
            )
            .await?;
        let raw_lot = batch["lot"]["id"]
            .as_str()
            .ok_or_else(|| AppError::Internal("demo seed: no raw lot".into()))?
            .to_owned();
        // Buying stocked the raw lot at the default store (`r_store`) already.
        let packed = api
            .post(
                &ws_r(&format!("/lots/{raw_lot}/packings")),
                json!({ "product_id": bags["product_id"], "code": format!("CF-BAG-{n:02}"),
                        "units": "120", "raw_kg": "30.00", "warehouse_id": r_store }),
                &r.owner,
            )
            .await?;
        let bag_lot = text(&packed, "id")?;
        let transfer = api
            .post(
                &ws_r(&format!("/lots/{bag_lot}/transfers")),
                json!({ "link_id": link_id, "warehouse_id": r_store, "quantity": "120" }),
                &r.owner,
            )
            .await?;
        if n == 3 {
            continue; // still on the road: the office's Receive button
        }
        let received = api
            .post(
                &shop.at(&format!(
                    "/supply-transfers/{}/receive",
                    text(&transfer, "id")?
                )),
                json!({ "product_id": coffee_id, "code": format!("AFM-CF-{n:02}"),
                        "warehouse_id": if n == 1 { store } else { depot }, "quantity": "120" }),
                &shop.owner,
            )
            .await?;
        let lot = received["to_lot"]["id"]
            .as_str()
            .ok_or_else(|| AppError::Internal("demo seed: no received lot".into()))?
            .to_owned();
        let place = code
            .chars()
            .next()
            .map_or_else(String::new, |c| c.to_string() + &code[1..].to_lowercase());
        api.call(
            Method::PUT,
            &shop.at(&format!("/lots/{lot}/passport")),
            Some(json!({ "title": format!("Ruaha coffee, {place} village"),
                         "intro": format!("DEMO: grown by smallholders in {place}, packed at the village store."),
                         "origin": { "country": "TZ", "region": "Iringa", "place": place },
                         "story": [{ "kind": "facts", "items": [
                             { "label": "Altitude", "value": "1,600 m" },
                             { "label": "Process", "value": "Washed" }] }] })),
            Some(&shop.owner),
        )
        .await?;
        api.post(
            &shop.at(&format!("/lots/{lot}/passport/publish")),
            json!({}),
            &shop.owner,
        )
        .await?;
        if public_code.is_empty() {
            let lot = api
                .call(
                    Method::GET,
                    &shop.at(&format!("/lots/{lot}")),
                    None,
                    Some(&shop.owner),
                )
                .await?;
            public_code = text(&lot["data"], "public_code")?;
        }
    }
    Ok(Supplied {
        coffee_id,
        coffee_cents,
        public_code,
    })
}
