//! GET /v1/workspace/{ws}/attention: what an office operator should act on today. Each
//! section reuses an existing rule (tied to it below); a section the caller can't list is
//! null; another workspace's rows never count.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{
    Api,
    invoicing::{self, F},
    member_with, send,
};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn attention(api: &Api, ws: &str, token: Option<&str>) -> (StatusCode, Value) {
    send(
        &api.app,
        Method::GET,
        &format!("/v1/workspace/{ws}/attention"),
        None,
        token,
    )
    .await
}

async fn ok(api: &Api, token: &str) -> Value {
    let (status, res) = attention(api, &api.ws, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

async fn sql(pool: &PgPool, q: &'static str, id: &str) {
    sqlx::query(q).bind(id).execute(pool).await.unwrap();
}

async fn order(f: &F) -> String {
    let (status, res) = f
        .api
        .with_base("/orders")
        .call(
            Method::POST,
            "",
            Some(json!({ "lines": [{ "product_id": f.product, "quantity": "1" }] })),
            &f.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["order_id"].as_str().unwrap().to_owned()
}

async fn detail(api: &Api, path: &str) -> Value {
    let (status, res) = send(
        &api.app,
        Method::GET,
        &format!("/v1/workspace/{}{path}", api.ws),
        None,
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

fn has(list: &Value, name: &str) -> bool {
    list.as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(name)))
}

fn ids(items: &Value, key: &str) -> Vec<String> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i[key].as_str().unwrap().to_owned())
        .collect()
}

/// Product `name` (tracked unless `has_stock: false` in `extra`); returns its id.
pub async fn product(api: &Api, name: &str, extra: Value) -> String {
    let mut body = json!({ "name": name, "price": "5" });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    api.with_base("/products").create(body, "product_id").await
}

/// Lot `code` of `product` holding `quantity` at `warehouse` (0 = stocked, then emptied).
pub async fn lot(api: &Api, product: &str, code: &str, warehouse: &str, quantity: i64) -> String {
    let lots = api.with_base("/lots");
    let id = lots
        .create(json!({ "product_id": product, "code": code }), "id")
        .await;
    for q in [quantity.max(1), quantity - quantity.max(1)] {
        if q == 0 {
            continue;
        }
        let (s, res) = lots
            .call(
                Method::POST,
                &format!("/{id}/stock-mutations"),
                Some(json!({ "warehouse_id": warehouse, "quantity": q, "reason": "count" })),
                &api.owner,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{res}");
    }
    id
}

/// Register `name` at `warehouse` with one open shift and one closed one.
async fn shifts(f: &F, name: &str) -> String {
    let ws = &f.api.ws;
    let register: String = sqlx::query_scalar(
        "INSERT INTO till_registers (workspace_id, name, warehouse_id, currency_id)
         SELECT $1::uuid, $2, $3::uuid, id FROM currencies WHERE workspace_id = $1::uuid LIMIT 1
         RETURNING id::text",
    )
    .bind(ws)
    .bind(name)
    .bind(&f.shop)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    let cashier: String = sqlx::query_scalar(
        "INSERT INTO till_users (workspace_id, name, pin_hash) VALUES ($1::uuid, 'Cas', 'x')
         RETURNING id::text",
    )
    .bind(ws)
    .fetch_one(&f.api.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO drawer_shifts (workspace_id, register_id, opened_by, opened_at, opening,
                                    closed_by, closed_at, expected, counted, difference)
         VALUES ($1::uuid, $2::uuid, $3::uuid, now() - interval '2 days', 0,
                 $3::uuid, now() - interval '1 day', 0, 0, 0)",
    )
    .bind(ws)
    .bind(&register)
    .bind(&cashier)
    .execute(&f.api.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO drawer_shifts (workspace_id, register_id, opened_by, opened_at, opening)
         VALUES ($1::uuid, $2::uuid, $3::uuid, now() - interval '20 hours', 500)",
    )
    .bind(ws)
    .bind(&register)
    .bind(&cashier)
    .execute(&f.api.pool)
    .await
    .unwrap();
    register
}

/// One failed and one delivered webhook delivery of event `type`.
async fn deliveries(f: &F, kind: &str) {
    sqlx::query(
        "WITH e AS (INSERT INTO webhook_endpoints (workspace_id, url, events, secret_sealed,
                                                   secret_key_id)
                    VALUES ($1::uuid, 'https://hooks.example/x', ARRAY[$2], '\\x00', 'k')
                    RETURNING id),
              ev AS (INSERT INTO webhook_events (workspace_id, type, payload)
                     VALUES ($1::uuid, $2, '{}'), ($1::uuid, $2, '{}') RETURNING id)
         INSERT INTO webhook_deliveries (workspace_id, event_id, endpoint_id, status, attempts,
                                         last_error)
         SELECT $1::uuid, ev.id, e.id,
                CASE WHEN row_number() OVER (ORDER BY ev.id) = 1 THEN 'failed' ELSE 'delivered' END,
                8, 'connection refused'
         FROM ev, e",
    )
    .bind(&f.api.ws)
    .bind(kind)
    .execute(&f.api.pool)
    .await
    .unwrap();
}

/// An empty workspace has nothing to deal with: every section zero, no items.
#[sqlx::test]
async fn empty_workspace_has_nothing_to_do(pool: PgPool) {
    let api = Api::new(pool, "/attention").await;
    let d = ok(&api, &api.owner.clone()).await;
    assert!(d["generated_at"].is_string(), "{d}");
    assert_eq!(
        d["orders"],
        json!({ "draft": 0, "open": 0, "awaiting_payment": 0, "items": [] })
    );
    assert_eq!(
        d["invoices"],
        json!({ "draft": 0, "unpaid": 0, "overdue": 0, "open": [], "items": [] })
    );
    assert_eq!(
        d["stock"],
        json!({ "negative": 0, "out_of_stock": 0, "items": [],
                "below_minimum": { "count": 0, "items": [] } })
    );
    assert_eq!(d["till_shifts"], json!({ "open": 0, "items": [] }));
    assert_eq!(
        d["lots"],
        json!({ "quarantined": 0, "held_orders": 0, "items": [] })
    );
    assert_eq!(d["webhooks"], json!({ "failed": 0, "items": [] }));
}

/// Orders: draft (confirm allowed), open (pick/complete), awaiting web payment. Completed
/// orders don't count; items are draft and open, oldest first, total in cents.
#[sqlx::test]
async fn orders_count_draft_open_and_awaiting(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    let draft = order(&f).await;
    let open = order(&f).await;
    let awaiting = order(&f).await;
    let done = order(&f).await;
    let db = &f.api.pool;
    sql(
        db,
        "UPDATE orders SET status = 'open' WHERE id = $1::uuid",
        &open,
    )
    .await;
    sql(
        db,
        "UPDATE orders SET status = 'awaiting_payment', expires_at = now() + interval '1 hour'
         WHERE id = $1::uuid",
        &awaiting,
    )
    .await;
    sql(
        db,
        "UPDATE orders SET status = 'completed' WHERE id = $1::uuid",
        &done,
    )
    .await;
    let d = ok(&f.api, &f.api.owner).await;
    let o = &d["orders"];
    assert_eq!(
        (&o["draft"], &o["open"], &o["awaiting_payment"]),
        (&json!(1), &json!(1), &json!(1)),
        "{o}"
    );
    assert_eq!(ids(&o["items"], "id"), [draft.clone(), open.clone()]);
    let first = &o["items"][0];
    assert_eq!(first["status"], "draft");
    assert!(first["number"].is_string(), "{first}");
    assert!(first["created_at"].is_string(), "{first}");
    let total: i64 = sqlx::query_scalar("SELECT total FROM orders WHERE id = $1::uuid")
        .bind(&draft)
        .fetch_one(db)
        .await
        .unwrap();
    assert_eq!(first["total"], json!(total));
    assert!(total > 0);
    assert_eq!(first["currency"], "EUR");
    assert_eq!(first["decimals"], 2);
    // Tied to the order rules: a counted draft is one the API lets you confirm.
    assert!(has(
        &detail(&f.api, &format!("/orders/{draft}")).await["allowed_actions"],
        "confirm"
    ));
}

/// The items list caps at five, oldest first, while the counts stay whole.
#[sqlx::test]
async fn items_cap_at_five_oldest_first(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    let mut made = Vec::new();
    for _ in 0..7 {
        made.push(order(&f).await);
    }
    let o = ok(&f.api, &f.api.owner).await["orders"].clone();
    assert_eq!(o["draft"], 7);
    assert_eq!(ids(&o["items"], "id"), made[..5]);
}

/// Invoices: drafts to send, and unpaid = the "pay" rule (not credit, pending or partially
/// paid). Overdue = unpaid with a due date before today. Open money sums in SQL per currency.
#[sqlx::test]
async fn invoices_count_drafts_unpaid_and_overdue(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    let db = &f.api.pool;
    let lines = || invoicing::chairs(&f, 1, None);
    let draft = invoicing::draft(&f, lines(), false).await;
    let late = invoicing::draft(&f, lines(), false).await;
    let part = invoicing::draft(&f, lines(), false).await;
    let paid = invoicing::draft(&f, lines(), false).await;
    let credit = invoicing::draft(&f, lines(), true).await;
    sql(
        db,
        "UPDATE invoices SET status = 'pending', invoice_number = 1001, invoice_appendix = '{\"prefix\": \"INV-\", \"digits\": 4}',
                due_date = now() - interval '3 days' WHERE id = $1::uuid",
        &late,
    )
    .await;
    sql(
        db,
        "UPDATE invoices SET status = 'partially_paid', invoice_number = 1002, invoice_appendix = '{\"prefix\": \"INV-\", \"digits\": 4}',
                paid_total = 210, due_date = now() + interval '3 days' WHERE id = $1::uuid",
        &part,
    )
    .await;
    sql(
        db,
        "UPDATE invoices SET status = 'paid', invoice_number = 1003, invoice_appendix = '{\"prefix\": \"INV-\", \"digits\": 4}', paid_total = total
         WHERE id = $1::uuid",
        &paid,
    )
    .await;
    sql(
        db,
        "UPDATE invoices SET status = 'pending', invoice_number = 1004, invoice_appendix = '{\"prefix\": \"INV-\", \"digits\": 4}' WHERE id = $1::uuid",
        &credit,
    )
    .await;
    let totals: Vec<i64> =
        sqlx::query_scalar("SELECT total FROM invoices WHERE id IN ($1::uuid, $2::uuid)")
            .bind(&late)
            .bind(&part)
            .fetch_all(db)
            .await
            .unwrap();
    let open: i64 = totals.iter().sum::<i64>() - 210;

    let i = ok(&f.api, &f.api.owner).await["invoices"].clone();
    assert_eq!(
        (&i["draft"], &i["unpaid"], &i["overdue"]),
        (&json!(1), &json!(2), &json!(1)),
        "{i}"
    );
    assert_eq!(
        i["open"],
        json!([{ "currency": "EUR", "decimals": 2, "amount": open }])
    );
    assert_eq!(ids(&i["items"], "id"), [late.clone(), part.clone()]);
    let first = &i["items"][0];
    assert_eq!(first["overdue"], true);
    assert_eq!(i["items"][1]["overdue"], false);
    assert_eq!(first["number"], "INV-1001");
    assert_eq!(first["status"], "pending");
    assert!(first["customer_name"].is_string(), "{first}");
    assert!(first["due_date"].is_string(), "{first}");
    assert_eq!(i["items"][1]["open"], json!(totals[1] - 210));
    assert_eq!(first["currency"], "EUR");
    // Tied to the invoice rules: counted = "pay" allowed; the credit note and the draft not.
    assert!(has(
        &detail(&f.api, &format!("/invoices/{late}")).await["allowed_actions"],
        "pay"
    ));
    assert!(!has(
        &detail(&f.api, &format!("/invoices/{credit}")).await["allowed_actions"],
        "pay"
    ));
    assert!(has(
        &detail(&f.api, &format!("/invoices/{draft}")).await["allowed_actions"],
        "send"
    ));
}

/// Overdue starts at local midnight in `general.timezone`; an unknown zone falls back to UTC.
#[sqlx::test]
async fn overdue_starts_at_local_midnight(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    let db = &f.api.pool;
    let before = invoicing::draft(&f, invoicing::chairs(&f, 1, None), false).await;
    let after = invoicing::draft(&f, invoicing::chairs(&f, 1, None), false).await;
    let due = |zone: &'static str, id: String, minutes: i32| async move {
        sqlx::query(
            "UPDATE invoices SET status = 'pending',
                    due_date = ((now() AT TIME ZONE $2)::date::timestamp AT TIME ZONE $2)
                               + make_interval(mins => $3)
             WHERE id = $1::uuid",
        )
        .bind(id)
        .bind(zone)
        .bind(minutes)
        .execute(db)
        .await
        .unwrap();
    };
    // UTC+14: local midnight is far from UTC midnight.
    invoicing::settings(
        &f.api,
        json!({ "general": { "timezone": "Pacific/Kiritimati" } }),
    )
    .await;
    due("Pacific/Kiritimati", before.clone(), -1).await;
    due("Pacific/Kiritimati", after.clone(), 1).await;
    let i = ok(&f.api, &f.api.owner).await["invoices"].clone();
    assert_eq!((&i["unpaid"], &i["overdue"]), (&json!(2), &json!(1)), "{i}");
    assert_eq!(i["items"][0]["id"], json!(before));

    invoicing::settings(&f.api, json!({ "general": { "timezone": "Mars/Olympus" } })).await;
    due("UTC", before.clone(), -1).await;
    due("UTC", after.clone(), 1).await;
    let i = ok(&f.api, &f.api.owner).await["invoices"].clone();
    assert_eq!(i["overdue"], 1, "unknown zone = UTC: {i}");
    assert_eq!(i["items"][0]["overdue"], true);
    assert_eq!(i["items"][1]["overdue"], false);
}

/// Stock: negative = store rows below zero (the negative-stock list); out of stock = the
/// shop's sold-out rule per tracked product (Σ level − reserved over live rows, held or
/// expired lots left out). An empty lot beside a full one is not out of stock.
#[sqlx::test]
async fn stock_counts_negative_rows_and_sold_out_products(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    let (api, db) = (&f.api, &f.api.pool);
    let honey = product(api, "Honey", json!({})).await;
    lot(api, &honey, "HON-A", &f.main, 0).await;
    lot(api, &honey, "HON-B", &f.main, 10).await;
    let wax = product(api, "Wax", json!({})).await;
    invoicing::stock(api, &wax, &f.main, 3).await;
    sqlx::query(
        "UPDATE stocks SET reserved = 3 WHERE product_id = $1::uuid AND warehouse_id = $2::uuid",
    )
    .bind(&wax)
    .bind(&f.main)
    .execute(db)
    .await
    .unwrap();
    let jam = product(api, "Jam", json!({})).await;
    let tea = product(api, "Tea", json!({})).await;
    invoicing::stock(api, &tea, &f.shop, 1).await;
    sqlx::query(
        "UPDATE stocks SET stock_lvl = -2 WHERE product_id = $1::uuid AND warehouse_id = $2::uuid",
    )
    .bind(&tea)
    .bind(&f.shop)
    .execute(db)
    .await
    .unwrap();
    let oil = product(api, "Oil", json!({})).await;
    let held = lot(api, &oil, "OIL-Q", &f.main, 4).await;
    sql(
        db,
        "UPDATE lots SET status = 'quarantine' WHERE id = $1::uuid",
        &held,
    )
    .await;
    let expired = product(api, "Milk", json!({})).await;
    let old = lot(api, &expired, "MILK-1", &f.main, 4).await;
    sql(
        db,
        "UPDATE lots SET expires_on = current_date - 1 WHERE id = $1::uuid",
        &old,
    )
    .await;
    product(api, "Delivery", json!({ "has_stock": false })).await;

    let s = ok(api, &api.owner).await["stock"].clone();
    assert_eq!(s["negative"], 1, "{s}");
    assert_eq!(s["out_of_stock"], 5, "{s}");
    let mut out = ids(&s["items"], "product_id");
    assert_eq!(out[0], tea, "lowest available first: {s}");
    assert_eq!(s["items"][0]["product_name"], "Tea");
    assert_eq!(s["items"][0]["available"], "-2.00");
    out.sort();
    let mut want = vec![wax, jam, tea, oil, expired];
    want.sort();
    assert_eq!(out, want);
}

/// Till: shifts still open, with register and opened_at; closed shifts don't count.
#[sqlx::test]
async fn till_lists_open_shifts(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    let register = shifts(&f, "Front desk").await;
    let t = ok(&f.api, &f.api.owner).await["till_shifts"].clone();
    assert_eq!(t["open"], 1, "{t}");
    let s = &t["items"][0];
    assert_eq!(s["register_id"], json!(register));
    assert_eq!(s["register_name"], "Front desk");
    assert_eq!(s["opened_by"], "Cas");
    assert!(s["opened_at"].is_string(), "{s}");
}

/// Lots: quarantined lots wait for release or recall; held_orders counts orders whose hold
/// sits on a lot taken off sale (the `/lots/{lot}/holds` rule).
#[sqlx::test]
async fn lots_count_quarantine_and_stuck_holds(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    let (api, db) = (&f.api, &f.api.pool);
    let oil = product(api, "Oil", json!({})).await;
    let held = lot(api, &oil, "OIL-Q", &f.main, 4).await;
    lot(api, &oil, "OIL-OK", &f.main, 4).await;
    let o = order(&f).await;
    sqlx::query(
        "INSERT INTO stock_reservations (workspace_id, stock_id, product_id, related_domain,
                                         related_id, quantity)
         SELECT workspace_id, id, product_id, 'order', $2::uuid, 1 FROM stocks
         WHERE lot_id = $1::uuid",
    )
    .bind(&held)
    .bind(&o)
    .execute(db)
    .await
    .unwrap();
    let l = ok(api, &api.owner).await["lots"].clone();
    assert_eq!(
        (&l["quarantined"], &l["held_orders"]),
        (&json!(0), &json!(0)),
        "released: {l}"
    );
    sql(
        db,
        "UPDATE lots SET status = 'quarantine' WHERE id = $1::uuid",
        &held,
    )
    .await;
    let l = ok(api, &api.owner).await["lots"].clone();
    assert_eq!(
        (&l["quarantined"], &l["held_orders"]),
        (&json!(1), &json!(1)),
        "{l}"
    );
    assert_eq!(ids(&l["items"], "id"), std::slice::from_ref(&held));
    assert_eq!(l["items"][0]["code"], "OIL-Q");
    assert_eq!(l["items"][0]["product_name"], "Oil");
    assert_eq!(l["items"][0]["held_orders"], 1);
    assert_eq!(
        detail(api, &format!("/lots/{held}/holds"))
            .await
            .as_array()
            .map(Vec::len),
        Some(1),
        "same hold the holds route lists"
    );
}

/// Webhooks: failed deliveries (dead letters) for admins; delivered ones don't count.
#[sqlx::test]
async fn webhooks_count_failed_deliveries(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    deliveries(&f, "order.created").await;
    let w = ok(&f.api, &f.api.owner).await["webhooks"].clone();
    assert_eq!(w["failed"], 1, "{w}");
    let d = &w["items"][0];
    assert_eq!(d["event_type"], "order.created");
    assert_eq!(d["attempts"], 8);
    assert_eq!(d["last_error"], "connection refused");
    assert!(
        d["endpoint_id"].is_string() && d["updated_at"].is_string(),
        "{d}"
    );
    assert!(d.get("url").is_none(), "no endpoint url: {d}");
}

/// Each section needs the permission of its list route; lots follow the lot routes (any
/// member). A cashier sees open shifts only.
#[sqlx::test]
async fn sections_follow_list_permissions(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    order(&f).await;
    shifts(&f, "Front desk").await;
    deliveries(&f, "order.created").await;
    let (app, db, ws) = (&f.api.app, &f.api.pool, &f.api.ws);

    let clerk = member_with(app, db, ws, "olga@example.com", &["order.list"]).await;
    let d = ok(&f.api, &clerk).await;
    assert_eq!(d["orders"]["draft"], 1, "{d}");
    for s in ["invoices", "stock", "till_shifts", "webhooks"] {
        assert!(d[s].is_null(), "{s}: {d}");
    }
    assert!(d["lots"].is_object(), "{d}");

    let cashier = member_with(app, db, ws, "cas@example.com", &["cashregister.list"]).await;
    let d = ok(&f.api, &cashier).await;
    assert_eq!(d["till_shifts"]["open"], 1, "{d}");
    for s in ["orders", "invoices", "stock", "webhooks"] {
        assert!(d[s].is_null(), "{s}: {d}");
    }

    let books = member_with(
        app,
        db,
        ws,
        "ina@example.com",
        &["invoice.list", "stock.list"],
    )
    .await;
    let d = ok(&f.api, &books).await;
    assert!(d["invoices"].is_object() && d["stock"].is_object(), "{d}");
    assert!(d["orders"].is_null() && d["webhooks"].is_null(), "{d}");

    let admin = member_with(app, db, ws, "adm@example.com", &["workspace_admin"]).await;
    assert_eq!(ok(&f.api, &admin).await["webhooks"]["failed"], 1);
}

/// Outsiders get 404 for Acme; their own summary never counts Acme's rows; no token is 401.
#[sqlx::test]
async fn another_workspace_is_refused_and_never_counted(pool: PgPool) {
    let f = invoicing::setup(pool).await;
    order(&f).await;
    let late = invoicing::draft(&f, invoicing::chairs(&f, 1, None), false).await;
    sql(
        &f.api.pool,
        "UPDATE invoices SET status = 'pending', due_date = now() - interval '1 day'
         WHERE id = $1::uuid",
        &late,
    )
    .await;
    shifts(&f, "Front desk").await;
    deliveries(&f, "order.created").await;
    let bob = f.api.as_new_owner("bob@example.com", "bobs").await;

    let (status, res) = attention(&f.api, &f.api.ws, Some(&bob.owner)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{res}");
    let (status, _) = attention(&f.api, &f.api.ws, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let d = ok(&bob, &bob.owner.clone()).await;
    assert_eq!(d["orders"]["draft"], 0, "{d}");
    assert_eq!(d["invoices"]["unpaid"], 0, "{d}");
    assert_eq!(d["invoices"]["draft"], 0, "{d}");
    assert_eq!(d["stock"]["out_of_stock"], 0, "{d}");
    assert_eq!(d["till_shifts"]["open"], 0, "{d}");
    assert_eq!(d["webhooks"]["failed"], 0, "{d}");
}
