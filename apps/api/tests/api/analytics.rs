//! Sales analytics (top customers, best sellers, period compare) and the accountant invoice
//! export. Orders are written straight in SQL so each test controls sold-at times and cents.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::common::{self, Api, invoicing};

pub struct W {
    pub api: Api,
    pub eur: String,
    pub usd: String,
    pub warehouse: String,
    pub alpha: String,
    pub beta: String,
    pub honey: String,
    pub wax: String,
}

pub async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "").await;
    let relation = |name: &'static str| {
        let api = api.with_base("/relations");
        async move {
            api.create(
                json!({ "company_name": name, "type": "business" }),
                "relation_id",
            )
            .await
        }
    };
    let (alpha, beta) = (relation("Alpha Farm").await, relation("Beta Shop").await);
    let product = |name: &'static str| {
        let api = api.with_base("/products");
        async move {
            api.create(json!({ "name": name, "price": "1" }), "product_id")
                .await
        }
    };
    let (honey, wax) = (product("Honey").await, product("Wax").await);
    let warehouse = api
        .with_base("/warehouses")
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let eur: String = sqlx::query_scalar(
        "SELECT id::text FROM currencies WHERE workspace_id = $1::uuid AND default_currency",
    )
    .bind(&api.ws)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    let usd = api
        .with_base("/currencies")
        .create(
            json!({ "name": "Dollar", "asset_type": "usd", "decimals": 2,
                    "default_currency": false, "active": true }),
            "currency_id",
        )
        .await;
    W {
        api,
        eur,
        usd,
        warehouse,
        alpha,
        beta,
        honey,
        wax,
    }
}

/// One line: product, variant, quantity, excl and VAT cents.
pub struct L<'a>(
    pub &'a str,
    pub Option<&'a str>,
    pub &'a str,
    pub i64,
    pub i64,
);

/// An order sold (`confirmed_at`) at `at`; subtotal, VAT and total = sums of its lines.
#[allow(clippy::too_many_arguments)]
pub async fn order(
    w: &W,
    at: &str,
    relation: Option<&str>,
    currency: &str,
    status: &str,
    source: &str,
    lines: &[L<'_>],
) {
    let total: i64 = lines.iter().map(|l| l.3 + l.4).sum();
    let id: String = sqlx::query_scalar(
        "INSERT INTO orders (workspace_id, number, status, source, relation_id, currency_id,
                             warehouse_id, subtotal, vat, total, confirmed_at)
         VALUES ($1::uuid, gen_random_uuid()::text, $2, $3, $4::uuid, $5::uuid, $6::uuid, $7, $8,
                 $9, $10::timestamptz)
         RETURNING id::text",
    )
    .bind(&w.api.ws)
    .bind(status)
    .bind(source)
    .bind(relation)
    .bind(currency)
    .bind(&w.warehouse)
    .bind(lines.iter().map(|l| l.3).sum::<i64>())
    .bind(lines.iter().map(|l| l.4).sum::<i64>())
    .bind(total)
    .bind(at)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    for (i, l) in lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO order_lines (workspace_id, order_id, position, product_id,
                                      product_variant_id, title, quantity, unit_price, vat_rate,
                                      line_excl, line_vat, tracked)
             VALUES ($1::uuid, $2::uuid, $3, $4::uuid, $5::uuid,
                     (SELECT name FROM products WHERE id = $4::uuid), $6::numeric, 0, 9, $7, $8,
                     false)",
        )
        .bind(&w.api.ws)
        .bind(&id)
        .bind(i32::try_from(i).unwrap() + 1)
        .bind(l.0)
        .bind(l.1)
        .bind(l.2)
        .bind(l.3)
        .bind(l.4)
        .execute(&w.api.pool)
        .await
        .unwrap();
    }
}

async fn get(api: &Api, token: &str, ws: &str, path: &str) -> (StatusCode, Value) {
    common::send(
        &api.app,
        Method::GET,
        &format!("/v1/workspace/{ws}{path}"),
        None,
        Some(token),
    )
    .await
}

async fn owner_get(w: &W, path: &str) -> Value {
    let (s, res) = get(&w.api, &w.api.owner, &w.api.ws, path).await;
    assert_eq!(s, StatusCode::OK, "{path}: {res}");
    res["data"].clone()
}

const OCT: &str = "from=2026-10-01T00:00:00Z&to=2026-11-01T00:00:00Z";

/// Alpha: 1000 + 500 EUR (two orders), 700 USD. Beta: 2000 EUR. Walk-in: 300 EUR.
/// Draft, cancelled and out-of-range orders never count.
async fn october(w: &W) {
    let (a, b) = (w.alpha.as_str(), w.beta.as_str());
    let (h, x) = (w.honey.as_str(), w.wax.as_str());
    order(
        w,
        "2026-10-02T10:00:00Z",
        Some(a),
        &w.eur,
        "completed",
        "office",
        &[L(h, None, "2", 900, 100)],
    )
    .await;
    order(
        w,
        "2026-10-03T10:00:00Z",
        Some(a),
        &w.eur,
        "open",
        "storefront",
        &[L(x, None, "1", 450, 50)],
    )
    .await;
    order(
        w,
        "2026-10-04T10:00:00Z",
        Some(b),
        &w.eur,
        "partially_returned",
        "office",
        &[L(h, None, "3", 1350, 150), L(x, None, "1", 450, 50)],
    )
    .await;
    order(
        w,
        "2026-10-05T10:00:00Z",
        None,
        &w.eur,
        "completed",
        "till",
        &[L(x, None, "1", 270, 30)],
    )
    .await;
    order(
        w,
        "2026-10-06T10:00:00Z",
        Some(a),
        &w.usd,
        "completed",
        "office",
        &[L(h, None, "1", 630, 70)],
    )
    .await;
    order(
        w,
        "2026-10-07T10:00:00Z",
        Some(a),
        &w.eur,
        "cancelled",
        "office",
        &[L(h, None, "9", 9000, 999)],
    )
    .await;
    order(
        w,
        "2026-10-07T10:00:00Z",
        Some(b),
        &w.eur,
        "draft",
        "office",
        &[L(h, None, "9", 9000, 999)],
    )
    .await;
    order(
        w,
        "2026-11-01T00:00:00Z",
        Some(a),
        &w.eur,
        "completed",
        "office",
        &[L(h, None, "9", 9000, 999)],
    )
    .await;
}

#[sqlx::test]
async fn top_customers_rank_per_currency(pool: PgPool) {
    let w = world(pool).await;
    october(&w).await;
    let d = owner_get(&w, &format!("/sales/top-customers?{OCT}")).await;
    assert_eq!(d["from"], "2026-10-01T00:00:00Z");
    assert_eq!(d["to"], "2026-11-01T00:00:00Z");
    assert_eq!(d["time_zone"], "UTC");
    assert_eq!(
        d["currencies"],
        json!([
            { "currency": "EUR", "decimals": 2,
              "rows": [
                  { "relation": { "id": w.beta, "name": "Beta Shop" }, "orders": 1, "gross_total": 2000, "net_total": 1800, "vat_total": 200 },
                  { "relation": { "id": w.alpha, "name": "Alpha Farm" }, "orders": 2, "gross_total": 1500, "net_total": 1350, "vat_total": 150 },
              ],
              "without_customer": { "orders": 1, "gross_total": 300, "net_total": 270, "vat_total": 30 } },
            { "currency": "USD", "decimals": 2,
              "rows": [ { "relation": { "id": w.alpha, "name": "Alpha Farm" }, "orders": 1, "gross_total": 700, "net_total": 630, "vat_total": 70 } ],
              "without_customer": { "orders": 0, "gross_total": 0, "net_total": 0, "vat_total": 0 } },
        ]),
        "{d}"
    );
    let d = owner_get(
        &w,
        &format!("/sales/top-customers?{OCT}&limit=1&source=office"),
    )
    .await;
    let eur = &d["currencies"][0];
    assert_eq!(eur["rows"].as_array().unwrap().len(), 1, "{d}");
    assert_eq!(eur["rows"][0]["relation"]["id"], w.beta.as_str());
    assert_eq!(
        eur["without_customer"]["orders"], 0,
        "till walk-in is not office"
    );
    assert_eq!(d["source"], "office");
}

/// "Who buys the most honey?": with `product_id` customers rank by that product's line totals
/// (other products on the same orders left out); `variant_id` narrows to one size.
#[sqlx::test]
async fn top_customers_filter_by_product_and_variant(pool: PgPool) {
    let w = world(pool).await;
    october(&w).await;
    let d = owner_get(
        &w,
        &format!("/sales/top-customers?{OCT}&product_id={}", w.honey),
    )
    .await;
    assert_eq!(
        d["currencies"],
        json!([
            { "currency": "EUR", "decimals": 2,
              "rows": [
                  { "relation": { "id": w.beta, "name": "Beta Shop" }, "orders": 1, "gross_total": 1500, "net_total": 1350, "vat_total": 150 },
                  { "relation": { "id": w.alpha, "name": "Alpha Farm" }, "orders": 1, "gross_total": 1000, "net_total": 900, "vat_total": 100 },
              ],
              "without_customer": { "orders": 0, "gross_total": 0, "net_total": 0, "vat_total": 0 } },
            { "currency": "USD", "decimals": 2,
              "rows": [ { "relation": { "id": w.alpha, "name": "Alpha Farm" }, "orders": 1, "gross_total": 700, "net_total": 630, "vat_total": 70 } ],
              "without_customer": { "orders": 0, "gross_total": 0, "net_total": 0, "vat_total": 0 } },
        ]),
        "beta's wax line is not honey: {d}"
    );
    let d = owner_get(
        &w,
        &format!("/sales/top-customers?{OCT}&product_id={}", w.wax),
    )
    .await;
    let eur = &d["currencies"][0];
    assert_eq!(
        eur["rows"][0]["relation"]["name"], "Alpha Farm",
        "tie: by name"
    );
    assert_eq!(eur["rows"][1]["gross_total"], 500);
    assert_eq!(
        eur["without_customer"],
        json!({ "orders": 1, "gross_total": 300, "net_total": 270, "vat_total": 30 })
    );
    assert_eq!(
        d["currencies"].as_array().unwrap().len(),
        1,
        "no wax in USD"
    );

    // A zero-VAT 250 g size, sold to Beta.
    let size: String = sqlx::query_scalar(
        "INSERT INTO product_variants (workspace_id, product_id, sku)
         VALUES ($1::uuid, $2::uuid, 'HON-250') RETURNING id::text",
    )
    .bind(&w.api.ws)
    .bind(&w.honey)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    order(
        &w,
        "2026-10-09T10:00:00Z",
        Some(&w.beta),
        &w.eur,
        "completed",
        "office",
        &[L(&w.honey, Some(&size), "1", 4000, 0)],
    )
    .await;
    let d = owner_get(
        &w,
        &format!(
            "/sales/top-customers?{OCT}&product_id={}&variant_id={size}",
            w.honey
        ),
    )
    .await;
    assert_eq!(
        d["currencies"],
        json!([{ "currency": "EUR", "decimals": 2,
                 "rows": [ { "relation": { "id": w.beta, "name": "Beta Shop" }, "orders": 1, "gross_total": 4000, "net_total": 4000, "vat_total": 0 } ],
                 "without_customer": { "orders": 0, "gross_total": 0, "net_total": 0, "vat_total": 0 } }]),
        "{d}"
    );
    let d = owner_get(
        &w,
        &format!("/sales/top-customers?{OCT}&product_id={}", w.honey),
    )
    .await;
    assert_eq!(
        d["currencies"][0]["rows"][0],
        json!({ "relation": { "id": w.beta, "name": "Beta Shop" }, "orders": 2, "gross_total": 5500, "net_total": 5350, "vat_total": 150 }),
        "all sizes together"
    );
    let d = owner_get(
        &w,
        &format!(
            "/sales/top-products?{OCT}&product_id={}&variant_id={size}",
            w.honey
        ),
    )
    .await;
    assert_eq!(
        d["currencies"][0]["rows"][0]["gross_total"], 4000,
        "top-products too"
    );

    // Another workspace's product finds nothing here.
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let jam = eve
        .with_base("/products")
        .create(json!({ "name": "Jam", "price": "1" }), "product_id")
        .await;
    let d = owner_get(&w, &format!("/sales/top-customers?{OCT}&product_id={jam}")).await;
    assert_eq!(d["currencies"], json!([]));
}

#[sqlx::test]
async fn top_products_rank_by_gross(pool: PgPool) {
    let w = world(pool).await;
    october(&w).await;
    let d = owner_get(&w, &format!("/sales/top-products?{OCT}")).await;
    assert_eq!(d["by"], "product");
    assert_eq!(
        d["currencies"][0],
        json!({ "currency": "EUR", "decimals": 2, "rows": [
            { "product": { "id": w.honey, "name": "Honey" }, "variant": null, "title": "Honey",
              "units": "5.00", "lines": 2, "orders": 2, "gross_total": 2500, "net_total": 2250, "vat_total": 250 },
            { "product": { "id": w.wax, "name": "Wax" }, "variant": null, "title": "Wax",
              "units": "3.00", "lines": 3, "orders": 3, "gross_total": 1300, "net_total": 1170, "vat_total": 130 },
        ] }),
        "{d}"
    );
    assert_eq!(d["currencies"][1]["rows"][0]["gross_total"], 700);

    // By variant: honey's 250 g size sells apart from the plain product.
    let size: String = sqlx::query_scalar(
        "INSERT INTO product_variants (workspace_id, product_id, sku)
         VALUES ($1::uuid, $2::uuid, 'HON-250') RETURNING id::text",
    )
    .bind(&w.api.ws)
    .bind(&w.honey)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    order(
        &w,
        "2026-10-09T10:00:00Z",
        None,
        &w.eur,
        "completed",
        "till",
        &[L(&w.honey, Some(&size), "1", 4000, 0)],
    )
    .await;
    let d = owner_get(&w, &format!("/sales/top-products?{OCT}&by=variant&limit=2")).await;
    assert_eq!(d["by"], "variant");
    let rows = &d["currencies"][0]["rows"];
    assert_eq!(rows.as_array().unwrap().len(), 2, "{d}");
    assert_eq!(rows[0]["variant"], json!({ "id": size, "sku": "HON-250" }));
    assert_eq!(rows[0]["gross_total"], 4000);
    assert_eq!(rows[1]["variant"], Value::Null);
    assert_eq!(rows[1]["gross_total"], 2500);

    // One product's sales: "how much honey did we sell in October?"
    let d = owner_get(
        &w,
        &format!("/sales/top-products?{OCT}&product_id={}", w.honey),
    )
    .await;
    let rows = &d["currencies"][0]["rows"];
    assert_eq!(rows.as_array().unwrap().len(), 1, "{d}");
    assert_eq!(rows[0]["gross_total"], 6500, "both sizes together");
    assert_eq!(rows[0]["units"], "6.00");
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let d = owner_get(
        &w,
        &format!("/sales/top-products?{OCT}&product_id={}", eve.ws),
    )
    .await;
    assert_eq!(d["currencies"], json!([]), "an unknown id finds nothing");
}

#[sqlx::test]
async fn compare_gives_difference_and_change(pool: PgPool) {
    let w = world(pool).await;
    october(&w).await;
    // September: 1000 EUR in one order, nothing in USD; a yen sale only in September.
    order(
        &w,
        "2026-09-10T10:00:00Z",
        Some(&w.alpha),
        &w.eur,
        "completed",
        "office",
        &[L(&w.honey, None, "1", 900, 100)],
    )
    .await;
    let jpy = w
        .api
        .with_base("/currencies")
        .create(
            json!({ "name": "Yen", "asset_type": "jpy", "decimals": 0,
                    "default_currency": false, "active": true }),
            "currency_id",
        )
        .await;
    order(
        &w,
        "2026-09-11T10:00:00Z",
        None,
        &jpy,
        "completed",
        "till",
        &[L(&w.wax, None, "1", 500, 0)],
    )
    .await;
    let d = owner_get(
        &w,
        &format!(
            "/sales/compare?{OCT}&compare_from=2026-09-01T00:00:00Z&compare_to=2026-10-01T00:00:00Z"
        ),
    )
    .await;
    assert_eq!(
        d["period"],
        json!({ "from": "2026-10-01T00:00:00Z", "to": "2026-11-01T00:00:00Z" })
    );
    assert_eq!(
        d["compare_to_period"],
        json!({ "from": "2026-09-01T00:00:00Z", "to": "2026-10-01T00:00:00Z" })
    );
    assert_eq!(
        d["currencies"],
        json!([
            { "currency": "EUR", "decimals": 2,
              "current": { "orders": 4, "gross_total": 3800, "net_total": 3420, "vat_total": 380, "refunded_total": 0 },
              "base": { "orders": 1, "gross_total": 1000, "net_total": 900, "vat_total": 100, "refunded_total": 0 },
              "gross_difference": 2800, "orders_difference": 3, "gross_change_bp": 28000,
              "net_difference": 2520, "net_change_bp": 28000 },
            { "currency": "JPY", "decimals": 0,
              "current": { "orders": 0, "gross_total": 0, "net_total": 0, "vat_total": 0, "refunded_total": 0 },
              "base": { "orders": 1, "gross_total": 500, "net_total": 500, "vat_total": 0, "refunded_total": 0 },
              "gross_difference": -500, "orders_difference": -1, "gross_change_bp": -10000,
              "net_difference": -500, "net_change_bp": -10000 },
            { "currency": "USD", "decimals": 2,
              "current": { "orders": 1, "gross_total": 700, "net_total": 630, "vat_total": 70, "refunded_total": 0 },
              "base": { "orders": 0, "gross_total": 0, "net_total": 0, "vat_total": 0, "refunded_total": 0 },
              "gross_difference": 700, "orders_difference": 1, "net_difference": 630 },
        ]),
        "no change % from a zero base; {d}"
    );
    // The same period on both sides: no change.
    let d = owner_get(
        &w,
        "/sales/compare?from=2026-09-11T00:00:00Z&to=2026-09-12T00:00:00Z\
         &compare_from=2026-09-11T00:00:00Z&compare_to=2026-09-12T00:00:00Z",
    )
    .await;
    assert_eq!(d["currencies"][0]["gross_change_bp"], 0, "same period: 0 %");
}

#[sqlx::test]
async fn plain_dates_start_at_midnight_in_the_workspace_time_zone(pool: PgPool) {
    let w = world(pool).await;
    // 22:30 UTC on 31 Oct is 01:30 on 1 Nov in Dar es Salaam (UTC+3).
    order(
        &w,
        "2026-10-31T22:30:00Z",
        Some(&w.alpha),
        &w.eur,
        "completed",
        "office",
        &[L(&w.honey, None, "1", 100, 0)],
    )
    .await;
    let q = "/sales/top-customers?from=2026-10-01&to=2026-11-01";
    let d = owner_get(&w, q).await;
    assert_eq!(d["time_zone"], "UTC");
    assert_eq!(
        d["currencies"].as_array().unwrap().len(),
        1,
        "UTC: still October"
    );

    let settings = |tz: &'static str| {
        let api = w.api.with_base("/settings");
        async move {
            let (s, res) = api
                .call(
                    Method::POST,
                    "",
                    Some(json!({ "general": { "timezone": tz } })),
                    &api.owner,
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{res}");
        }
    };
    settings("Africa/Dar_es_Salaam").await;
    let d = owner_get(&w, q).await;
    assert_eq!(d["time_zone"], "Africa/Dar_es_Salaam");
    assert_eq!(d["from"], "2026-09-30T21:00:00Z");
    assert_eq!(d["to"], "2026-10-31T21:00:00Z");
    assert_eq!(d["currencies"], json!([]), "local time: already November");
    // An instant stays an instant.
    let d = owner_get(
        &w,
        "/sales/top-customers?from=2026-10-01&to=2026-11-01T00:00:00Z",
    )
    .await;
    assert_eq!(d["currencies"].as_array().unwrap().len(), 1);

    settings("Mars/Olympus").await;
    let d = owner_get(&w, q).await;
    assert_eq!(d["time_zone"], "UTC", "unknown zone falls back to UTC");
}

#[sqlx::test]
async fn analytics_refuse_bad_queries(pool: PgPool) {
    let w = world(pool).await;
    for (path, field) in [
        ("/sales/top-customers?to=2026-11-01", "from"),
        ("/sales/top-customers?from=2026-10-01", "to"),
        ("/sales/top-customers?from=yesterday&to=2026-11-01", "from"),
        ("/sales/top-customers?from=2026-02-30&to=2026-11-01", "from"),
        ("/sales/top-customers?from=2026-11-01&to=2026-11-01", "to"),
        ("/sales/top-customers?from=2025-01-01&to=2026-11-01", "to"),
        (
            "/sales/top-customers?from=2026-10-01&to=2026-11-01&limit=0",
            "limit",
        ),
        (
            "/sales/top-customers?from=2026-10-01&to=2026-11-01&limit=21",
            "limit",
        ),
        (
            "/sales/top-products?from=2026-10-01&to=2026-11-01&limit=x",
            "limit",
        ),
        (
            "/sales/top-products?from=2026-10-01&to=2026-11-01&by=colour",
            "by",
        ),
        (
            "/sales/top-products?from=2026-10-01&to=2026-11-01&source=fax",
            "source",
        ),
        (
            "/sales/compare?from=2026-10-01&to=2026-11-01",
            "compare_from",
        ),
        (
            "/sales/compare?from=2026-10-01&to=2026-11-01&compare_from=2026-09-01&compare_to=2026-08-01",
            "compare_to",
        ),
        ("/invoices-export?from=2026-10-01", "to"),
        (
            "/sales/top-products?from=2026-10-01&to=2026-11-01&product_id=honey",
            "product_id",
        ),
        (
            "/sales/top-customers?from=2026-10-01&to=2026-11-01&product_id=honey",
            "product_id",
        ),
        (
            "/sales/top-customers?from=2026-10-01&to=2026-11-01&variant_id=x",
            "variant_id",
        ),
    ] {
        let (s, res) = get(&w.api, &w.api.owner, &w.api.ws, path).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {res}");
        assert!(res["errors"][field].is_array(), "{path}: {res}");
    }
    let d = owner_get(
        &w,
        "/sales/top-customers?from=2026-10-01&to=2026-11-01&limit=20",
    )
    .await;
    assert_eq!(d["currencies"], json!([]));
}

#[sqlx::test]
async fn analytics_stay_inside_the_workspace_and_its_permissions(pool: PgPool) {
    let w = world(pool).await;
    october(&w).await;
    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    for path in [
        format!("/sales/top-customers?{OCT}"),
        format!("/sales/top-customers?{OCT}&product_id={}", w.honey),
        format!("/sales/top-products?{OCT}"),
        format!("/sales/compare?{OCT}&compare_from=2026-09-01&compare_to=2026-10-01"),
        format!("/invoices-export?{OCT}"),
    ] {
        // Eve's own workspace sees none of Acme's sales.
        let (s, res) = get(&eve, &eve.owner, &eve.ws, &path).await;
        assert_eq!(s, StatusCode::OK, "{path}: {res}");
        let empty = res["data"]["currencies"] == json!([]) || res["data"]["invoices"] == 0;
        assert!(empty, "{path}: {res}");
        // Acme's workspace refuses her.
        let (s, _) = get(&eve, &eve.owner, &w.api.ws, &path).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}");
    }
    let orders_only = common::member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "olly@example.com",
        &["order.list"],
    )
    .await;
    let (s, _) = get(
        &w.api,
        &orders_only,
        &w.api.ws,
        &format!("/sales/top-customers?{OCT}"),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "customer names need relation.list"
    );
    let (s, _) = get(
        &w.api,
        &orders_only,
        &w.api.ws,
        &format!("/sales/top-products?{OCT}"),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = get(
        &w.api,
        &orders_only,
        &w.api.ws,
        &format!("/invoices-export?{OCT}"),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "export needs invoice.list");
    let nobody =
        common::member_with(&w.api.app, &w.api.pool, &w.api.ws, "nat@example.com", &[]).await;
    for path in ["/sales/top-products", "/sales/compare"] {
        let (s, _) = get(&w.api, &nobody, &w.api.ws, &format!("{path}?{OCT}")).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}");
    }
}

// --- invoice export --------------------------------------------------------------------------

/// Sent (numbered) invoice of `lines`, its send date moved to `at`.
async fn sent(f: &invoicing::F, lines: Value, credit: bool, at: &str) -> String {
    let id = invoicing::draft(f, lines, credit).await;
    let (s, res) = invoicing::send(f, &id, invoicing::message(f, "manual")).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    sqlx::query("UPDATE invoices SET send_date = $2::timestamptz WHERE id = $1::uuid")
        .bind(&id)
        .bind(at)
        .execute(&f.api.pool)
        .await
        .unwrap();
    id
}

async fn export(f: &invoicing::F, q: &str) -> (StatusCode, Value) {
    get(
        &f.api,
        &f.api.owner,
        &f.api.ws,
        &format!("/invoices-export?{q}"),
    )
    .await
}

#[sqlx::test]
async fn invoice_export_is_a_csv_for_the_accountant(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    // A customer name that a spreadsheet would run as a formula, with a comma and a quote.
    sqlx::query(
        "UPDATE relations SET company_name = '=HYPERLINK(\"x\"), Ltd', vat_number = 'NL001'
         WHERE id = $1::uuid",
    )
    .bind(&f.rel)
    .execute(&f.api.pool)
    .await
    .unwrap();
    // Two chairs at 10 excl, 21 % VAT: 20.00 + 4.20.
    let first = sent(
        &f,
        invoicing::chairs(&f, 2, None),
        false,
        "2026-10-05T09:00:00Z",
    )
    .await;
    let credit = sent(
        &f,
        json!([{ "title": "Chair", "quantity": -1, "price": "10", "vat": "21" }]),
        true,
        "2026-10-20T09:00:00Z",
    )
    .await;
    let cancelled = sent(
        &f,
        invoicing::chairs(&f, 1, None),
        false,
        "2026-10-21T09:00:00Z",
    )
    .await;
    sqlx::query("UPDATE invoices SET status = 'cancelled' WHERE id = $1::uuid")
        .bind(&cancelled)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let _later = sent(
        &f,
        invoicing::chairs(&f, 1, None),
        false,
        "2026-11-01T00:00:00Z",
    )
    .await;
    let _draft = invoicing::draft(&f, invoicing::chairs(&f, 1, None), false).await;

    let (s, res) = export(&f, OCT).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(d["invoices"], 3, "{d}");
    assert_eq!(d["lines"], 3);
    assert_eq!(d["name"], "invoices-2026-10-01-to-2026-10-31.csv");
    assert_eq!(
        d["currencies"],
        json!([{ "currency": "EUR", "decimals": 2, "excl": 2000, "vat": 420, "total": 2420 }]),
        "20.00 + 10.00 (cancelled) − 10.00 (credit) excl"
    );
    assert!(
        d["url"]
            .as_str()
            .unwrap()
            .contains(d["file_id"].as_str().unwrap()),
        "{d}"
    );
    assert!(d["expires_at"].is_string());

    let key: String = sqlx::query_scalar(
        "SELECT key FROM files WHERE id = $1::uuid AND expires_at > now() + interval '23 hours'",
    )
    .bind(d["file_id"].as_str().unwrap())
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    let store = f.delivery_state.files.as_ref().unwrap();
    let (bytes, content_type) = store.read(&key).unwrap();
    assert!(content_type.starts_with("text/csv"), "{content_type}");
    let csv = String::from_utf8(bytes).unwrap();
    let csv = csv.strip_prefix('\u{feff}').expect("BOM for Excel");
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(
        rows[0],
        "invoice_number,invoice_date,due_date,type,status,customer,customer_vat_number,currency,\
         line,quantity,unit_price_excl,vat_rate,discount_pct,line_excl,line_vat,line_total,\
         invoice_excl,invoice_vat,invoice_total"
    );
    assert_eq!(rows.len(), 4, "{csv}");
    let number = |id: &str| {
        let pool = f.api.pool.clone();
        let id = id.to_owned();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT invoice_number FROM invoices WHERE id = $1::uuid")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let n = number(&first).await;
    assert!(
        rows[1].starts_with(&format!("INV-{n},2026-10-05,")),
        "{}",
        rows[1]
    );
    assert!(
        rows[1].contains(",invoice,pending,\"'=HYPERLINK(\"\"x\"\"), Ltd\",NL001,EUR,Chair,2,10.00,21,0,20.00,4.20,24.20,20.00,4.20,24.20"),
        "{}",
        rows[1]
    );
    let n = number(&credit).await;
    assert!(
        rows[2].starts_with(&format!("CRE-{n},2026-10-20,")),
        "{}",
        rows[2]
    );
    assert!(rows[2].contains(",credit_note,"), "{}", rows[2]);
    assert!(
        rows[2].ends_with(",-10.00,-2.10,-12.10,-10.00,-2.10,-12.10"),
        "{}",
        rows[2]
    );
    assert!(
        rows[3].contains(",cancelled,"),
        "kept for the number sequence: {}",
        rows[3]
    );

    // Nothing in range: no file stored.
    let (s, res) = export(&f, "from=2001-01-01&to=2001-02-01").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(res["data"]["invoices"], 0);
    assert_eq!(res["data"]["url"], Value::Null);
}

#[sqlx::test]
async fn invoice_exports_expire_and_stay_in_their_workspace(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    sent(
        &f,
        invoicing::chairs(&f, 1, None),
        false,
        "2026-10-05T09:00:00Z",
    )
    .await;
    let (_, res) = export(&f, OCT).await;
    let file = res["data"]["file_id"].as_str().unwrap().to_owned();

    let eve = f.api.as_new_owner("eve@example.com", "evil").await;
    let (s, _) = get(&f.api, &eve.owner, &eve.ws, &format!("/files/{file}")).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "another workspace can't fetch it");
    let (s, _) = get(&f.api, &f.api.owner, &f.api.ws, &format!("/files/{file}")).await;
    assert_eq!(s, StatusCode::OK);

    let key: String = sqlx::query_scalar("SELECT key FROM files WHERE id = $1::uuid")
        .bind(&file)
        .fetch_one(&f.api.pool)
        .await
        .unwrap();
    let store = f.delivery_state.files.clone().unwrap();
    let swept = genesy_360_api::cleanup::sweep_files(&f.api.pool, &store)
        .await
        .unwrap();
    assert_eq!(swept, 0, "a fresh export stays");
    sqlx::query("UPDATE files SET expires_at = now() - interval '1 minute' WHERE id = $1::uuid")
        .bind(&file)
        .execute(&f.api.pool)
        .await
        .unwrap();
    let swept = genesy_360_api::cleanup::sweep_files(&f.api.pool, &store)
        .await
        .unwrap();
    assert_eq!(swept, 1);
    assert!(store.read(&key).is_none(), "object removed");
    let (s, _) = get(&f.api, &f.api.owner, &f.api.ws, &format!("/files/{file}")).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "pruned");
}

#[sqlx::test]
async fn invoice_export_needs_the_file_store(pool: PgPool) {
    let f = invoicing::setup_with(pool, false).await;
    let id = invoicing::draft(&f, invoicing::chairs(&f, 1, None), false).await;
    let (s, res) = invoicing::send(&f, &id, invoicing::message(&f, "manual")).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let now = chrono::Utc::now();
    let q = format!(
        "from={}&to={}",
        (now - chrono::Duration::days(1)).format("%Y-%m-%d"),
        (now + chrono::Duration::days(2)).format("%Y-%m-%d")
    );
    let (s, _) = export(&f, &q).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
}

/// GET /sales/daily: one row per local day per currency sold in range, zero-filled.
#[sqlx::test]
async fn sales_daily_buckets_local_days_per_currency(pool: PgPool) {
    let w = world(pool).await;
    invoicing::settings(
        &w.api,
        json!({ "general": { "timezone": "Africa/Dar_es_Salaam" } }),
    )
    .await;
    let eur = |net, vat| [L(&w.honey, None, "1", net, vat)];
    // 22:30Z = 01:30 next day in Dar (UTC+3); 20:00Z stays on 1 Oct.
    order(
        &w,
        "2026-10-01T22:30:00Z",
        None,
        &w.eur,
        "completed",
        "office",
        &eur(1000, 90),
    )
    .await;
    order(
        &w,
        "2026-10-01T20:00:00Z",
        None,
        &w.eur,
        "open",
        "till",
        &eur(500, 45),
    )
    .await;
    order(
        &w,
        "2026-10-01T20:30:00Z",
        None,
        &w.eur,
        "open",
        "till",
        &eur(200, 18),
    )
    .await;
    order(
        &w,
        "2026-10-02T10:00:00Z",
        None,
        &w.eur,
        "draft",
        "office",
        &eur(9999, 0),
    )
    .await;
    order(
        &w,
        "2026-10-03T10:00:00Z",
        None,
        &w.usd,
        "completed",
        "office",
        &eur(700, 0),
    )
    .await;
    order(
        &w,
        "2026-10-04T10:00:00Z",
        None,
        &w.eur,
        "completed",
        "office",
        &eur(1, 0),
    )
    .await;

    let d = owner_get(&w, "/sales/daily?from=2026-10-01&to=2026-10-04").await;
    let day = |date: &str, gross: i64, net: i64, orders: i64| json!({ "date": date, "gross": gross, "net": net, "orders": orders });
    assert_eq!(
        d,
        json!({
            "time_zone": "Africa/Dar_es_Salaam", "from": "2026-10-01", "to": "2026-10-04",
            "currencies": [
                { "currency": "EUR", "decimals": 2, "days": [
                    day("2026-10-01", 763, 700, 2), day("2026-10-02", 1090, 1000, 1), day("2026-10-03", 0, 0, 0)] },
                { "currency": "USD", "decimals": 2, "days": [
                    day("2026-10-01", 0, 0, 0), day("2026-10-02", 0, 0, 0), day("2026-10-03", 700, 700, 1)] },
            ]
        })
    );
    let till = owner_get(&w, "/sales/daily?from=2026-10-01&to=2026-10-02&source=till").await;
    assert_eq!(
        till["currencies"][0]["days"],
        json!([day("2026-10-01", 763, 700, 2)])
    );
    let none = owner_get(&w, "/sales/daily?from=2025-01-01&to=2025-01-03").await;
    assert_eq!(none["currencies"], json!([]));
}

#[sqlx::test]
async fn sales_daily_refuses_bad_ranges_and_other_workspaces(pool: PgPool) {
    let w = world(pool).await;
    let api = &w.api;
    for (q, field) in [
        ("to=2026-10-02", "from"),
        ("from=2026-10-01T00:00:00Z&to=2026-10-02", "from"),
        ("from=2026-10-01&to=2026-10-01", "to"),
        ("from=2026-10-01&to=2027-01-02", "to"),
        ("from=2026-10-01&to=2026-10-02&source=web", "source"),
    ] {
        let (s, res) = get(api, &api.owner, &api.ws, &format!("/sales/daily?{q}")).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {res}");
        assert!(res["errors"][field].is_array(), "{q}: {res}");
    }
    // 92 days is the cap.
    let (s, _) = get(
        api,
        &api.owner,
        &api.ws,
        "/sales/daily?from=2026-10-01&to=2027-01-01",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let q = "/sales/daily?from=2026-10-01&to=2026-10-02";
    let clerk =
        common::member_with(&api.app, &api.pool, &api.ws, "clerk@x.io", &["stock.list"]).await;
    assert_eq!(get(api, &clerk, &api.ws, q).await.0, StatusCode::FORBIDDEN);
    let other = api.as_new_owner("eve@x.io", "evil").await;
    assert_eq!(
        get(api, &other.owner, &api.ws, q).await.0,
        StatusCode::NOT_FOUND
    );
    let (s, _) = common::send(
        &api.app,
        Method::GET,
        &format!("/v1/workspace/{}{q}", api.ws),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}
