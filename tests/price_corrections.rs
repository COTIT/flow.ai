//! /v1/workspace/{ws}/products/price-corrections (Laravel ERP/PriceCorrection) and the product
//! prices they correct (Laravel ProductPriceTrait::buildPrice).

mod common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::{Value, json};
use sqlx::PgPool;

const PERMS: &[&str] = &[
    "pricecorrection.create",
    "pricecorrection.list",
    "pricecorrection.view",
    "pricecorrection.update",
    "pricecorrection.delete",
];

async fn bands(pool: PgPool) -> Api {
    Api::new(pool, "/products/price-corrections").await
}

fn band(from: &str, to: &str, round_on: Value, remove: Value) -> Value {
    json!({ "value_from": from, "value_to": to, "round_on": round_on, "remove_amount": remove })
}

async fn error(api: &Api, path: &str, body: Value, field: &str) -> String {
    let (status, res) = api.call(Method::POST, path, Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    res["errors"][field][0]
        .as_str()
        .unwrap_or_else(|| panic!("no {field} error: {res}"))
        .to_owned()
}

#[sqlx::test]
async fn crud_and_shape(pool: PgPool) {
    let api = bands(pool).await;
    let id = api
        .create(
            band("0", "100", json!("1"), json!(0.01)),
            "price_correction_id",
        )
        .await;
    let b = api.get(&id).await;
    assert_eq!(b["id"], id.as_str());
    // Laravel MoneyHelper::formatDecimals on uncast columns: 2 dp, trimmed, null "0.00".
    assert_eq!(
        (
            &b["value_from"],
            &b["value_to"],
            &b["round_on"],
            &b["remove_amount"]
        ),
        (&json!("0"), &json!("100"), &json!("1"), &json!("0.01"))
    );
    assert_eq!(b["meta"], Value::Null);
    assert!(b["created_at"].is_string() && b["last_updated_at"].is_string());

    // Patch: round_on null clears it; value_to untouched.
    let (status, res) = api
        .update(&id, json!({ "round_on": null, "remove_amount": "0.05" }))
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let b = api.get(&id).await;
    assert_eq!(
        (&b["round_on"], &b["remove_amount"], &b["value_to"]),
        (&json!("0.00"), &json!("0.05"), &json!("100"))
    );

    let list = api.list("").await;
    assert_eq!(list["data"].as_array().unwrap().len(), 1);
    assert_eq!(list["meta"]["total"], 1);
    api.delete(&id).await;
    assert_eq!(api.list("").await["data"], json!([]));
}

#[sqlx::test]
async fn rules(pool: PgPool) {
    let api = bands(pool).await;
    let cases = [
        (
            json!({ "value_to": "1" }),
            "value_from",
            "The value from field is required.",
        ),
        (
            json!({ "value_from": "1" }),
            "value_to",
            "The value to field is required.",
        ),
        (
            band("x", "1", Value::Null, Value::Null),
            "value_from",
            "The value from must be a number.",
        ),
        (
            band("0", "1", json!("abc"), Value::Null),
            "round_on",
            "The round on must be a number.",
        ),
        (
            band("0", "1", Value::Null, json!([])),
            "remove_amount",
            "The remove amount must be a number.",
        ),
        (
            band("0", "1.123456789", Value::Null, Value::Null),
            "value_to",
            "The value to must have 0-8 decimal places.",
        ),
    ];
    for (body, field, msg) in cases {
        assert_eq!(error(&api, "", body, field).await, msg, "{field}");
    }
    let id = api
        .create(
            band("0", "1", Value::Null, Value::Null),
            "price_correction_id",
        )
        .await;
    // A NOT NULL column sent as null is a 422 (Laravel answered 500).
    assert_eq!(
        error(
            &api,
            &format!("/{id}"),
            json!({ "value_from": null }),
            "value_from"
        )
        .await,
        "The value from field is required."
    );
}

#[sqlx::test]
async fn access(pool: PgPool) {
    let api = bands(pool).await;
    let id = api
        .create(
            band("0", "1", Value::Null, Value::Null),
            "price_correction_id",
        )
        .await;
    api.assert_member_forbidden(&id).await;
    api.assert_granted(
        PERMS,
        band("0", "1", Value::Null, Value::Null),
        "price_correction_id",
    )
    .await;
    api.assert_foreign_404(&id, json!({ "value_to": "2" }))
        .await;
    api.delete(&id).await;
    api.assert_bad_ids_404(&id, json!({ "value_to": "2" }))
        .await;
}

#[sqlx::test]
async fn workspace_holds_at_most_1000_bands(pool: PgPool) {
    let api = bands(pool.clone()).await;
    sqlx::query(
        "INSERT INTO price_corrections (workspace_id, value_from, value_to)
         SELECT $1::uuid, n, n FROM generate_series(1, 999) n",
    )
    .bind(&api.ws)
    .execute(&pool)
    .await
    .unwrap();
    api.create(
        band("0", "1", Value::Null, Value::Null),
        "price_correction_id",
    )
    .await;
    assert_eq!(
        error(
            &api,
            "",
            band("0", "1", Value::Null, Value::Null),
            "price_corrections"
        )
        .await,
        "The workspace can't hold more than 1000 price corrections."
    );
}

// --- product prices -----------------------------------------------------------------------

/// Workspace with a default 21 % VAT group; returns the bands API.
async fn world(pool: PgPool) -> Api {
    let api = bands(pool).await;
    api.with_base("/settings/vat-groups")
        .create(
            json!({ "name": "High", "vat_amount": "21", "default_group": true, "country": "NL" }),
            "vat_group_id",
        )
        .await;
    api
}

async fn product(api: &Api, extra: Value) -> String {
    let mut body = json!({ "name": "Cable", "price": "10", "gross_price": "10" });
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    api.with_base("/products").create(body, "product_id").await
}

const FIELDS: [&str; 8] = [
    "price",
    "price_incl",
    "original_price",
    "original_price_incl",
    "business_price",
    "business_price_incl",
    "price_without_correction",
    "price_without_correction_incl",
];

async fn prices(api: &Api, id: &str) -> Vec<String> {
    let p = api.with_base("/products").get(id).await;
    FIELDS
        .iter()
        .map(|f| p[f].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn want(corrected: (&str, &str), uncorrected: (&str, &str)) -> Vec<String> {
    let (c, ci) = corrected;
    let (u, ui) = uncorrected;
    [c, ci, c, ci, c, ci, u, ui].map(str::to_owned).to_vec()
}

#[sqlx::test]
async fn corrections_round_the_product_price(pool: PgPool) {
    let api = world(pool).await;
    let p = product(&api, json!({})).await;
    assert_eq!(
        prices(&api, &p).await,
        want(("10", "12.1"), ("10", "12.1")),
        "no band"
    );

    // 10 excl = 12.10 incl -> 12 on whole euros, minus 1 cent incl (Laravel oracle case
    // `whole_euro_minus_cent`).
    let id = api
        .create(
            band("0", "100", json!("1"), json!("0.01")),
            "price_correction_id",
        )
        .await;
    assert_eq!(
        prices(&api, &p).await,
        want(("9.9091", "11.99"), ("10", "12.1"))
    );

    // Changes apply at once (Laravel cached the bands forever per workspace).
    api.update(&id, json!({ "round_on": null })).await;
    assert_eq!(
        prices(&api, &p).await,
        want(("9.9917", "12.09"), ("10", "12.1"))
    );

    // The product can opt out.
    let out = product(&api, json!({ "exclude_price_corrections": true })).await;
    assert_eq!(
        prices(&api, &out).await,
        want(("10", "12.1"), ("10", "12.1"))
    );

    api.delete(&id).await;
    assert_eq!(prices(&api, &p).await, want(("10", "12.1"), ("10", "12.1")));
}

#[sqlx::test]
async fn band_pick_lowest_start_both_ends_included(pool: PgPool) {
    let api = world(pool).await;
    let p = product(&api, json!({})).await;
    // Above the price: no match.
    api.create(
        band("10.01", "100", json!("1"), json!("0.01")),
        "price_correction_id",
    )
    .await;
    assert_eq!(prices(&api, &p).await[0], "10");
    // Ends exactly at the price: matches.
    api.create(
        band("5", "10", json!("0.05"), Value::Null),
        "price_correction_id",
    )
    .await;
    assert_eq!(prices(&api, &p).await[0], "10", "12.10 is already on 5 ct");
    // Overlapping band with a lower start wins.
    api.create(
        band("0", "50", json!("1"), json!("0.01")),
        "price_correction_id",
    )
    .await;
    assert_eq!(prices(&api, &p).await[0], "9.9091");
}

#[sqlx::test]
async fn business_price_follows_double_discount_setting(pool: PgPool) {
    let api = world(pool).await;
    api.create(
        band("0", "100", json!("1"), json!("0.01")),
        "price_correction_id",
    )
    .await;
    // 10 % below gross.
    let p = product(&api, json!({ "price": "9", "gross_price": "10" })).await;
    // 9 excl = 10.89 incl -> 11 - 0.01 = 10.99 incl = 9.0826 excl.
    assert_eq!(
        prices(&api, &p).await,
        want(("9.0826", "10.99"), ("9", "10.89"))
    );
    let (status, res) = api
        .with_base("/settings")
        .call(
            Method::POST,
            "",
            Some(json!({ "product": { "disable_double_discount_for_business": true } })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    // Business prices start from gross; the consumer price keeps the sale price until special
    // offers (F3b) can make it switch.
    let got = prices(&api, &p).await;
    assert_eq!(&got[..4], &want(("9.0826", "10.99"), ("", ""))[..4]);
    assert_eq!(&got[4..], ["9.9091", "11.99", "10", "12.1"]);
}

#[sqlx::test]
async fn bands_load_once_per_page(pool: PgPool) {
    let api = world(pool).await;
    for i in 0..30 {
        api.create(
            band(
                &i.to_string(),
                &(i + 1).to_string(),
                json!("1"),
                Value::Null,
            ),
            "price_correction_id",
        )
        .await;
    }
    for _ in 0..50 {
        product(&api, json!({})).await;
    }
    let started = std::time::Instant::now();
    let list = api.with_base("/products").list("?page_size=50").await;
    assert_eq!(list["data"].as_array().unwrap().len(), 50);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}
