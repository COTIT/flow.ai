//! /v1/workspace/{ws}/erp/special-offers and /erp/products/{product}/special-offers (Laravel
//! ERP/SpecialOffer), and the product prices an active offer sets (ProductPriceTrait).

use crate::common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::{Value, json};
use sqlx::PgPool;

const PERMS: &[&str] = &[
    "specialoffer.create",
    "specialoffer.list",
    "specialoffer.view",
    "specialoffer.update",
    "specialoffer.delete",
];

fn day(n: i64) -> String {
    (chrono::Utc::now().date_naive() + chrono::Duration::days(n)).to_string()
}

/// Workspace with a default 21 % VAT group and product P (price = gross = 100).
struct World {
    api: Api,
    product: String,
}

async fn world(pool: PgPool) -> World {
    let api = Api::new(pool, "/erp/special-offers").await;
    api.with_base("/settings/vat-groups")
        .create(
            json!({ "name": "High", "vat_amount": "21", "default_group": true, "country": "NL" }),
            "vat_group_id",
        )
        .await;
    let product = product(&api, json!({})).await;
    World { api, product }
}

async fn product(api: &Api, extra: Value) -> String {
    let mut body =
        json!({ "name": "Cable", "price": "100", "gross_price": "100", "brand": "Acme" });
    merge(&mut body, extra);
    api.with_base("/products").create(body, "product_id").await
}

fn merge(body: &mut Value, extra: Value) {
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
}

/// Legacy offer body (no rule type): `type` + `value` on one product.
fn legacy(kind: &str, value: &str, extra: Value) -> Value {
    let mut b = json!({ "type": kind, "value": value, "always_active": true });
    merge(&mut b, extra);
    b
}

fn on(w: &World) -> Api {
    let path: &'static str =
        Box::leak(format!("/erp/products/{}/special-offers", w.product).into_boxed_str());
    w.api.with_base(path)
}

async fn create(api: &Api, body: Value) -> String {
    api.create(body, "special_offer_id").await
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
    let w = world(pool).await;
    let api = on(&w);
    let id = create(
        &api,
        legacy(
            "fixed",
            "12.5",
            json!({ "always_active": false, "from_date": day(0), "to_date": day(3) }),
        ),
    )
    .await;
    let o = api.get(&id).await;
    assert_eq!(o["id"], id.as_str());
    assert_eq!(
        (&o["from_date"], &o["to_date"]),
        (&json!(day(0)), &json!(day(3)))
    );
    assert_eq!(o["always_active"], 0, "Laravel left the tinyint uncast");
    assert_eq!((&o["value"], &o["type"]), (&json!("12.5"), &json!("fixed")));
    assert_eq!(o["discount_rule_type"], "legacy");
    assert_eq!(
        (&o["discount_value"], &o["discount_type"]),
        (&json!("12.5"), &json!("fixed"))
    );
    assert_eq!(
        (&o["priority"], &o["apply_to_cheapest"]),
        (&json!(0), &json!(false))
    );
    assert_eq!(o["product_id"], w.product.as_str());
    assert_eq!(o["relationships"], json!([]));
    // The global route reaches the same offer.
    assert_eq!(w.api.get(&id).await["id"], id.as_str());

    let (status, res) = api.update(&id, json!({ "value": "20" })).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let o = api.get(&id).await;
    assert_eq!(
        (&o["value"], &o["always_active"], &o["product_id"]),
        (&json!("20"), &json!(0), &json!(w.product)),
        "a patch keeps always_active and the product (Laravel reset both)"
    );

    assert_eq!(api.list("").await["meta"]["total"], 1);
    assert_eq!(
        w.api.list("?discount_rule_type=legacy").await["meta"]["total"],
        1
    );
    assert_eq!(
        w.api.list("?discount_rule_type=bogo").await["meta"]["total"],
        0
    );
    api.delete(&id).await;
    let (status, _) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn product_route_only_reaches_its_own_offers(pool: PgPool) {
    let w = world(pool).await;
    let other = product(&w.api, json!({ "name": "Other" })).await;
    let id = create(&on(&w), legacy("fixed", "1", json!({}))).await;
    let path: &'static str =
        Box::leak(format!("/erp/products/{other}/special-offers").into_boxed_str());
    let wrong = w.api.with_base(path);
    for method in [Method::GET, Method::POST, Method::DELETE] {
        let body = (method == Method::POST).then(|| json!({ "value": "2" }));
        let (status, _) = wrong
            .call(method.clone(), &format!("/{id}"), body, &wrong.owner)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}");
    }
    assert_eq!(wrong.list("").await["meta"]["total"], 0);
    let (status, _) = w
        .api
        .with_base("/erp/products/01900000-0000-7000-8000-000000000001/special-offers")
        .call(Method::GET, "", None, &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown product");
}

#[sqlx::test]
async fn legacy_rules(pool: PgPool) {
    let w = world(pool).await;
    let api = on(&w);
    let cases = [
        (
            &w.api,
            legacy("fixed", "1", json!({})),
            "product_id",
            "The product id field is required.",
        ),
        (
            &api,
            json!({ "value": "1", "always_active": true }),
            "type",
            "The type field is required.",
        ),
        (
            &api,
            json!({ "type": "fixed", "always_active": true }),
            "value",
            "The value field is required.",
        ),
        (
            &api,
            legacy("wrong", "1", json!({})),
            "type",
            "The selected type: wrong is invalid.",
        ),
        (
            &api,
            legacy("fixed", "100.01", json!({})),
            "value",
            "The selected value is higher then the current product price.",
        ),
        (
            &api,
            legacy("percentage", "100", json!({})),
            "value",
            "Only values between 1 and 99 are available",
        ),
        (
            &api,
            legacy("percentage", "0", json!({})),
            "value",
            "Only values between 1 and 99 are available",
        ),
        (
            &api,
            legacy("fixed", "-1", json!({})),
            "value",
            "The value must be at least 0.",
        ),
        (
            &api,
            legacy("fixed", "1", json!({ "always_active": false })),
            "from_date",
            "The from date field is required.",
        ),
        (
            &api,
            legacy(
                "fixed",
                "1",
                json!({ "always_active": false, "from_date": day(2), "to_date": day(1) }),
            ),
            "from_date",
            "The from date must be a date before to date.",
        ),
        (
            &api,
            legacy(
                "fixed",
                "1",
                json!({ "always_active": false, "from_date": "soon", "to_date": day(1) }),
            ),
            "from_date",
            "The from date is not a valid date.",
        ),
        (
            &w.api,
            legacy(
                "fixed",
                "1",
                json!({ "product_id": "01900000-0000-7000-8000-000000000001" }),
            ),
            "product_id",
            "The product id does not exist",
        ),
    ];
    for (api, body, field, msg) in cases {
        assert_eq!(error(api, "", body, field).await, msg, "{field}");
    }
    // Dates may overlap no other offer of the product (an always-active one included).
    create(
        &api,
        legacy(
            "fixed",
            "1",
            json!({ "always_active": false, "from_date": day(0), "to_date": day(5) }),
        ),
    )
    .await;
    assert_eq!(
        error(
            &api,
            "",
            legacy(
                "fixed",
                "1",
                json!({ "always_active": false, "from_date": day(3), "to_date": day(9) })
            ),
            "to_date"
        )
        .await,
        "There is already an special offer active between the given dates."
    );
    // A timestamp is read as its UTC date (Laravel printed it so).
    let id = create(&api, legacy("fixed", "1", json!({ "always_active": false, "from_date": format!("{}T22:00:00.000Z", day(10)), "to_date": day(12) }))).await;
    assert_eq!(api.get(&id).await["from_date"], day(10));
}

#[sqlx::test]
async fn advanced_rules(pool: PgPool) {
    let w = world(pool).await;
    let g = &w.api;
    let unknown = "01900000-0000-7000-8000-000000000001";
    let cases = [
        (
            json!({ "discount_value": "5", "always_active": true }),
            "discount_rule_type",
            "The discount rule type field is required when discount type / discount value / condition quantity / condition brand / condition supplier relation id / condition category id / target brand / target supplier relation id / target category id / target product id / priority / apply to cheapest is present.",
        ),
        (
            json!({ "discount_rule_type": "nope", "discount_value": "5", "always_active": true }),
            "discount_rule_type",
            "The selected discount rule type: nope is invalid.",
        ),
        (
            json!({ "discount_rule_type": "brand_discount", "discount_value": "5", "always_active": true }),
            "product_id",
            "At least one condition (product_id, condition_brand, condition_supplier_relation_id, or condition_category_id) must be specified.",
        ),
        (
            json!({ "discount_rule_type": "global_discount", "always_active": true }),
            "discount_value",
            "Either discount_value or value must be provided.",
        ),
        (
            json!({ "discount_rule_type": "cross_product_discount", "condition_brand": "Acme", "discount_value": "5", "always_active": true }),
            "target_brand",
            "At least one target field must be specified for cross-product discounts.",
        ),
        (
            json!({ "discount_rule_type": "quantity_based", "condition_brand": "Acme", "discount_value": "5", "always_active": true }),
            "condition_quantity",
            "The condition quantity field is required.",
        ),
        (
            json!({ "discount_rule_type": "bogo", "condition_brand": "Acme", "condition_quantity": 2, "discount_value": "5", "always_active": true }),
            "discount_quantity",
            "The discount quantity field is required.",
        ),
        (
            json!({ "discount_rule_type": "brand_discount", "condition_supplier_relation_id": unknown, "discount_value": "5", "always_active": true }),
            "condition_supplier_relation_id",
            "The selected condition supplier relation id is invalid.",
        ),
        (
            json!({ "discount_rule_type": "brand_discount", "condition_brand": "Acme", "discount_type": "half", "discount_value": "5", "always_active": true }),
            "discount_type",
            "The selected discount type: half is invalid.",
        ),
        (
            json!({ "discount_rule_type": "brand_discount", "condition_brand": "Acme", "priority": -1, "discount_value": "5", "always_active": true }),
            "priority",
            "The priority must be at least 0.",
        ),
        (
            json!({ "discount_rule_type": "brand_discount", "condition_brand": "Acme", "discount_type": "free", "always_active": true, "discount_value": "1", "type": "free" }),
            "type",
            "The selected type: free is invalid.",
        ),
    ];
    for (body, field, msg) in cases {
        assert_eq!(error(g, "", body, field).await, msg, "{field}");
    }
    // A global discount needs no condition; `type` falls back to the discount type, and a
    // discount value of 0..100 without a type counts as a percentage (Laravel).
    let id = create(g, json!({ "discount_rule_type": "global_discount", "discount_value": "15", "always_active": true })).await;
    let o = g.get(&id).await;
    assert_eq!(
        (&o["discount_rule_type"], &o["discount_type"], &o["type"]),
        (
            &json!("global_discount"),
            &json!("percentage"),
            &json!("percentage")
        )
    );
    assert_eq!(
        (&o["value"], &o["discount_value"], &o["product_id"]),
        (&json!("15"), &json!("15"), &Value::Null)
    );
}

#[sqlx::test]
async fn access(pool: PgPool) {
    let w = world(pool).await;
    let body = json!({ "discount_rule_type": "global_discount", "discount_value": "5", "always_active": true });
    let id = create(&w.api, body.clone()).await;
    w.api.assert_member_forbidden(&id).await;
    w.api.assert_granted(PERMS, body, "special_offer_id").await;
    w.api
        .assert_foreign_404(&id, json!({ "priority": 1 }))
        .await;
    w.api.delete(&id).await;
    w.api
        .assert_bad_ids_404(&id, json!({ "priority": 1 }))
        .await;
}

#[sqlx::test]
async fn a_product_holds_at_most_100_offers(pool: PgPool) {
    let w = world(pool.clone()).await;
    sqlx::query(
        "INSERT INTO special_offers (workspace_id, product_id, value, type, from_date, to_date)
         SELECT $1::uuid, $2::uuid, 1, 'fixed', DATE '2000-01-01' + 2 * n, DATE '2000-01-02' + 2 * n
         FROM generate_series(1, 100) n",
    )
    .bind(&w.api.ws)
    .bind(&w.product)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        error(
            &on(&w),
            "",
            legacy("fixed", "1", json!({})),
            "special_offers"
        )
        .await,
        "The product can't hold more than 100 special offers."
    );
    // Lists and calculate-items only load today's offers; the detail loads them all.
    let list = w.api.with_base("/products").list("").await;
    assert_eq!(
        list["data"][0]["relationships"]["special_offers"],
        json!([])
    );
    let p = w.api.with_base("/products").get(&w.product).await;
    assert_eq!(
        p["relationships"]["special_offers"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
}

// --- product prices -----------------------------------------------------------------------

const FIELDS: [&str; 4] = [
    "price",
    "original_price",
    "business_price",
    "price_without_correction",
];

async fn prices(w: &World, id: &str) -> Vec<String> {
    let p = w.api.with_base("/products").get(id).await;
    FIELDS
        .iter()
        .map(|f| p[f].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn want(special: &str, original: &str) -> Vec<String> {
    [special, original, special, special]
        .map(str::to_owned)
        .to_vec()
}

#[sqlx::test]
async fn legacy_offers_set_the_price(pool: PgPool) {
    let w = world(pool).await;
    let api = on(&w);
    assert_eq!(prices(&w, &w.product).await, want("100", "100"));
    let id = create(&api, legacy("percentage", "10", json!({}))).await;
    assert_eq!(prices(&w, &w.product).await, want("90", "100"));
    let p = w.api.with_base("/products").get(&w.product).await;
    assert_eq!(
        p["relationships"]["active_special_offer"]["id"],
        id.as_str()
    );
    assert_eq!(
        p["relationships"]["special_offers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // A legacy percentage freezes the product price it was made on (Laravel base_price).
    let (status, res) = w
        .api
        .with_base("/products")
        .update(&w.product, json!({ "price": "200" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(prices(&w, &w.product).await, want("90", "200"));
    api.update(&id, json!({ "value": "20" })).await;
    assert_eq!(
        prices(&w, &w.product).await,
        want("160", "200"),
        "editing the offer refreezes"
    );

    api.update(&id, json!({ "type": "exact", "value": "150" }))
        .await;
    assert_eq!(prices(&w, &w.product).await, want("150", "200"));
}

#[sqlx::test]
async fn dated_offers_count_both_days(pool: PgPool) {
    let w = world(pool).await;
    let api = on(&w);
    let id = create(
        &api,
        legacy(
            "fixed",
            "10",
            json!({ "always_active": false, "from_date": day(1), "to_date": day(3) }),
        ),
    )
    .await;
    assert_eq!(
        prices(&w, &w.product).await,
        want("100", "100"),
        "starts tomorrow"
    );
    api.update(&id, json!({ "from_date": day(-3), "to_date": day(0) }))
        .await;
    assert_eq!(
        prices(&w, &w.product).await,
        want("90", "100"),
        "ends today"
    );
    api.update(&id, json!({ "from_date": day(-3), "to_date": day(-1) }))
        .await;
    assert_eq!(prices(&w, &w.product).await, want("100", "100"), "ended");
}

#[sqlx::test]
async fn highest_priority_then_newest_wins(pool: PgPool) {
    let w = world(pool).await;
    let api = on(&w);
    let dated = create(
        &api,
        legacy(
            "fixed",
            "10",
            json!({ "always_active": false, "from_date": day(0), "to_date": day(2) }),
        ),
    )
    .await;
    // Always-active offers skip the overlap check, so two offers can be active together.
    create(&api, json!({ "discount_rule_type": "quantity_based", "condition_quantity": 1, "discount_type": "fixed", "discount_value": "30", "priority": 5, "always_active": true })).await;
    assert_eq!(prices(&w, &w.product).await, want("70", "100"));
    create(&api, json!({ "discount_rule_type": "quantity_based", "condition_quantity": 1, "discount_type": "fixed", "discount_value": "40", "priority": 5, "always_active": true })).await;
    assert_eq!(
        prices(&w, &w.product).await,
        want("60", "100"),
        "same priority: newest"
    );
    // Editing the dated offer without touching its dates skips the overlap check.
    let (status, res) = api.update(&dated, json!({ "value": "11" })).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    // A zero discount value counts as missing, as Laravel's `! $value` did.
    assert_eq!(
        error(&w.api, "", json!({ "discount_rule_type": "global_discount", "discount_value": "0", "always_active": true }), "discount_value").await,
        "Either discount_value or value must be provided."
    );
}

#[sqlx::test]
async fn advanced_offers_set_the_price(pool: PgPool) {
    let w = world(pool).await;
    let api = on(&w);
    // No cart needed: fixed discount_value, floored at 0.
    let id = create(&api, json!({ "discount_rule_type": "brand_discount", "discount_type": "fixed", "discount_value": "15", "always_active": true })).await;
    assert_eq!(prices(&w, &w.product).await, want("85", "100"));
    api.update(&id, json!({ "discount_value": "500" })).await;
    assert_eq!(prices(&w, &w.product).await, want("0.00", "100"));
    api.delete(&id).await;
    // Cart rules on a product listing use the offer's own discount (20 % inferred).
    let id = create(&api, json!({ "discount_rule_type": "global_discount", "discount_value": "20", "always_active": true })).await;
    assert_eq!(prices(&w, &w.product).await, want("80", "100"));
    api.delete(&id).await;
    // A percentage over 100 never makes the price negative (Laravel printed it below 0).
    let id = create(&api, json!({ "discount_rule_type": "brand_discount", "type": "percentage", "value": "500", "always_active": true })).await;
    assert_eq!(prices(&w, &w.product).await, want("0.00", "100"));
    api.delete(&id).await;
    // A free-item rule takes the whole price off.
    create(&api, json!({ "discount_rule_type": "bogo", "condition_quantity": 1, "discount_quantity": 1, "discount_type": "free", "discount_value": "1", "always_active": true })).await;
    assert_eq!(prices(&w, &w.product).await, want("0.00", "100"));
}

#[sqlx::test]
async fn offers_feed_price_corrections_and_business_settings(pool: PgPool) {
    let w = world(pool).await;
    // 10 % below gross: business prices start from gross with the setting on.
    let p = product(&w.api, json!({ "price": "90", "gross_price": "100" })).await;
    let path: &'static str =
        Box::leak(format!("/erp/products/{p}/special-offers").into_boxed_str());
    let api = w.api.with_base(path);
    create(&api, json!({ "discount_rule_type": "brand_discount", "discount_type": "percentage", "discount_value": "10", "always_active": true })).await;
    assert_eq!(prices(&w, &p).await, ["81", "90", "81", "81"]);
    w.api
        .with_base("/settings")
        .call(
            Method::POST,
            "",
            Some(json!({ "product": { "disable_double_discount_for_business": true } })),
            &w.api.owner,
        )
        .await;
    // With an active offer the consumer price also starts from gross.
    assert_eq!(prices(&w, &p).await, ["90", "90", "90", "90"]);
    // A price band rounds the special price: 90 excl = 108.90 incl -> 109 - 0.01.
    w.api
        .with_base("/products/price-corrections")
        .create(json!({ "value_from": "0", "value_to": "1000", "round_on": "1", "remove_amount": "0.01" }), "price_correction_id")
        .await;
    let got = prices(&w, &p).await;
    assert_eq!((got[0].as_str(), got[3].as_str()), ("90.0744", "90"));
}

#[sqlx::test]
async fn a_bigger_percentage_offer_cancels_the_customer_discount(pool: PgPool) {
    let w = world(pool.clone()).await;
    let supplier = w
        .api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Supplier", "type": "business" }),
            "relation_id",
        )
        .await;
    let customer = w
        .api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Customer", "type": "business" }),
            "relation_id",
        )
        .await;
    let p = product(
        &w.api,
        json!({ "supplier_relation_id": supplier, "supplier_discount_code": "CAB" }),
    )
    .await;
    let group = w
        .api
        .with_base("/products/discount-groups")
        .create(
            json!({ "name": "Retail", "active": true, "supplier_relation_id": supplier }),
            "discount_group_id",
        )
        .await;
    w.api.with_base("/products/discounts")
        .create(json!({ "discount_group_id": group, "name": "D", "active": true, "discount_product_code": "CAB", "percentage": "12.5" }), "discount_id").await;
    w.api
        .with_base("/products/discount-groups")
        .call(
            Method::POST,
            &format!("/{group}/attach-to-relations"),
            Some(json!({ "relation_ids": [customer] })),
            &w.api.owner,
        )
        .await;
    let calc = |p: String, customer: String| json!({ "relation_id": customer, "items": [{ "id": 1, "product_id": p, "quantity": 1, "price": 100, "vat": 0 }] });
    let total_discount = |res: Value| res["data"]["total_discount"].clone();
    let path: &'static str =
        Box::leak(format!("/erp/products/{p}/special-offers").into_boxed_str());
    let offers = w.api.with_base(path);
    let id = create(&offers, legacy("percentage", "10", json!({}))).await;
    let (_, res) = w
        .api
        .with_base("/calculate-items")
        .call(
            Method::POST,
            "",
            Some(calc(p.clone(), customer.clone())),
            &w.api.owner,
        )
        .await;
    assert_eq!(
        total_discount(res),
        json!(12.5),
        "offer 10 % < discount 12.5 %"
    );
    offers.update(&id, json!({ "value": "20" })).await;
    let (_, res) = w
        .api
        .with_base("/calculate-items")
        .call(Method::POST, "", Some(calc(p, customer)), &w.api.owner)
        .await;
    assert_eq!(
        total_discount(res),
        json!(0),
        "offer 20 % beats the discount: discount dropped"
    );
}
