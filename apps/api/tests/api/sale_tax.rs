//! One verified tax calculation path in active books (Flow ERP M6-A, masterplan §13 TAX):
//! sales tax per line and per component from the covering verified profile, quote = commit,
//! client legacy rates ignored, unsupported scope refused, credits reverse each component.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    common::tax::{profile, profile_in, replace_sale_profiles},
    till_sales::{cash, line, post, sale, world},
};

/// Three honey lines whose per-line, per-component tax (10.5 % + 2.5 %) is 65 + 16 = 81,
/// where one 13 % rate on the summed 615 would round to 80.
fn three(w: &crate::till_sales::W) -> Value {
    json!([
        line(&w.honey, "1", 105),
        line(&w.honey, "1", 205),
        line(&w.honey, "1", 305)
    ])
}

const SPLIT: [(&str, &str); 2] = [("STATE", "10.5"), ("CITY", "2.5")];

async fn quote(w: &crate::till_sales::W, lines: Value) -> (StatusCode, Value) {
    w.api
        .call(
            Method::POST,
            "/sales/quote",
            Some(json!({ "lines": lines, "payment": { "method": "card" } })),
            &w.t1,
        )
        .await
}

async fn order_totals(pool: &PgPool, ws: &str) -> (i64, i64, i64) {
    sqlx::query_as("SELECT subtotal, vat, total FROM orders WHERE workspace_id = $1::uuid")
        .bind(ws)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn unsupported(status: StatusCode, res: &Value) {
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["code"], "unsupported_active_erp_operation", "{res}");
}

#[sqlx::test]
async fn till_quote_equals_sale_with_exact_line_and_component_rounding(pool: PgPool) {
    let w = world(pool).await;
    crate::till_sales::activate_with_stock(&w).await;
    let profile = replace_sale_profiles(&w.api.pool, &w.api.ws, &SPLIT).await;
    let (status, q) = quote(&w, three(&w)).await;
    assert_eq!(status, StatusCode::OK, "{q}");
    let q = &q["data"];
    assert_eq!(
        (&q["subtotal"], &q["vat"], &q["total"]),
        (&json!(615), &json!(81), &json!(696))
    );
    let (status, res) = post(&w, &w.t1, &sale("r-tax", three(&w), cash(1000))).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(order_totals(&w.api.pool, &w.api.ws).await, (615, 81, 696));
    // Snapshot per line and component; legacy fields are projections of it.
    let snap: Vec<(i32, String, i64, i64, Uuid)> = sqlx::query_as(
        "SELECT l.position, t.code, t.base, t.tax, t.profile_id FROM order_line_taxes t
         JOIN order_lines l ON l.id = t.order_line_id
         WHERE t.workspace_id = $1::uuid ORDER BY l.position, t.line_no",
    )
    .bind(&w.api.ws)
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    let want = [
        (1, "STATE", 105, 11),
        (1, "CITY", 105, 3),
        (2, "STATE", 205, 22),
        (2, "CITY", 205, 5),
        (3, "STATE", 305, 32),
        (3, "CITY", 305, 8),
    ];
    assert_eq!(snap.len(), want.len());
    for (got, want) in snap.iter().zip(want) {
        assert_eq!(
            (got.0, got.1.as_str(), got.2, got.3, got.4),
            (want.0, want.1, want.2, want.3, profile)
        );
    }
    let legacy: Vec<(String, i64)> = sqlx::query_as(
        "SELECT trim_scale(vat_rate)::text, line_vat FROM order_lines
         WHERE workspace_id = $1::uuid ORDER BY position",
    )
    .bind(&w.api.ws)
    .fetch_all(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(
        legacy,
        [("13".into(), 14), ("13".into(), 27), ("13".into(), 40)]
    );
    assert_eq!(
        crate::inventory_valuation::role_balance(&w.api.pool, &w.api.ws, "tax_payable").await,
        -81
    );
}

#[sqlx::test]
async fn setup_books_keep_tax_per_rate(pool: PgPool) {
    let w = world(pool).await;
    // Honey 9 %: one rate on 615 = 55 (per line it would be 9 + 18 + 27 = 54).
    let (status, q) = quote(&w, three(&w)).await;
    assert_eq!(status, StatusCode::OK, "{q}");
    assert_eq!(
        (&q["data"]["vat"], &q["data"]["total"]),
        (&json!(55), &json!(670))
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM order_line_taxes")
            .fetch_one(&w.api.pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
async fn uncovered_or_ambiguous_scope_is_refused(pool: PgPool) {
    let w = world(pool).await;
    crate::till_sales::activate_with_stock(&w).await;
    let (status, res) = post(
        &w,
        &w.t1,
        &sale("r-ok", json!([line(&w.honey, "1", 1250)]), cash(2000)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let order = res["data"]["order_id"].as_str().unwrap().to_owned();
    // Profiles retired (superseded) for one of an unused category: nothing covers the lines.
    let unused: Uuid = sqlx::query_scalar(
        "INSERT INTO product_categories (workspace_id, name) VALUES ($1::uuid, 'Unused') RETURNING id",
    )
    .bind(&w.api.ws)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    let elsewhere = profile(&w.api.pool, &w.api.ws, &[unused], &[("VAT", "21")]).await;
    sqlx::query(
        "UPDATE tax_profiles SET superseded_by_id = $2 WHERE workspace_id = $1::uuid AND id <> $2",
    )
    .bind(&w.api.ws)
    .bind(elsewhere)
    .execute(&w.api.pool)
    .await
    .unwrap();
    let (status, res) = quote(&w, three(&w)).await;
    unsupported(status, &res);
    let (status, res) = post(&w, &w.t1, &sale("r-none", three(&w), cash(1000))).await;
    unsupported(status, &res);
    // Another workspace's profile never covers this one; this one's order is 404 to it.
    let other = w.api.as_new_owner("other@tax.test", "other").await;
    profile(&w.api.pool, &other.ws, &[], &[("VAT", "21")]).await;
    let (status, res) = quote(&w, three(&w)).await;
    unsupported(status, &res);
    let (status, _) = other
        .with_base("/orders")
        .call(Method::GET, &format!("/{order}"), None, &other.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Two jurisdictions covering the same line: refused, never guessed.
    profile_in(&w.api.pool, &w.api.ws, "BE", &[], &[("VAT", "21")]).await;
    profile(&w.api.pool, &w.api.ws, &[], &[("VAT", "9")]).await;
    let (status, res) = quote(&w, three(&w)).await;
    unsupported(status, &res);
    assert!(
        res["message"].as_str().unwrap().contains("jurisdiction"),
        "{res}"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM orders")
            .fetch_one(&w.api.pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test]
async fn invoice_ignores_a_client_rate_and_its_quote_is_its_commit(pool: PgPool) {
    let f = crate::sales_billing::active(pool).await;
    replace_sale_profiles(&f.api.pool, &f.api.ws, &SPLIT).await;
    let items: Vec<Value> = ["1.05", "2.05", "3.05"]
        .iter()
        .enumerate()
        .map(|(i, p)| {
            json!({ "id": i + 1, "title": "Chair", "product_id": f.product,
                               "quantity": 1, "price": p, "vat": "99" })
        })
        .collect();
    let (status, q) = f
        .api
        .with_base("/calculate-items")
        .call(
            Method::POST,
            "",
            Some(json!({ "relation_id": f.rel, "items": items })),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{q}");
    let q = &q["data"];
    assert_eq!(
        (&q["total_excl"], &q["total_vat"], &q["total"]),
        (&json!(6.15), &json!(0.81), &json!(6.96))
    );
    let lines: Vec<Value> = items
        .iter()
        .map(|i| {
            let mut i = i.clone();
            i.as_object_mut().unwrap().remove("id");
            i
        })
        .collect();
    let id = crate::common::invoicing::draft(&f, json!(lines), false).await;
    let stored: (i64, i64) =
        sqlx::query_as("SELECT subtotal, total FROM invoices WHERE id = $1::uuid")
            .bind(&id)
            .fetch_one(&f.api.pool)
            .await
            .unwrap();
    assert_eq!(stored, (615, 696));
    let rates: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT trim_scale(vat)::text FROM items WHERE invoice_id = $1::uuid",
    )
    .bind(&id)
    .fetch_all(&f.api.pool)
    .await
    .unwrap();
    assert_eq!(rates, ["13"]);
    let comps: i64 =
        sqlx::query_scalar("SELECT sum(tax)::int8 FROM item_taxes WHERE workspace_id = $1::uuid")
            .bind(&f.api.ws)
            .fetch_one(&f.api.pool)
            .await
            .unwrap();
    assert_eq!(comps, 81);
}

#[sqlx::test]
async fn credits_reverse_each_component_in_thirds(pool: PgPool) {
    let (s, _, _, _, _) = crate::sales_returns_schema::shipped(pool).await;
    replace_sale_profiles(s.pool(), s.ws(), &SPLIT).await;
    let (o, l) = s.open_order("3").await;
    s.ship(&o, &[(&l, "3")]).await;
    let (rec, line): (Uuid, Uuid) = sqlx::query_as(
        "SELECT r.id, l.id FROM sales_recognitions r
         JOIN sales_recognition_lines l ON l.recognition_id = r.id WHERE r.order_id = $1::uuid",
    )
    .bind(&o)
    .fetch_one(s.pool())
    .await
    .unwrap();
    for _ in 0..3 {
        let p = crate::sales_returns::credit(&s, rec, &[(line, "1")]).await;
        // Each credit's tax is the sum of its components.
        let parts: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(t.tax), 0)::int8 FROM sales_return_line_taxes t
             JOIN sales_return_lines x ON x.id = t.return_line_id
             WHERE x.return_id = $1::uuid",
        )
        .bind(p["id"].as_str().unwrap())
        .fetch_one(s.pool())
        .await
        .unwrap();
        let tax: rust_decimal::Decimal = p["tax"].as_str().unwrap().parse().unwrap();
        assert_eq!(
            rust_decimal::Decimal::from(parts),
            tax * rust_decimal::Decimal::ONE_HUNDRED
        );
    }
    // Three thirds reverse every component exactly.
    let per: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT o.code, o.tax, (SELECT sum(t.tax)::int8 FROM sales_return_line_taxes t
                               WHERE t.workspace_id = o.workspace_id AND t.code = o.code)
         FROM order_line_taxes o WHERE o.order_line_id = $1::uuid ORDER BY o.line_no",
    )
    .bind(&l)
    .fetch_all(s.pool())
    .await
    .unwrap();
    assert_eq!(per.len(), 2);
    for (code, sold, credited) in per {
        assert_eq!(sold, credited, "{code}");
    }
}
