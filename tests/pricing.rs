//! POST /v1/workspace/{ws}/calculate-items and /calculate-marge (live totals for the item
//! editor in vimdesk-web and the POS).

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn api(pool: PgPool) -> Api {
    Api::new(pool, "/calculate-items").await
}

async fn calc(api: &Api, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, "", Some(body), &api.owner).await
}

async fn ok(api: &Api, body: Value) -> Value {
    let (status, res) = calc(api, body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

/// First message for `field` in a 422 body.
async fn error(api: &Api, body: Value, field: &str) -> String {
    let (status, res) = calc(api, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    res["errors"][field][0]
        .as_str()
        .unwrap_or_else(|| panic!("no error for {field}: {res}"))
        .to_owned()
}

fn item(q: Value, p: Value, vat: Value) -> Value {
    json!({ "id": 1, "quantity": q, "price": p, "vat": vat })
}

/// A product named `name` with wholesale price `wholesale` (its purchase price).
async fn product(api: &Api, name: &str, wholesale: &str) -> String {
    api.with_base("/products")
        .create(
            json!({ "name": name, "price": "10", "wholesale_price": wholesale }),
            "product_id",
        )
        .await
}

// --- happy ---------------------------------------------------------------------------------

#[sqlx::test]
async fn three_at_9_99_with_21_percent(pool: PgPool) {
    let api = api(pool).await;
    let d = ok(
        &api,
        json!({ "items": [item(json!(3), json!(9.99), json!(21))] }),
    )
    .await;
    assert_eq!(
        d,
        json!({
            "sum_total": 29.97,
            "total_discount": 0,
            "sub_total": 29.97,
            "total_vats": { "21": { "percentage": 21, "total_amount": 6.29 } },
            "total_vat": 6.29,
            "total": 36.26,
            "total_excl": 29.97,
            "total_excl_materials": 29.97,
            "total_excl_hours": 0,
            "total_excl_hours_users": [],
            "discounts": [],
        })
    );
}

/// Laravel answers 32.64 here; its own lines say 26.97 + 5.66 (user decision: totals add up).
#[sqlx::test]
async fn discounted_line_adds_up_and_is_echoed(pool: PgPool) {
    let api = api(pool).await;
    let d = ok(
        &api,
        json!({ "items": [{ "id": "row-1", "title": "Cable", "quantity": 3, "price": 9.99,
                            "vat": 21, "productDiscount": 10 }] }),
    )
    .await;
    assert_eq!(d["total_discount"], json!(3));
    assert_eq!(d["total_excl"], json!(26.97));
    assert_eq!(d["total_vat"], json!(5.66));
    assert_eq!(d["total"], json!(32.63));
    assert_eq!(
        d["discounts"],
        json!([{
            "id": "row-1", "title": "Cable", "quantity": 3, "price": 9.99, "vat": 21,
            "description": "", "product_id": null, "total": 26.97, "subtotal": 29.97,
            "productDiscount": "10", "purchase_price": null, "client_default_discount": "0.00",
            "multi_set_item_id": null,
        }])
    );
}

#[sqlx::test]
async fn vat_grouped_per_rate(pool: PgPool) {
    let api = api(pool).await;
    let d = ok(
        &api,
        json!({ "items": [
            item(json!(2), json!(12.5), json!(21)),
            item(json!(1), json!(3.99), json!(9)),
            item(json!(4), json!(1.25), json!(0)),
        ] }),
    )
    .await;
    assert_eq!(
        d["total_vats"],
        json!({
            "21": { "percentage": 21, "total_amount": 5.25 },
            "9": { "percentage": 9, "total_amount": 0.36 },
            "0": { "percentage": 0, "total_amount": 0 },
        })
    );
    assert_eq!(d["total"], json!(39.6));
}

#[sqlx::test]
async fn product_line_takes_name_and_purchase_price(pool: PgPool) {
    let api = api(pool).await;
    let drill = product(&api, "Drill", "6").await;
    let d = ok(
        &api,
        json!({ "items": [{ "id": 7, "product_id": drill, "quantity": 2, "price": 10,
                            "vat": 21, "productDiscount": 10 }] }),
    )
    .await;
    assert_eq!(d["discounts"][0]["description"], "Drill");
    assert_eq!(d["discounts"][0]["purchase_price"], json!(6));
    assert_eq!(d["total"], json!(21.78));
}

#[sqlx::test]
async fn explicit_currency_is_used(pool: PgPool) {
    let api = api(pool).await;
    let eur = api.with_base("/currencies").list("").await["data"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let d = ok(
        &api,
        json!({ "currency_id": eur, "items": [item(json!(1), json!(5), json!(21))] }),
    )
    .await;
    assert_eq!(d["total"], json!(6.05));
}

// --- errors --------------------------------------------------------------------------------

#[sqlx::test]
async fn needs_token_and_membership(pool: PgPool) {
    let api = api(pool).await;
    let body = json!({ "items": [item(json!(1), json!(1), json!(21))] });
    let uri = format!("/v1/workspace/{}/calculate-items", api.ws);
    let (status, _) = send(&api.app, Method::POST, &uri, Some(body.clone()), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (mallory, _) = signup(&api.app, "mallory@example.com", "mal").await;
    let (status, _) = send(&api.app, Method::POST, &uri, Some(body), Some(&mallory)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn any_active_member_may_calculate(pool: PgPool) {
    let api = api(pool.clone()).await;
    let carl = common::member_with(&api.app, &pool, &api.ws, "carl@example.com", &[]).await;
    let (status, res) = api
        .call(
            Method::POST,
            "",
            Some(json!({ "items": [item(json!(1), json!(1), json!(21))] })),
            &carl,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn items_must_be_a_non_empty_list(pool: PgPool) {
    let api = api(pool).await;
    let required = "The items field is required.";
    assert_eq!(error(&api, json!({}), "items").await, required);
    assert_eq!(error(&api, json!({ "items": [] }), "items").await, required);
    assert_eq!(
        error(&api, json!({ "items": null }), "items").await,
        required
    );
    assert_eq!(
        error(&api, json!({ "items": "x" }), "items").await,
        "The items must be an array."
    );
    assert_eq!(
        error(&api, json!({ "items": [5] }), "items.0").await,
        "The items.0 must be an array."
    );
}

#[sqlx::test]
async fn item_fields_are_validated(pool: PgPool) {
    let api = api(pool).await;
    let one = |v: Value| json!({ "items": [item(json!(1), json!(1), json!(21)), v] });
    let cases = [
        (
            json!({ "price": "abc" }),
            "items.1.price",
            "The items.1.price must be a number.",
        ),
        (
            json!({ "quantity": [] }),
            "items.1.quantity",
            "The items.1.quantity must be a number.",
        ),
        (
            json!({ "vat": -1 }),
            "items.1.vat",
            "The items.1.vat must be at least 0.",
        ),
        (
            json!({ "productDiscount": 101 }),
            "items.1.productDiscount",
            "The items.1.productDiscount must not be greater than 100.",
        ),
        (
            json!({ "productDiscount": -1 }),
            "items.1.productDiscount",
            "The items.1.productDiscount must be at least 0.",
        ),
        (
            json!({ "title": "x".repeat(501) }),
            "items.1.title",
            "The items.1.title must not be greater than 500 characters.",
        ),
        (
            json!({ "title": 5 }),
            "items.1.title",
            "The items.1.title must be a string.",
        ),
        (
            json!({ "description": "x".repeat(5001) }),
            "items.1.description",
            "The items.1.description must not be greater than 5000 characters.",
        ),
        (
            json!({ "purchase_price": "x" }),
            "items.1.purchase_price",
            "The items.1.purchase price must be a number.",
        ),
    ];
    for (bad, field, msg) in cases {
        assert_eq!(error(&api, one(bad), field).await, msg, "{field}");
    }
}

#[sqlx::test]
async fn unknown_references_are_422(pool: PgPool) {
    let api = api(pool).await;
    let line = item(json!(1), json!(1), json!(21));
    let missing = "0199f9a0-0000-7000-8000-000000000000";
    for currency in [json!(missing), json!("not-a-uuid")] {
        assert_eq!(
            error(
                &api,
                json!({ "currency_id": currency, "items": [line.clone()] }),
                "currency_id"
            )
            .await,
            "This currency id does not exist"
        );
    }
    assert_eq!(
        error(
            &api,
            json!({ "relation_id": missing, "items": [line.clone()] }),
            "relation_id"
        )
        .await,
        "The relation id is an invalid id for the relation domain."
    );
    let with_product = json!({ "items": [{ "product_id": missing, "quantity": 1, "price": 1 }] });
    assert_eq!(
        error(&api, with_product, "items.0.product_id").await,
        "The selected items.0.product id is invalid."
    );
}

#[sqlx::test]
async fn another_workspaces_rows_are_unknown(pool: PgPool) {
    let api = api(pool).await;
    let (mallory, mal_ws) = signup(&api.app, "mallory@example.com", "mal").await;
    let (_, res) = send(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{mal_ws}/products"),
        Some(json!({ "name": "Secret", "price": "1" })),
        Some(&mallory),
    )
    .await;
    let their_product = res["data"]["product_id"].as_str().unwrap().to_owned();
    let uri = format!("/v1/workspace/{mal_ws}/currencies");
    let (_, res) = send(&api.app, Method::GET, &uri, None, Some(&mallory)).await;
    let their_currency = res["data"][0]["id"].as_str().unwrap().to_owned();
    let line = json!({ "product_id": their_product, "quantity": 1, "price": 1 });
    assert_eq!(
        error(&api, json!({ "items": [line] }), "items.0.product_id").await,
        "The selected items.0.product id is invalid."
    );
    assert_eq!(
        error(
            &api,
            json!({ "currency_id": their_currency, "items": [item(json!(1), json!(1), json!(0))] }),
            "currency_id"
        )
        .await,
        "This currency id does not exist"
    );
}

#[sqlx::test]
async fn no_currency_at_all_is_422(pool: PgPool) {
    let api = api(pool).await;
    sqlx::query("UPDATE currencies SET deleted_at = now()")
        .execute(&api.pool)
        .await
        .unwrap();
    assert_eq!(
        error(
            &api,
            json!({ "items": [item(json!(1), json!(1), json!(21))] }),
            "currency_id"
        )
        .await,
        "No default currency has been found. Please create one"
    );
}

// --- edge ----------------------------------------------------------------------------------

#[sqlx::test]
async fn blanks_count_as_zero(pool: PgPool) {
    let api = api(pool).await;
    let d = ok(
        &api,
        json!({ "items": [
            { "id": 1, "quantity": "", "price": null, "vat": "" },
            { "id": 2 },
        ] }),
    )
    .await;
    assert_eq!(d["total"], json!(0));
    assert_eq!(
        d["total_vats"],
        json!({ "0": { "percentage": 0, "total_amount": 0 } })
    );
}

#[sqlx::test]
async fn numeric_strings_and_extra_keys(pool: PgPool) {
    let api = api(pool).await;
    let d = ok(
        &api,
        json!({ "disable_client_discounts": true, "items": [{
            "id": "a", "quantity": "2", "price": "9.9900", "vat": "21", "productDiscount": "5",
            "drag_key": 3, "product_type": "simple", "client_default_discount": "7",
            "relationships": {}, "position": 0,
        }] }),
    )
    .await;
    assert_eq!(d["total_excl"], json!(18.98));
    assert_eq!(d["total_vat"], json!(3.99));
    assert_eq!(d["total"], json!(22.97));
    assert_eq!(d["discounts"][0]["productDiscount"], "5");
}

#[sqlx::test]
async fn credit_lines_mirror_debit_lines(pool: PgPool) {
    let api = api(pool).await;
    let d = ok(
        &api,
        json!({ "items": [{ "id": 1, "quantity": -3, "price": 9.99, "vat": 21,
                            "productDiscount": 10 }] }),
    )
    .await;
    assert_eq!(d["total"], json!(-32.63));
    assert_eq!(d["total_vat"], json!(-5.66));
    // Laravel lists a line under discounts only when its discount is > 0.
    assert_eq!(d["discounts"], json!([]));
}

#[sqlx::test]
async fn five_and_five_and_a_half_stay_apart(pool: PgPool) {
    let api = api(pool).await;
    let d = ok(
        &api,
        json!({ "items": [
            item(json!(1), json!(100), json!(5.5)),
            item(json!(1), json!(100), json!(5)),
        ] }),
    )
    .await;
    assert_eq!(
        d["total_vats"],
        json!({
            "5.5": { "percentage": 5.5, "total_amount": 5.5 },
            "5": { "percentage": 5, "total_amount": 5 },
        })
    );
}

#[sqlx::test]
async fn full_discount_and_zero_quantity(pool: PgPool) {
    let api = api(pool).await;
    let d = ok(
        &api,
        json!({ "items": [
            { "id": 1, "quantity": 2, "price": 50, "vat": 21, "productDiscount": 100 },
            item(json!(0), json!(19.95), json!(21)),
        ] }),
    )
    .await;
    assert_eq!(d["sum_total"], json!(100));
    assert_eq!(d["total_discount"], json!(100));
    assert_eq!(d["total"], json!(0));
}

// --- boundary ------------------------------------------------------------------------------

#[sqlx::test]
async fn rounds_to_the_currencys_decimals(pool: PgPool) {
    let api = api(pool).await;
    let yen = api
        .with_base("/currencies")
        .create(
            json!({ "name": "Yen", "asset_type": "JPY", "decimals": 0, "active": true,
                    "default_currency": false }),
            "currency_id",
        )
        .await;
    let d = ok(
        &api,
        json!({ "currency_id": yen, "items": [item(json!(3), json!(9.99), json!(21))] }),
    )
    .await;
    assert_eq!(
        (&d["total_excl"], &d["total_vat"], &d["total"]),
        (&json!(30), &json!(6), &json!(36))
    );
}

#[sqlx::test]
async fn half_cents_round_away_from_zero(pool: PgPool) {
    let api = api(pool).await;
    let d = ok(
        &api,
        json!({ "items": [item(json!(1), json!(0.5), json!(21))] }),
    )
    .await;
    assert_eq!(d["total_vat"], json!(0.11));
    let d = ok(
        &api,
        json!({ "items": [item(json!(-3), json!(0.335), json!(21))] }),
    )
    .await;
    assert_eq!(d["total_excl"], json!(-1.01));
}

#[sqlx::test]
async fn eight_decimals_pass_nine_do_not(pool: PgPool) {
    let api = api(pool).await;
    ok(
        &api,
        json!({ "items": [item(json!(1), json!("0.12345678"), json!(21))] }),
    )
    .await;
    assert_eq!(
        error(
            &api,
            json!({ "items": [item(json!(1), json!("0.123456789"), json!(21))] }),
            "items.0.price"
        )
        .await,
        "The items.0.price must have 0-8 decimal places."
    );
}

#[sqlx::test]
async fn overflowing_line_is_422_not_500(pool: PgPool) {
    let api = api(pool).await;
    let max = "9999999999999999.99999999";
    ok(
        &api,
        json!({ "items": [item(json!(max), json!(1), json!(21))] }),
    )
    .await;
    assert_eq!(
        error(
            &api,
            json!({ "items": [item(json!(max), json!(max), json!(21))] }),
            "items.0"
        )
        .await,
        "The items.0 is out of range."
    );
    assert_eq!(
        error(
            &api,
            json!({ "items": [item(json!("99999999999999999"), json!(1), json!(0))] }),
            "items.0.quantity"
        )
        .await,
        "The items.0.quantity must not be greater than 9999999999999999.99999999."
    );
}

#[sqlx::test]
async fn at_most_1000_lines(pool: PgPool) {
    let api = api(pool).await;
    let lines = |n: usize| json!({ "items": vec![item(json!(1), json!(0.01), json!(21)); n] });
    let d = ok(&api, lines(1000)).await;
    assert_eq!(d["total_excl"], json!(10));
    assert_eq!(
        error(&api, lines(1001), "items").await,
        "The items must not have more than 1000 items."
    );
}

// --- performance ---------------------------------------------------------------------------

/// Products load in one batched query whatever the line count; a per-line lookup over 1000
/// lines blows well past the bound.
#[sqlx::test]
async fn thousand_product_lines_stay_fast(pool: PgPool) {
    let api = api(pool).await;
    let mut products = Vec::new();
    for i in 0..20 {
        products.push(product(&api, &format!("P{i}"), "1").await);
    }
    let items: Vec<Value> = (0..1000)
        .map(|i| {
            json!({ "id": i, "product_id": products[i % 20], "quantity": 1, "price": 2,
                         "vat": 21, "productDiscount": 1 })
        })
        .collect();
    let started = std::time::Instant::now();
    let d = ok(&api, json!({ "items": items })).await;
    let elapsed = started.elapsed();
    assert_eq!(d["discounts"].as_array().unwrap().len(), 1000);
    assert!(elapsed.as_millis() < 1_000, "calculate took {elapsed:?}");
}

// --- calculate-marge -----------------------------------------------------------------------

async fn marge(api: &Api, body: Value) -> (StatusCode, Value) {
    api.with_base("/calculate-marge")
        .call(Method::POST, "", Some(body), &api.owner)
        .await
}

fn zero_component() -> Value {
    json!({ "cost_excl": 0, "revenue_excl": 0, "margin_amount": 0, "margin_percentage": 0 })
}

#[sqlx::test]
async fn marge_has_laravels_shape(pool: PgPool) {
    let api = api(pool).await;
    let (status, res) = marge(
        &api,
        json!({ "items": [
            { "id": 1, "quantity": 2, "price": 25, "vat": 21, "purchase_price": 15 },
            { "id": 2, "quantity": 1, "price": 10, "vat": 21, "productDiscount": 10,
              "purchase_price": "4.5" },
        ] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let pct = res["data"]["marge_percentage"].as_f64().unwrap();
    assert!((pct - 41.525_423_728_813_56).abs() < 1e-9, "{pct}");
    let items = json!({ "cost_excl": 34.5, "revenue_excl": 59, "margin_amount": 24.5,
                        "margin_percentage": pct });
    assert_eq!(
        res["data"],
        json!({
            "total_purchase_price": 34.5,
            "marge_percentage": pct,
            "marge_total_amount": 24.5,
            "extra_data": {
                "items_percentage": pct,
                "items_purchase_price": 34.5,
                "items_amount": 24.5,
                "transfer_lists_percentage": 0,
                "transfer_lists_purchase_price": 0,
                "transfer_lists_amount": 0,
                "hours_purchase_price": 0,
                "hours_sale_price": 0,
                "components": {
                    "items": items,
                    "transfer_lists": zero_component(),
                    "hours": zero_component(),
                },
                "totals": items,
            },
        })
    );
}

#[sqlx::test]
async fn marge_falls_back_to_the_products_purchase_price(pool: PgPool) {
    let api = api(pool).await;
    let drill = product(&api, "Drill", "6").await;
    let (_, res) = marge(
        &api,
        json!({ "items": [{ "id": 1, "product_id": drill, "quantity": 2, "price": 10,
                            "vat": 21, "purchase_price": 0 }] }),
    )
    .await;
    assert_eq!(res["data"]["total_purchase_price"], json!(12));
    assert_eq!(res["data"]["marge_total_amount"], json!(8));
    assert_eq!(res["data"]["marge_percentage"], json!(40));
}

#[sqlx::test]
async fn marge_skips_set_children_and_divides_zero_revenue_by_one(pool: PgPool) {
    let api = api(pool).await;
    let (_, res) = marge(
        &api,
        json!({ "items": [
            { "id": 1, "quantity": 1, "price": 30, "vat": 21, "purchase_price": 10 },
            { "id": 2, "quantity": 1, "price": 30, "vat": 21, "purchase_price": 10,
              "multi_set_item_id": 1 },
        ] }),
    )
    .await;
    assert_eq!(res["data"]["total_purchase_price"], json!(10));
    let (_, res) = marge(
        &api,
        json!({ "items": [{ "id": 1, "quantity": 1, "price": 0, "purchase_price": 5 }] }),
    )
    .await;
    assert_eq!(res["data"]["marge_percentage"], json!(-500));
}

#[sqlx::test]
async fn marge_guards_and_validation_match_calculate_items(pool: PgPool) {
    let api = api(pool).await;
    let uri = format!("/v1/workspace/{}/calculate-marge", api.ws);
    let line = json!({ "items": [item(json!(1), json!(1), json!(21))] });
    let (status, _) = send(&api.app, Method::POST, &uri, Some(line.clone()), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (mallory, _) = signup(&api.app, "mallory@example.com", "mal").await;
    let (status, _) = send(&api.app, Method::POST, &uri, Some(line), Some(&mallory)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, res) = marge(
        &api,
        json!({ "items": [item(json!(1), json!(1), json!(-1))] }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["items.0.vat"][0],
        "The items.0.vat must be at least 0."
    );

    let max = "9999999999999999.99999999";
    let (status, res) = marge(
        &api,
        json!({ "items": [{ "quantity": max, "price": 0, "purchase_price": max }] }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"]["items.0"][0], "The items.0 is out of range.");
}
