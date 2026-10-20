//! /v1/workspace/{ws}/searches?domain=X: the dropdown search every vimdesk-web select uses
//! (CurrenciesSelect, VatGroupsSelect, ShipmentMethodSelect, ...). One registry, each domain
//! answers with its own list shape.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn post(a: &Api, path: &str, body: Value) -> String {
    let uri = format!("/v1/workspace/{}{path}", a.ws);
    let (status, res) = send(&a.app, Method::POST, &uri, Some(body), Some(&a.owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{path}: {res}");
    res["data"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

/// Ada's `acme` with one "Zeta …" row in every searchable domain.
async fn seeded(pool: PgPool) -> Api {
    let a = Api::new(pool, "/searches").await;
    post(&a, "/currencies", json!({ "name": "Zeta Coin", "asset_type": "chf", "decimals": 2, "default_currency": false, "active": true })).await;
    let vat = post(
        &a,
        "/settings/vat-groups",
        json!({ "name": "Zeta VAT", "vat_amount": 9 }),
    )
    .await;
    post(
        &a,
        "/settings/sales-groups",
        json!({ "name": "Zeta Sales" }),
    )
    .await;
    post(&a, "/hourly-rates", json!({ "title": "Zeta Rate", "description": "d", "consumer_price": 1, "business_price": 1, "vat_group_id": vat })).await;
    post(&a, "/shipment-methods", json!({ "name": "Zeta Ship", "carrier": "DHL", "shipping_costs": 1, "min_weight": 0, "max_weight": 1, "can_be_selected": true })).await;
    post(&a, "/shipment-methods", json!({ "name": "Zeta Hidden", "carrier": "DHL", "shipping_costs": 1, "min_weight": 0, "max_weight": 1 })).await;
    post(
        &a,
        "/custom-status",
        json!({ "name": "Zeta Status", "domain": "deal" }),
    )
    .await;
    post(
        &a,
        "/custom-types",
        json!({ "name": "Zeta Type", "domain": "relation" }),
    )
    .await;
    a
}

fn names(res: &Value, key: &str) -> Vec<String> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r[key].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test]
async fn every_domain_answers_with_its_module_shape(pool: PgPool) {
    let a = seeded(pool).await;

    for (domain, key, name, own_key) in [
        ("currency", "name", "Zeta Coin", "default_currency"),
        ("vat_group", "name", "Zeta VAT", "vat_amount"),
        ("sales_group", "name", "Zeta Sales", "revenue_code"),
        ("hourly_rate", "title", "Zeta Rate", "relationships"),
        ("custom_status", "name", "Zeta Status", "archived"),
        ("custom_type", "name", "Zeta Type", "properties"),
    ] {
        let res = a.list(&format!("?domain={domain}&q=zeta")).await;
        assert_eq!(names(&res, key), [name], "{domain}");
        assert!(
            res["data"][0].get(own_key).is_some(),
            "{domain} misses {own_key}"
        );
        assert_eq!(res["meta"]["total"], 1, "{domain}");
    }
}

#[sqlx::test]
async fn shipment_methods_filter_on_can_be_selected(pool: PgPool) {
    let a = seeded(pool).await;

    let all = a.list("?domain=shipment_method&q=zeta").await;
    assert_eq!(names(&all, "name"), ["Zeta Ship", "Zeta Hidden"]);
    for flag in ["1", "true"] {
        let res = a
            .list(&format!("?domain=shipment_method&can_be_selected={flag}"))
            .await;
        assert_eq!(names(&res, "name"), ["Zeta Ship"], "{flag}");
    }
    let res = a
        .list("?domain=shipment_method&can_be_selected=0&q=zeta")
        .await;
    assert_eq!(names(&res, "name"), ["Zeta Hidden"]);
}

#[sqlx::test]
async fn star_or_blank_query_matches_everything(pool: PgPool) {
    let a = seeded(pool).await;

    let star = a.list("?domain=sales_group&q=*").await;
    let blank = a.list("?domain=sales_group&q=").await;

    assert_eq!(names(&star, "name"), ["Zeta Sales"]);
    assert_eq!(star["meta"], blank["meta"]);
}

#[sqlx::test]
async fn unknown_unported_or_missing_domain_is_404(pool: PgPool) {
    let a = seeded(pool).await;

    for query in [
        "?domain=nope",
        "?domain=product",
        "?domain=relation",
        "",
        "?q=zeta",
    ] {
        let (status, _) = a.call(Method::GET, query, None, &a.owner).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{query}");
    }
}

#[sqlx::test]
async fn needs_global_search_and_stays_in_the_workspace(pool: PgPool) {
    let a = seeded(pool).await;
    let bob = member_with(&a.app, &a.pool, &a.ws, "bob@example.com", &[]).await;
    let carl = member_with(
        &a.app,
        &a.pool,
        &a.ws,
        "carl@example.com",
        &["workspace.global-search"],
    )
    .await;

    let (status, res) = a.call(Method::GET, "?domain=currency", None, &bob).await;
    assert_eq!(
        (status, res["message"].clone()),
        (StatusCode::FORBIDDEN, json!("This action is unauthorized."))
    );
    let (status, res) = a
        .call(Method::GET, "?domain=vat_group&q=zeta", None, &carl)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&res, "name"), ["Zeta VAT"]);

    // Eve searches her own workspace: acme's rows never show up.
    let (eve, eve_ws) = signup(&a.app, "eve@example.com", "evil").await;
    for domain in [
        "currency",
        "vat_group",
        "sales_group",
        "hourly_rate",
        "shipment_method",
        "custom_status",
        "custom_type",
    ] {
        let uri = format!("/v1/workspace/{eve_ws}/searches?domain={domain}&q=zeta");
        let (status, res) = send(&a.app, Method::GET, &uri, None, Some(&eve)).await;
        assert_eq!(status, StatusCode::OK, "{domain}");
        assert_eq!(res["data"], json!([]), "{domain} leaks");
    }
    let (status, _) = a.call(Method::GET, "?domain=currency", None, &eve).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "outsider");
}
