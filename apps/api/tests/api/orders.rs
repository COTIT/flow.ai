//! Office orders: drafts priced from the catalog in minor units, the status machine and the
//! stock each status holds or takes.

use crate::common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{Api, invoicing::stock, member_with, send_raw};
use serde_json::{Value, json};
use sqlx::PgPool;

struct W {
    api: Api,
    main: String,
    honey: String,
    wax: String,
}

/// Acme: a main warehouse, honey (€12.50 excl, 9 % VAT, 10 in stock) and wax (€3 excl, 21 %,
/// 10 in stock).
async fn world(pool: PgPool) -> W {
    let api = Api::new(pool, "/orders").await;
    let vat = |rate: &'static str| {
        let api = api.with_base("/settings/vat-groups");
        async move {
            api.create(
                json!({ "name": rate, "vat_amount": rate, "country": "NL" }),
                "vat_group_id",
            )
            .await
        }
    };
    let (low, high) = (vat("9").await, vat("21").await);
    let main = api
        .with_base("/warehouses")
        .create(
            json!({ "name": "Main", "default_warehouse": true }),
            "warehouse_id",
        )
        .await;
    let products = api.with_base("/products");
    let honey = products
        .create(
            json!({ "name": "Honey", "price": "12.50", "vat_group_id": low }),
            "product_id",
        )
        .await;
    let wax = products
        .create(
            json!({ "name": "Wax", "price": "3", "vat_group_id": high }),
            "product_id",
        )
        .await;
    stock(&api, &honey, &main, 10).await;
    stock(&api, &wax, &main, 10).await;
    W {
        api,
        main,
        honey,
        wax,
    }
}

async fn call(w: &W, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    w.api.call(method, path, body, &w.api.owner).await
}

fn line(product: &str, q: &str) -> Value {
    json!({ "product_id": product, "quantity": q })
}

async fn draft(w: &W, lines: Value) -> String {
    let (s, res) = call(w, Method::POST, "", Some(json!({ "lines": lines }))).await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    res["data"]["order_id"].as_str().unwrap().to_owned()
}

async fn act(w: &W, id: &str, action: &str) -> (StatusCode, Value) {
    call(w, Method::POST, &format!("/{id}/{action}"), None).await
}

/// (level, reserved) of a product at the main warehouse.
async fn level(w: &W, product: &str) -> (String, String) {
    sqlx::query_as(
        "SELECT stock_lvl::text, reserved::text FROM stocks
         WHERE product_id = $1::uuid AND warehouse_id = $2::uuid AND bin_id IS NULL",
    )
    .bind(product)
    .bind(&w.main)
    .fetch_one(&w.api.pool)
    .await
    .unwrap()
}

/// Σ reservation rows of an order.
async fn held(w: &W, order: &str) -> String {
    sqlx::query_scalar(
        "SELECT coalesce(sum(quantity), 0)::numeric(19,2)::text FROM stock_reservations
         WHERE related_domain = 'order' AND related_id = $1::uuid",
    )
    .bind(order)
    .fetch_one(&w.api.pool)
    .await
    .unwrap()
}

async fn pick_all(w: &W, id: &str) {
    let (_, o) = call(w, Method::GET, &format!("/{id}"), None).await;
    let lines: Vec<Value> = o["data"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| json!({ "line_id": l["id"], "quantity": l["quantity"] }))
        .collect();
    let (s, res) = call(
        w,
        Method::POST,
        &format!("/{id}/pick"),
        Some(json!({ "lines": lines })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn a_draft_is_priced_in_minor_units_with_vat_per_rate(pool: PgPool) {
    let w = world(pool).await;
    let id = draft(&w, json!([line(&w.honey, "2"), line(&w.wax, "1.5")])).await;
    let (s, o) = call(&w, Method::GET, &format!("/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    let o = &o["data"];
    assert_eq!(o["number"], "ORD-0001");
    assert_eq!(o["status"], "draft");
    assert_eq!(o["source"], "office");
    assert_eq!(o["warehouse_id"], w.main.as_str());
    // 2 × 12.50 = 25.00 at 9 %; 1.5 × 3 = 4.50 at 21 %.
    assert_eq!(
        (&o["subtotal"], &o["vat"], &o["total"]),
        (&json!(2950), &json!(320), &json!(3270))
    );
    assert_eq!(
        o["summary"]["vats"],
        json!([{ "rate": "9", "base": 2500, "vat": 225 }, { "rate": "21", "base": 450, "vat": 95 }])
    );
    let l = &o["lines"][0];
    assert_eq!(
        (&l["unit_price"], &l["line_excl"], &l["line_vat"]),
        (&json!(1250), &json!(2500), &json!(225))
    );
    assert_eq!(
        (&l["title"], &l["quantity"], &l["tracked"]),
        (&json!("Honey"), &json!("2.00"), &json!(true))
    );
    let second = draft(&w, json!([line(&w.honey, "1")])).await;
    let (_, o) = call(&w, Method::GET, &format!("/{second}"), None).await;
    assert_eq!(o["data"]["number"], "ORD-0002");
    assert_eq!(
        level(&w, &w.honey).await,
        ("10.00".into(), "0.00".into()),
        "a draft holds nothing"
    );
}

#[sqlx::test]
async fn office_may_set_price_discount_and_title(pool: PgPool) {
    let w = world(pool).await;
    let body = json!([{ "product_id": w.honey, "quantity": 1, "unit_price": 1000,
                        "discount_percentage": "10", "title": "Honey (gift)" }]);
    let id = draft(&w, body).await;
    let (_, o) = call(&w, Method::GET, &format!("/{id}"), None).await;
    let l = &o["data"]["lines"][0];
    assert_eq!(
        (&l["unit_price"], &l["line_excl"], &l["title"]),
        (&json!(1000), &json!(900), &json!("Honey (gift)"))
    );
    assert_eq!(o["data"]["total"], 981);
}

#[sqlx::test]
async fn bad_lines_are_refused(pool: PgPool) {
    let w = world(pool).await;
    let other = w.api.as_new_owner("bob@example.com", "bobs").await;
    let theirs = other
        .with_base("/products")
        .create(json!({ "name": "Theirs", "price": 1 }), "product_id")
        .await;
    for (lines, key) in [
        (json!([]), "lines"),
        (json!([line(&w.honey, "0")]), "lines.0.quantity"),
        (json!([line(&w.honey, "1.001")]), "lines.0.quantity"),
        (json!([line(&w.honey, "100001")]), "lines.0.quantity"),
        (json!([line(&theirs, "1")]), "lines.0.product_id"),
        (json!([line("nope", "1")]), "lines.0.product_id"),
        (
            json!([{ "product_id": w.honey, "product_variant_id": theirs, "quantity": 1 }]),
            "lines.0.product_variant_id",
        ),
        (Value::Array(vec![line(&w.honey, "1"); 51]), "lines"),
    ] {
        let (s, res) = call(&w, Method::POST, "", Some(json!({ "lines": lines }))).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{key}: {res}");
        assert!(res["errors"][key].is_array(), "{key}: {res}");
    }
}

#[sqlx::test]
async fn confirm_holds_complete_takes_cancel_gives_back(pool: PgPool) {
    let w = world(pool).await;
    let a = draft(&w, json!([line(&w.honey, "3"), line(&w.wax, "1")])).await;
    let (s, o) = act(&w, &a, "confirm").await;
    assert_eq!((s, &o["data"]["status"]), (StatusCode::OK, &json!("open")));
    assert_eq!(level(&w, &w.honey).await, ("10.00".into(), "3.00".into()));
    assert_eq!(held(&w, &a).await, "4.00");

    let (s, res) = act(&w, &a, "complete").await;
    assert_eq!(
        (s, &res["message"]),
        (
            StatusCode::CONFLICT,
            &json!("Pick every stock line in full before completing.")
        )
    );
    pick_all(&w, &a).await;
    let (s, o) = act(&w, &a, "complete").await;
    assert_eq!(
        (s, &o["data"]["status"]),
        (StatusCode::OK, &json!("completed"))
    );
    assert_eq!(o["data"]["lines"][0]["deducted"], "3.00");
    assert_eq!(level(&w, &w.honey).await, ("7.00".into(), "0.00".into()));
    assert_eq!(held(&w, &a).await, "0.00");
    let ledger: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stock_movements WHERE related_domain = 'order' AND related_id = $1::uuid",
    )
    .bind(&a)
    .fetch_one(&w.api.pool)
    .await
    .unwrap();
    assert_eq!(ledger, 2);

    let b = draft(&w, json!([line(&w.honey, "2")])).await;
    act(&w, &b, "confirm").await;
    assert_eq!(level(&w, &w.honey).await, ("7.00".into(), "2.00".into()));
    let (s, o) = act(&w, &b, "cancel").await;
    assert_eq!(
        (s, &o["data"]["status"]),
        (StatusCode::OK, &json!("cancelled"))
    );
    assert!(o["data"]["cancelled_at"].is_string());
    assert_eq!(level(&w, &w.honey).await, ("7.00".into(), "0.00".into()));
    assert_eq!(held(&w, &b).await, "0.00");
}

#[sqlx::test]
async fn confirm_refuses_more_than_available_and_holds_nothing(pool: PgPool) {
    let w = world(pool).await;
    let a = draft(&w, json!([line(&w.wax, "1"), line(&w.honey, "8")])).await;
    act(&w, &a, "confirm").await;
    let b = draft(&w, json!([line(&w.wax, "1"), line(&w.honey, "3")])).await;
    let (s, res) = act(&w, &b, "confirm").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["lines.1.quantity"][0],
        "Not enough stock: 2 available."
    );
    assert_eq!(held(&w, &b).await, "0.00");
    assert_eq!(level(&w, &w.wax).await, ("10.00".into(), "1.00".into()));
    let (_, o) = call(&w, Method::GET, &format!("/{b}"), None).await;
    assert_eq!(o["data"]["status"], "draft");
}

#[sqlx::test]
async fn the_status_machine_refuses_wrong_moves(pool: PgPool) {
    let w = world(pool).await;
    let id = draft(&w, json!([line(&w.honey, "1")])).await;
    for action in ["complete", "pick"] {
        let body = (action == "pick").then(|| json!({ "lines": [] }));
        let (s, _) = call(&w, Method::POST, &format!("/{id}/{action}"), body).await;
        assert!(
            s == StatusCode::CONFLICT || s == StatusCode::UNPROCESSABLE_ENTITY,
            "{action}: {s}"
        );
    }
    act(&w, &id, "confirm").await;
    let (s, _) = act(&w, &id, "confirm").await;
    assert_eq!(s, StatusCode::CONFLICT);
    let patch = json!({ "lines": [line(&w.honey, "2")] });
    let (s, _) = call(&w, Method::PATCH, &format!("/{id}"), Some(patch)).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = call(&w, Method::DELETE, &format!("/{id}"), None).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, o) = call(
        &w,
        Method::PATCH,
        &format!("/{id}"),
        Some(json!({ "reference": "PO-7" })),
    )
    .await;
    assert_eq!(
        (s, &o["data"]["reference"]),
        (StatusCode::OK, &json!("PO-7"))
    );
    pick_all(&w, &id).await;
    act(&w, &id, "complete").await;
    for action in ["cancel", "confirm", "complete"] {
        let (s, _) = act(&w, &id, action).await;
        assert_eq!(s, StatusCode::CONFLICT, "{action}");
    }
}

#[sqlx::test]
async fn a_paid_order_needs_a_refund_before_cancel(pool: PgPool) {
    let w = world(pool).await;
    let id = draft(&w, json!([line(&w.honey, "1")])).await;
    act(&w, &id, "confirm").await;
    // Paid online (the payment suites drive this through the provider).
    sqlx::query(
        "WITH o AS (UPDATE orders SET paid_at = now() WHERE id = $1::uuid
                    RETURNING workspace_id, id, total)
         INSERT INTO order_payments (workspace_id, order_id, kind, provider, provider_id, amount,
                                     currency, status)
         SELECT workspace_id, id, 'payment', 'fake', 'fk_1', total, 'EUR', 'paid' FROM o",
    )
    .bind(&id)
    .execute(&w.api.pool)
    .await
    .unwrap();
    let (s, res) = act(&w, &id, "cancel").await;
    assert_eq!(
        (s, &res["message"]),
        (StatusCode::CONFLICT, &json!("refund_required"))
    );
    assert_eq!(held(&w, &id).await, "1.00");
}

#[sqlx::test]
async fn drafts_change_and_delete(pool: PgPool) {
    let w = world(pool).await;
    let id = draft(&w, json!([line(&w.honey, "1")])).await;
    let patch = json!({ "lines": [line(&w.wax, "2")], "note": "back door" });
    let (s, o) = call(&w, Method::PATCH, &format!("/{id}"), Some(patch)).await;
    assert_eq!(s, StatusCode::OK, "{o}");
    assert_eq!(
        (&o["data"]["total"], &o["data"]["note"]),
        (&json!(726), &json!("back door"))
    );
    assert_eq!(o["data"]["lines"].as_array().unwrap().len(), 1);
    let (s, _) = call(&w, Method::DELETE, &format!("/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(&w, Method::GET, &format!("/{id}"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn list_pages_and_filters(pool: PgPool) {
    let w = world(pool).await;
    let a = draft(&w, json!([line(&w.honey, "1")])).await;
    draft(&w, json!([line(&w.honey, "1")])).await;
    act(&w, &a, "confirm").await;
    let (s, res) = call(&w, Method::GET, "?status=open", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(res["data"].as_array().unwrap().len(), 1);
    assert_eq!(res["data"][0]["id"], a.as_str());
    assert_eq!(res["meta"]["total"], 1);
    let (s, _) = call(&w, Method::GET, "?status=nope", None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

/// Ids on a list page, in order.
fn ids(res: &Value) -> Vec<String> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap().to_owned())
        .collect()
}

async fn created_at(w: &W, id: &str, at: &str) {
    sqlx::query("UPDATE orders SET created_at = $2::timestamptz WHERE id = $1::uuid")
        .bind(id)
        .bind(at)
        .execute(&w.api.pool)
        .await
        .unwrap();
}

/// `created_from` inclusive, `created_to` exclusive; a plain date is local midnight in the
/// workspace `general.timezone` (Dar es Salaam, +03:00): 22:30 UTC on the 7th is the 8th there.
#[sqlx::test]
async fn list_filters_by_created_range_in_workspace_time(pool: PgPool) {
    let w = world(pool).await;
    common::invoicing::settings(
        &w.api,
        json!({ "general": { "timezone": "Africa/Dar_es_Salaam" } }),
    )
    .await;
    let early = draft(&w, json!([line(&w.honey, "1")])).await;
    let late = draft(&w, json!([line(&w.honey, "1")])).await;
    created_at(&w, &early, "2026-11-07T20:00:00Z").await;
    created_at(&w, &late, "2026-11-07T22:30:00Z").await;
    let api = &w.api;
    let list = |q: &'static str| api.list(q);

    let day = list("?created_from=2026-11-08&created_to=2026-11-09").await;
    assert_eq!(ids(&day), [late.as_str()]);
    assert_eq!(day["meta"]["total"], 1);
    assert_eq!(ids(&list("?created_to=2026-11-08").await), [early.as_str()]);
    assert_eq!(
        ids(&list("?created_from=2026-11-07T22:30:00Z").await),
        [late.as_str()],
        "from is inclusive"
    );
    assert_eq!(
        ids(&list("?created_to=2026-11-07T22:30:00Z").await),
        [early.as_str()],
        "to is exclusive"
    );
    assert_eq!(
        ids(&list("?created_from=2026-11-07T23:00:00%2B01:00&status=draft").await),
        [late.as_str()],
        "offset honoured, combines with status"
    );

    for (q, field, msg) in [
        (
            "?created_from=yesterday",
            "created_from",
            "The created from must be an RFC 3339 time or a YYYY-MM-DD date.",
        ),
        (
            "?created_to=2026-13-01",
            "created_to",
            "The created to must be an RFC 3339 time or a YYYY-MM-DD date.",
        ),
        (
            "?created_from=2026-11-09&created_to=2026-11-08",
            "created_to",
            "The created to must be after created from.",
        ),
    ] {
        let (s, res) = call(&w, Method::GET, q, None).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {res}");
        assert_eq!(res["errors"][field][0], msg, "{q}");
    }

    // A time zone Postgres doesn't know falls back to UTC: 22:30 UTC stays on the 7th.
    common::invoicing::settings(&w.api, json!({ "general": { "timezone": "Mars/Olympus" } })).await;
    let utc = list("?created_from=2026-11-07&created_to=2026-11-08").await;
    assert_eq!(ids(&utc), [late.clone(), early.clone()]);

    // Another workspace sees none of Ada's orders in the same range.
    let bob = w.api.as_new_owner("bob@example.com", "bobs").await;
    let theirs = bob
        .list("?created_from=2026-11-01&created_to=2026-11-30")
        .await;
    assert_eq!(theirs["meta"]["total"], 0);
    assert!(ids(&theirs).is_empty());
}

#[sqlx::test]
async fn idempotency_key_creates_once(pool: PgPool) {
    let w = world(pool).await;
    let uri = format!("/v1/workspace/{}/orders", w.api.ws);
    let body = json!({ "lines": [line(&w.honey, "1")] });
    let first = send_raw(
        &w.api.app,
        Method::POST,
        &uri,
        Some(body.clone()),
        Some(&w.api.owner),
        &[("idempotency-key", "k1")],
    )
    .await;
    let again = send_raw(
        &w.api.app,
        Method::POST,
        &uri,
        Some(body),
        Some(&w.api.owner),
        &[("idempotency-key", "k1")],
    )
    .await;
    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(first, again);
    let other = json!({ "lines": [line(&w.honey, "2")] });
    let (s, _) = send_raw(
        &w.api.app,
        Method::POST,
        &uri,
        Some(other),
        Some(&w.api.owner),
        &[("idempotency-key", "k1")],
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM orders")
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn order_created_is_sent_without_customer_details(pool: PgPool) {
    let w = world(pool).await;
    let (s, res) = w
        .api
        .with_base("/webhook-endpoints")
        .call(
            Method::POST,
            "",
            Some(json!({ "url": "https://hooks.example.com/x", "events": ["order.created"] })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let id = draft(&w, json!([line(&w.honey, "1")])).await;
    let payload: Value =
        sqlx::query_scalar("SELECT payload FROM webhook_events WHERE type = 'order.created'")
            .fetch_one(&w.api.pool)
            .await
            .unwrap();
    let mut keys: Vec<&str> = payload
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "currency", "number", "order_id", "source", "status", "subtotal", "total", "vat"
        ]
    );
    assert_eq!(payload["order_id"], id.as_str());
}

#[sqlx::test]
async fn other_workspaces_and_missing_permissions_are_refused(pool: PgPool) {
    let w = world(pool).await;
    let id = draft(&w, json!([line(&w.honey, "1")])).await;
    let bob = w.api.as_new_owner("bob@example.com", "bobs").await;
    // Bob's own workspace: Ada's order isn't there.
    let (s, _) = bob
        .call(Method::GET, &format!("/{id}"), None, &bob.owner)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    for action in ["confirm", "cancel", "complete"] {
        let (s, _) = bob
            .call(Method::POST, &format!("/{id}/{action}"), None, &bob.owner)
            .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{action}");
    }
    let (s, _) = bob
        .call(Method::DELETE, &format!("/{id}"), None, &bob.owner)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Ada's workspace with Bob's token: not a member.
    let (s, _) = w
        .api
        .call(Method::GET, &format!("/{id}"), None, &bob.owner)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let viewer = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "vi@example.com",
        &["order.view"],
    )
    .await;
    let (s, _) = w
        .api
        .call(Method::GET, &format!("/{id}"), None, &viewer)
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = w
        .api
        .call(Method::POST, &format!("/{id}/confirm"), None, &viewer)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(level(&w, &w.honey).await, ("10.00".into(), "0.00".into()));
}

/// 50 orders of honey + wax and 50 of wax + honey confirmed at once, then cancelled at once:
/// no deadlock, each order holds exactly its lines, all holds drop to zero.
#[sqlx::test]
async fn opposite_line_orders_confirm_and_cancel_in_parallel(pool: PgPool) {
    let w = world(pool).await;
    stock(&w.api, &w.honey, &w.main, 990).await;
    stock(&w.api, &w.wax, &w.main, 990).await;
    let mut ids = Vec::new();
    for i in 0..100 {
        let lines = if i % 2 == 0 {
            json!([line(&w.honey, "1"), line(&w.wax, "2")])
        } else {
            json!([line(&w.wax, "2"), line(&w.honey, "1")])
        };
        ids.push(draft(&w, lines).await);
    }
    let w = Arc::new(w);
    for action in ["confirm", "cancel"] {
        let tasks: Vec<_> = ids
            .iter()
            .map(|id| {
                let (w, id) = (w.clone(), id.clone());
                tokio::spawn(async move { act(&w, &id, action).await })
            })
            .collect();
        for t in tasks {
            let (s, res) = t.await.unwrap();
            assert_eq!(s, StatusCode::OK, "{action}: {res}");
        }
        if action == "confirm" {
            assert_eq!(
                level(&w, &w.honey).await,
                ("1000.00".into(), "100.00".into())
            );
            for id in &ids {
                assert_eq!(held(&w, id).await, "3.00");
            }
        }
    }
    assert_eq!(level(&w, &w.wax).await, ("1000.00".into(), "0.00".into()));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM stock_reservations")
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

async fn completed(w: &W, lines: Value) -> String {
    let id = draft(w, lines).await;
    act(w, &id, "confirm").await;
    pick_all(w, &id).await;
    let (s, res) = act(w, &id, "complete").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    id
}

async fn line_ids(w: &W, id: &str) -> Vec<String> {
    let (_, o) = call(w, Method::GET, &format!("/{id}"), None).await;
    o["data"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test]
async fn returns_restock_and_close_the_order(pool: PgPool) {
    let w = world(pool).await;
    let id = completed(&w, json!([line(&w.honey, "3"), line(&w.wax, "1")])).await;
    let l = line_ids(&w, &id).await;
    assert_eq!(level(&w, &w.honey).await, ("7.00".into(), "0.00".into()));
    let path = format!("/{id}/returns");
    let ret = |lines: Value, restock: bool| {
        call(
            &w,
            Method::POST,
            &path,
            Some(json!({ "lines": lines, "restock": restock })),
        )
    };
    let (s, o) = ret(json!([{ "line_id": l[0], "quantity": "2" }]), true).await;
    assert_eq!(
        (s, &o["data"]["status"]),
        (StatusCode::OK, &json!("partially_returned")),
        "{o}"
    );
    assert_eq!(o["data"]["lines"][0]["returned"], "2.00");
    assert_eq!(level(&w, &w.honey).await, ("9.00".into(), "0.00".into()));
    let (s, res) = ret(json!([{ "line_id": l[0], "quantity": "2" }]), true).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["lines.0.quantity"].is_array());
    // Damaged goods: back on the books, not on the shelf.
    let (s, o) = ret(
        json!([{ "line_id": l[0], "quantity": "1" }, { "line_id": l[1], "quantity": "1" }]),
        false,
    )
    .await;
    assert_eq!(
        (s, &o["data"]["status"]),
        (StatusCode::OK, &json!("returned"))
    );
    assert_eq!(level(&w, &w.honey).await, ("9.00".into(), "0.00".into()));
    let (s, _) = ret(json!([{ "line_id": l[1], "quantity": "1" }]), true).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let open = draft(&w, json!([line(&w.honey, "1")])).await;
    act(&w, &open, "confirm").await;
    let lo = line_ids(&w, &open).await;
    let (s, _) = call(
        &w,
        Method::POST,
        &format!("/{open}/returns"),
        Some(json!({ "lines": [{ "line_id": lo[0], "quantity": "1" }] })),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "only completed orders take returns"
    );
}

#[sqlx::test]
async fn an_order_becomes_one_draft_invoice_without_moving_stock(pool: PgPool) {
    let w = world(pool).await;
    let id = completed(&w, json!([line(&w.honey, "2"), line(&w.wax, "1.5")])).await;
    let (s, res) = act(&w, &id, "invoice").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["relation_id"].is_array());
    let rel = w
        .api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Shop BV", "type": "business" }),
            "relation_id",
        )
        .await;
    call(
        &w,
        Method::PATCH,
        &format!("/{id}"),
        Some(json!({ "relation_id": rel })),
    )
    .await;
    let (s, o) = act(&w, &id, "invoice").await;
    assert_eq!(s, StatusCode::OK, "{o}");
    let invoice = o["data"]["invoice_id"].as_str().unwrap().to_owned();
    let (s, inv) = w
        .api
        .with_base("/invoices")
        .call(Method::GET, &format!("/{invoice}"), None, &w.api.owner)
        .await;
    assert_eq!(s, StatusCode::OK, "{inv}");
    assert_eq!(inv["data"]["status"], "draft");
    let total: i64 = sqlx::query_scalar("SELECT total FROM invoices WHERE id = $1::uuid")
        .bind(&invoice)
        .fetch_one(&w.api.pool)
        .await
        .unwrap();
    assert_eq!(json!(total), o["data"]["total"], "same engine, same total");
    assert_eq!(inv["data"]["items"].as_array().unwrap().len(), 2);
    let (s, _) = act(&w, &id, "invoice").await;
    assert_eq!(s, StatusCode::CONFLICT, "once");
    assert_eq!(level(&w, &w.honey).await, ("8.00".into(), "0.00".into()));
    let fresh = draft(&w, json!([line(&w.honey, "1")])).await;
    let (s, _) = act(&w, &fresh, "invoice").await;
    assert_eq!(s, StatusCode::CONFLICT, "drafts aren't invoiced");
}

const ACTIONS: [&str; 7] = [
    "confirm", "pick", "complete", "cancel", "return", "refund", "invoice",
];

/// Try `action` with a well-formed body, so a refusal comes from the order's state.
async fn try_action(w: &W, id: &str, action: &str) -> StatusCode {
    let lines = line_ids(w, id).await;
    let one = |q: &str| json!({ "lines": [{ "line_id": lines[0], "quantity": q }] });
    let (path, body) = match action {
        "pick" => ("pick", Some(one("0"))),
        "return" => ("returns", Some(one("1"))),
        "refund" => ("refunds", Some(json!({ "amount": 1 }))),
        a => (a, None),
    };
    call(w, Method::POST, &format!("/{id}/{path}"), body)
        .await
        .0
}

/// `allowed_actions` lists exactly `want`, and every other action is refused.
async fn allows(w: &W, id: &str, want: &[&str]) {
    let (_, o) = call(w, Method::GET, &format!("/{id}"), None).await;
    let got: Vec<&str> = o["data"]["allowed_actions"]
        .as_array()
        .unwrap_or_else(|| panic!("no allowed_actions: {o}"))
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(got, want, "{}", o["data"]["status"]);
    let (_, page) = call(w, Method::GET, "", None).await;
    let row = page["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    assert_eq!(row["allowed_actions"], o["data"]["allowed_actions"], "list");
    for a in ACTIONS.iter().filter(|a| !want.contains(a)) {
        let s = try_action(w, id, a).await;
        assert!(s.is_client_error(), "{a} not listed but answered {s}");
    }
}

/// Mark the order paid `amount` through a fake provider payment; its id.
async fn paid_sql(w: &W, id: &str) -> String {
    sqlx::query_scalar(
        "WITH o AS (UPDATE orders SET paid_at = now() WHERE id = $1::uuid
                    RETURNING workspace_id, id, total)
         INSERT INTO order_payments (workspace_id, order_id, kind, provider, provider_id, amount,
                                     currency, status)
         SELECT workspace_id, id, 'payment', 'fake', 'fk_' || id, total, 'EUR', 'paid' FROM o
         RETURNING id::text",
    )
    .bind(id)
    .fetch_one(&w.api.pool)
    .await
    .unwrap()
}

#[sqlx::test]
async fn allowed_actions_are_what_the_api_accepts(pool: PgPool) {
    let w = world(pool).await;
    let rel = w
        .api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Shop BV", "type": "business" }),
            "relation_id",
        )
        .await;
    let with_relation = async |id: &str| {
        let body = Some(json!({ "relation_id": rel }));
        call(&w, Method::PATCH, &format!("/{id}"), body).await;
    };

    // Draft without a relation: invoicing needs one, and a draft can't be invoiced anyway.
    let id = draft(&w, json!([line(&w.honey, "2")])).await;
    allows(&w, &id, &["confirm", "cancel"]).await;
    act(&w, &id, "cancel").await;
    allows(&w, &id, &[]).await;

    // Open, not picked: no complete. Then picked.
    let id = draft(&w, json!([line(&w.honey, "2")])).await;
    act(&w, &id, "confirm").await;
    allows(&w, &id, &["pick", "cancel"]).await;
    with_relation(&id).await;
    allows(&w, &id, &["pick", "cancel", "invoice"]).await;
    pick_all(&w, &id).await;
    allows(&w, &id, &["pick", "complete", "cancel", "invoice"]).await;
    act(&w, &id, "complete").await;
    allows(&w, &id, &["return", "invoice"]).await;
    act(&w, &id, "invoice").await;
    allows(&w, &id, &["return"]).await;

    // Paid: cancel needs a refund first; refunded in full: cancel again, nothing to refund.
    let id = draft(&w, json!([line(&w.wax, "1")])).await;
    act(&w, &id, "confirm").await;
    let pay = paid_sql(&w, &id).await;
    allows(&w, &id, &["pick", "refund"]).await;
    sqlx::query(
        "INSERT INTO order_payments (workspace_id, order_id, kind, provider, provider_id,
                                     parent_id, amount, currency, status)
         SELECT workspace_id, order_id, 'refund', 'fake', 'fr_1', id, amount, 'EUR', 'refunded'
         FROM order_payments WHERE id = $1::uuid",
    )
    .bind(&pay)
    .execute(&w.api.pool)
    .await
    .unwrap();
    allows(&w, &id, &["pick", "cancel"]).await;
}

#[sqlx::test]
async fn products_show_what_open_orders_reserve(pool: PgPool) {
    let w = world(pool).await;
    let products = w.api.with_base("/products");
    let reserved = |id: String| {
        let products = &products;
        async move {
            let (s, p) = products
                .call(Method::GET, &format!("/{id}"), None, &products.owner)
                .await;
            assert_eq!(s, StatusCode::OK, "{p}");
            let (_, list) = products.call(Method::GET, "", None, &products.owner).await;
            let row = list["data"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == id.as_str())
                .unwrap()
                .clone();
            assert_eq!(
                row["total_reserved_stock"], p["data"]["total_reserved_stock"],
                "list = get"
            );
            p["data"]["total_reserved_stock"].clone()
        }
    };
    assert_eq!(reserved(w.honey.clone()).await, json!(0));
    let a = draft(&w, json!([line(&w.honey, "3")])).await;
    act(&w, &a, "confirm").await;
    let b = draft(&w, json!([line(&w.honey, "1.5")])).await;
    act(&w, &b, "confirm").await;
    assert_eq!(reserved(w.honey.clone()).await, json!(4.5));
    assert_eq!(reserved(w.wax.clone()).await, json!(0));
    act(&w, &b, "cancel").await;
    assert_eq!(reserved(w.honey.clone()).await, json!(3));
    pick_all(&w, &a).await;
    act(&w, &a, "complete").await;
    assert_eq!(
        reserved(w.honey.clone()).await,
        json!(0),
        "taken, no longer held"
    );
}

/// `(kind, actor type, actor name)` of an order's events, newest first.
async fn events(w: &W, id: &str) -> Vec<(String, Value, Value)> {
    let (s, res) = call(w, Method::GET, &format!("/{id}/events"), None).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            assert!(e["at"].is_string(), "{e}");
            let a = &e["actor"];
            (
                e["kind"].as_str().unwrap().to_owned(),
                a["type"].clone(),
                a["name"].clone(),
            )
        })
        .collect()
}

fn ada(kind: &str) -> (String, Value, Value) {
    (kind.to_owned(), json!("user"), json!("Ada Lovelace"))
}

#[sqlx::test]
async fn every_office_action_leaves_one_event_with_its_actor(pool: PgPool) {
    let w = world(pool).await;
    let id = draft(&w, json!([line(&w.honey, "2"), line(&w.wax, "1")])).await;
    assert_eq!(events(&w, &id).await, [ada("created")]);
    let (_, o) = call(&w, Method::GET, &format!("/{id}"), None).await;
    assert_eq!(o["data"]["confirmed_at"], Value::Null);
    assert_eq!(o["data"]["picked_at"], Value::Null);

    act(&w, &id, "confirm").await;
    let (_, o) = call(&w, Method::GET, &format!("/{id}"), None).await;
    assert!(o["data"]["confirmed_at"].is_string(), "{o}");

    // A partial pick is an event; picked_at waits for the last line.
    let first = &o["data"]["lines"][0];
    let body = json!({ "lines": [{ "line_id": first["id"], "quantity": first["quantity"] }] });
    let (s, o) = call(&w, Method::POST, &format!("/{id}/pick"), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{o}");
    assert_eq!(o["data"]["picked_at"], Value::Null, "one line left");
    pick_all(&w, &id).await;
    let (_, o) = call(&w, Method::GET, &format!("/{id}"), None).await;
    let picked_at = o["data"]["picked_at"].clone();
    assert!(picked_at.is_string(), "{o}");
    pick_all(&w, &id).await;
    let (_, o) = call(&w, Method::GET, &format!("/{id}"), None).await;
    assert_eq!(o["data"]["picked_at"], picked_at, "stamped once");

    act(&w, &id, "complete").await;
    let wax_line = o["data"]["lines"][1]["id"].clone();
    let body = json!({ "lines": [{ "line_id": wax_line, "quantity": "1" }] });
    let (s, res) = call(&w, Method::POST, &format!("/{id}/returns"), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        events(&w, &id).await,
        [
            ada("returned"),
            ada("completed"),
            ada("picked"),
            ada("picked"),
            ada("picked"),
            ada("confirmed"),
            ada("created"),
        ]
    );

    let b = draft(&w, json!([line(&w.honey, "1")])).await;
    act(&w, &b, "confirm").await;
    act(&w, &b, "cancel").await;
    assert_eq!(
        events(&w, &b).await,
        [ada("cancelled"), ada("confirmed"), ada("created")]
    );
    let (_, res) = call(
        &w,
        Method::GET,
        &format!("/{b}/events?page_size=1&page=2"),
        None,
    )
    .await;
    assert_eq!(res["meta"]["total"], 3);
    assert_eq!(res["data"][0]["kind"], "confirmed");
}

#[sqlx::test]
async fn an_invoice_is_an_event_and_a_refused_action_is_none(pool: PgPool) {
    let w = world(pool).await;
    let rel = w
        .api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Client", "type": "business" }),
            "relation_id",
        )
        .await;
    let (s, res) = call(
        &w,
        Method::POST,
        "",
        Some(json!({ "relation_id": rel, "lines": [line(&w.honey, "1")] })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let id = res["data"]["order_id"].as_str().unwrap().to_owned();
    let (s, _) = act(&w, &id, "complete").await;
    assert_eq!(s, StatusCode::CONFLICT, "a draft can't complete");
    act(&w, &id, "confirm").await;
    pick_all(&w, &id).await;
    act(&w, &id, "complete").await;
    let (s, res) = act(&w, &id, "invoice").await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let kinds: Vec<String> = events(&w, &id).await.into_iter().map(|e| e.0).collect();
    assert_eq!(
        kinds,
        ["invoiced", "completed", "picked", "confirmed", "created"]
    );
}

#[sqlx::test]
async fn events_stay_in_their_workspace_and_a_gone_member_loses_their_name(pool: PgPool) {
    let w = world(pool).await;
    let clerk = common::member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "clerk@example.com",
        &["order.create", "order.update", "order.view"],
    )
    .await;
    let (s, res) = w
        .api
        .call(
            Method::POST,
            "",
            Some(json!({ "lines": [line(&w.honey, "1")] })),
            &clerk,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let id = res["data"]["order_id"].as_str().unwrap().to_owned();
    let (_, res) = call(&w, Method::GET, &format!("/{id}/events"), None).await;
    let actor = res["data"][0]["actor"].clone();
    assert_eq!(actor["type"], "user");
    assert!(actor["name"].is_string(), "{actor}");
    let clerk_id = actor["id"].clone();

    sqlx::query(
        "DELETE FROM workspace_members WHERE workspace_id = $1::uuid AND user_id = $2::uuid",
    )
    .bind(&w.api.ws)
    .bind(clerk_id.as_str().unwrap())
    .execute(&w.api.pool)
    .await
    .unwrap();
    let (_, res) = call(&w, Method::GET, &format!("/{id}/events"), None).await;
    assert_eq!(res["data"][0]["actor"]["id"], clerk_id, "id kept");
    assert_eq!(
        res["data"][0]["actor"]["name"],
        Value::Null,
        "no longer a member"
    );

    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (s, _) = eve
        .call(Method::GET, &format!("/{id}/events"), None, &eve.owner)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "acme's order under evil");
    let viewer =
        common::member_with(&w.api.app, &w.api.pool, &w.api.ws, "v@example.com", &[]).await;
    let (s, _) = w
        .api
        .call(Method::GET, &format!("/{id}/events"), None, &viewer)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "needs order.view");
}

#[sqlx::test]
async fn lines_show_their_lots_and_image_and_the_order_its_units(pool: PgPool) {
    let w = world(pool).await;
    let jam = w
        .api
        .with_base("/products")
        .create(json!({ "name": "Jam", "price": "4" }), "product_id")
        .await;
    let lots = w.api.with_base("/lots");
    let mut ids = Vec::new();
    for (code, expires, q) in [("JAM-A", "2027-01-01", "2"), ("JAM-B", "2027-06-01", "5")] {
        let lot = lots
            .create(
                json!({ "product_id": jam, "code": code, "expires_on": expires }),
                "id",
            )
            .await;
        let (s, res) = lots
            .call(
                Method::POST,
                &format!("/{lot}/stock-mutations"),
                Some(json!({ "warehouse_id": w.main, "quantity": q })),
                &lots.owner,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        ids.push(lot);
    }
    let id = draft(&w, json!([line(&jam, "3"), line(&w.honey, "1")])).await;
    let get = |w: &W, id: String| {
        let api = w.api.with_base("/orders");
        async move {
            let (s, o) = api
                .call(Method::GET, &format!("/{id}"), None, &api.owner)
                .await;
            assert_eq!(s, StatusCode::OK, "{o}");
            o["data"].clone()
        }
    };
    let o = get(&w, id.clone()).await;
    assert_eq!(o["units"], "4.00");
    assert_eq!(
        o["payment_method"],
        Value::Null,
        "an office order isn't paid here"
    );
    assert_eq!(o["lines"][0]["lots"], json!([]), "a draft holds nothing");
    assert_eq!(o["lines"][0]["image"], Value::Null);

    act(&w, &id, "confirm").await;
    let held = json!([
        { "id": ids[0], "code": "JAM-A", "quantity": "2.00" },
        { "id": ids[1], "code": "JAM-B", "quantity": "1.00" },
    ]);
    let o = get(&w, id.clone()).await;
    assert_eq!(o["lines"][0]["lots"], held, "held, earliest expiry first");
    assert_eq!(
        o["lines"][1]["lots"],
        json!([]),
        "honey has no-lot stock only"
    );

    pick_all(&w, &id).await;
    let (s, o) = act(&w, &id, "complete").await;
    assert_eq!(s, StatusCode::OK, "{o}");
    assert_eq!(
        o["data"]["lines"][0]["lots"], held,
        "taken from the same lots"
    );

    let file = w
        .api
        .with_base("/files")
        .create(
            json!({ "upload": "iVBORw0KGgo=", "document_name": "jam.png", "public": 1 }),
            "file_id",
        )
        .await;
    let (s, res) = w
        .api
        .with_base("/products")
        .call(
            Method::PUT,
            &format!("/{jam}/images"),
            Some(json!({ "images": [{ "file_id": file }] })),
            &w.api.owner,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{res}");
    let o = get(&w, id.clone()).await;
    assert!(
        o["lines"][0]["image"]["url"]
            .as_str()
            .unwrap()
            .contains("jam.png"),
        "{o}"
    );
    assert_eq!(o["lines"][1]["image"], Value::Null);
    let (_, list) = call(&w, Method::GET, "", None).await;
    let row = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id.as_str())
        .unwrap()
        .clone();
    assert_eq!(row["lines"][0]["lots"], held, "list = get");
    assert_eq!(row["units"], "4.00");

    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (s, _) = eve
        .call(Method::GET, &format!("/{id}"), None, &eve.owner)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

async fn summary(w: &W, token: &str, ws: &str, q: &str) -> (StatusCode, Value) {
    common::send(
        &w.api.app,
        Method::GET,
        &format!("/v1/workspace/{ws}/sales-summary?{q}"),
        None,
        Some(token),
    )
    .await
}

#[sqlx::test]
async fn sales_summary_counts_sold_orders_per_currency(pool: PgPool) {
    let w = world(pool).await;
    // 2 honey (9 %): 2500 + 225 = 2725. 1 wax (21 %): 300 + 63 = 363.
    let a = draft(&w, json!([line(&w.honey, "2")])).await;
    act(&w, &a, "confirm").await;
    let b = draft(&w, json!([line(&w.wax, "1")])).await;
    act(&w, &b, "confirm").await;
    pick_all(&w, &b).await;
    act(&w, &b, "complete").await;
    let _draft = draft(&w, json!([line(&w.honey, "1")])).await;
    let c = draft(&w, json!([line(&w.honey, "1")])).await;
    act(&w, &c, "confirm").await;
    act(&w, &c, "cancel").await;
    // One sale in another currency.
    let usd = w
        .api
        .with_base("/currencies")
        .create(
            json!({ "name": "Dollar", "asset_type": "usd", "decimals": 2,
                    "default_currency": false, "active": true }),
            "currency_id",
        )
        .await;
    let d = draft(&w, json!([line(&w.wax, "2")])).await;
    act(&w, &d, "confirm").await;
    sqlx::query("UPDATE orders SET currency_id = $1::uuid WHERE id = $2::uuid")
        .bind(&usd)
        .bind(&d)
        .execute(&w.api.pool)
        .await
        .unwrap();

    let (ws, owner) = (w.api.ws.clone(), w.api.owner.clone());
    let past = "from=2000-01-01T00:00:00Z&to=2001-01-01T00:00:00Z";
    let (s, res) = summary(&w, &owner, &ws, past).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["currencies"], json!([]), "nothing sold then");

    let now = chrono::Utc::now();
    let range = format!(
        "from={}&to={}",
        (now - chrono::Duration::days(1)).format("%Y-%m-%dT%H:%M:%SZ"),
        (now + chrono::Duration::days(1)).format("%Y-%m-%dT%H:%M:%SZ")
    );
    let (s, res) = summary(&w, &owner, &ws, &range).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    assert_eq!(
        res["data"]["currencies"],
        json!([
            { "currency": "EUR", "decimals": 2, "orders": 2, "gross_total": 2725 + 363,
              "refunded_total": 0 },
            { "currency": "USD", "decimals": 2, "orders": 1, "gross_total": 726,
              "refunded_total": 0 },
        ]),
        "draft and cancelled don't count; currencies never mix"
    );
    let (_, res) = summary(&w, &owner, &ws, &format!("{range}&source=office")).await;
    assert_eq!(res["data"]["currencies"][0]["orders"], 2);
    let (_, res) = summary(&w, &owner, &ws, &format!("{range}&source=till")).await;
    assert_eq!(res["data"]["currencies"], json!([]));

    // `from` inclusive, `to` exclusive, on confirmed_at.
    let (_, o) = call(&w, Method::GET, &format!("/{a}"), None).await;
    let at = o["data"]["confirmed_at"].as_str().unwrap().to_owned();
    let enc = at.replace('+', "%2B");
    let fmt = |d: chrono::DateTime<chrono::Utc>| d.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let (day_before, day_after) = (
        fmt(now - chrono::Duration::days(1)),
        fmt(now + chrono::Duration::days(1)),
    );
    let (_, res) = summary(&w, &owner, &ws, &format!("from={day_before}&to={enc}")).await;
    assert_eq!(res["data"]["currencies"], json!([]), "a is at `to`: out");
    let (_, res) = summary(&w, &owner, &ws, &format!("from={enc}&to={day_after}")).await;
    assert_eq!(res["data"]["currencies"][0]["orders"], 2, "a at `from`: in");

    for q in [
        "",
        "from=2026-01-01T00:00:00Z",
        "to=2026-01-01T00:00:00Z",
        "from=yesterday&to=2026-01-01T00:00:00Z",
        "from=2026-02-01T00:00:00Z&to=2026-01-01T00:00:00Z",
        "from=2024-01-01T00:00:00Z&to=2026-01-01T00:00:00Z",
        "from=2026-01-01T00:00:00Z&to=2026-01-02T00:00:00Z&source=web",
    ] {
        let (s, res) = summary(&w, &owner, &ws, q).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {res}");
    }

    let eve = w.api.as_new_owner("eve@example.com", "evil").await;
    let (s, res) = summary(&w, &eve.owner, &eve.ws, &range).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        res["data"]["currencies"],
        json!([]),
        "acme's sales never count for evil"
    );
    let (s, _) = summary(&w, &eve.owner, &ws, &range).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let clerk = member_with(&w.api.app, &w.api.pool, &ws, "c@example.com", &[]).await;
    let (s, _) = summary(&w, &clerk, &ws, &range).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "needs order.list");
}

async fn relation(w: &W, name: &str) -> String {
    w.api
        .with_base("/relations")
        .create(
            json!({ "company_name": name, "type": "business",
                    "addresses": [{ "address": "Main", "house_number": "1", "zipcode": "1000AA",
                                    "city": "Amsterdam", "country": "NL", "type": "billing" }] }),
            "relation_id",
        )
        .await
}

/// `relation_id` narrows the list to one customer; every item names its customer
/// (`customer_name`: the relation's company name, else the web customer's name).
#[sqlx::test]
async fn list_filters_by_customer_and_names_it(pool: PgPool) {
    let w = world(pool).await;
    let serena = relation(&w, "Hotel Serena").await;
    let other = relation(&w, "Kili Lodge").await;
    let (s, res) = call(
        &w,
        Method::POST,
        "",
        Some(json!({ "relation_id": serena, "lines": [line(&w.honey, "1")] })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{res}");
    let hotel = res["data"]["order_id"].as_str().unwrap().to_owned();
    let (s, _) = call(
        &w,
        Method::POST,
        "",
        Some(json!({ "relation_id": other, "lines": [line(&w.wax, "1")] })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let web = draft(&w, json!([line(&w.honey, "1")])).await;
    sqlx::query(
        r#"UPDATE orders SET customer = '{"name": "Jordan Web", "email": "j@example.com"}'
           WHERE id = $1::uuid"#,
    )
    .bind(&web)
    .execute(&w.api.pool)
    .await
    .unwrap();

    let mine = w.api.list(&format!("?relation_id={serena}")).await;
    assert_eq!(ids(&mine), [hotel.as_str()]);
    assert_eq!(mine["meta"]["total"], 1);
    assert_eq!(mine["data"][0]["customer_name"], "Hotel Serena");
    let all = w.api.list("").await;
    let name_of = |id: &str| {
        all["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["id"] == id)
            .unwrap()["customer_name"]
            .clone()
    };
    assert_eq!(name_of(&web), "Jordan Web");
    let (s, res) = call(&w, Method::GET, "?relation_id=serena", None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["relation_id"][0],
        "The relation id must be a valid UUID."
    );

    // Another workspace: Ada's relation id finds nothing there.
    let bob = w.api.as_new_owner("bob@example.com", "bobs").await;
    let theirs = bob.list(&format!("?relation_id={serena}")).await;
    assert_eq!(theirs["meta"]["total"], 0);
}
