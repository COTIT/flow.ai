//! Flow noticed: GET /attention/priorities (grouped alerts, below-minimum stock rule, grounded
//! impact) and per-user marks (PUT/DELETE /attention/marks). Acknowledge never resolves: an item
//! leaves only when its condition clears. Marks belong to one user in one workspace.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    analytics::{L, W, order, world},
    attention::lot,
    common::{self, Api, invoicing, member_with},
};

async fn call(
    api: &Api,
    method: Method,
    ws: &str,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    common::send(
        &api.app,
        method,
        &format!("/v1/workspace/{ws}/attention{path}"),
        body,
        token,
    )
    .await
}

async fn priorities(api: &Api, token: &str) -> Value {
    let (s, res) = call(api, Method::GET, &api.ws, "/priorities", None, Some(token)).await;
    assert_eq!(s, StatusCode::OK, "{res}");
    res["data"].clone()
}

fn group<'a>(p: &'a Value, kind: &str) -> &'a Value {
    p["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["kind"] == kind)
        .unwrap_or(&Value::Null)
}

fn labels(g: &Value) -> Vec<String> {
    g["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_owned())
        .collect()
}

async fn sql(pool: &PgPool, q: &'static str, a: &str, b: &str) {
    sqlx::query(q).bind(a).bind(b).execute(pool).await.unwrap();
}

async fn product(w: &W, name: &str) -> String {
    crate::attention::product(&w.api, name, json!({})).await
}

/// Second warehouse `Shop`.
async fn shop(w: &W) -> String {
    w.api
        .with_base("/warehouses")
        .create(json!({ "name": "Shop", "is_store": true }), "warehouse_id")
        .await
}

async fn mark(w: &W, token: &str, body: Value) -> (StatusCode, Value) {
    call(
        &w.api,
        Method::PUT,
        &w.api.ws,
        "/marks",
        Some(body),
        Some(token),
    )
    .await
}

/// Honey: 8 + 4 left, row minimums 10 + 5 → minimum 15, shortfall 3. Wax: 4 left, no row
/// minimum → workspace threshold (default 5). Tea: exactly 5 → not below. Oil: 2 free plus a
/// quarantined lot and an expired lot (left out) → below. Untracked products never count.
#[sqlx::test]
async fn below_minimum_uses_row_minimum_else_threshold(pool: PgPool) {
    let w = world(pool).await;
    let api = &w.api;
    let shop = shop(&w).await;
    invoicing::stock(api, &w.honey, &w.warehouse, 8).await;
    invoicing::stock(api, &w.honey, &shop, 4).await;
    sql(&api.pool, "UPDATE stocks SET minimal_stock_level = CASE WHEN warehouse_id = $2::uuid THEN 10 ELSE 5 END WHERE product_id = $1::uuid", &w.honey, &w.warehouse).await;
    invoicing::stock(api, &w.wax, &w.warehouse, 4).await;
    let tea = product(&w, "Tea").await;
    invoicing::stock(api, &tea, &w.warehouse, 5).await;
    let oil = product(&w, "Oil").await;
    invoicing::stock(api, &oil, &w.warehouse, 2).await;
    let q = lot(api, &oil, "OIL-Q", &w.warehouse, 20).await;
    sql(
        &api.pool,
        "UPDATE lots SET status = 'quarantine' WHERE id = $1::uuid AND $2 = $2",
        &q,
        "",
    )
    .await;
    let old = lot(api, &oil, "OIL-X", &w.warehouse, 20).await;
    sql(
        &api.pool,
        "UPDATE lots SET expires_on = current_date - 1 WHERE id = $1::uuid AND $2 = $2",
        &old,
        "",
    )
    .await;
    crate::attention::product(api, "Delivery", json!({ "has_stock": false })).await;

    let p = priorities(api, &api.owner).await;
    let g = group(&p, "stock.below_minimum");
    assert_eq!(g["count"], 3, "{g}");
    assert_eq!(
        labels(g),
        ["Honey", "Oil", "Wax"],
        "shortfall desc, then name: {g}"
    );
    let honey = &g["items"][0];
    assert_eq!(honey["ref_id"], w.honey.as_str());
    assert_eq!(honey["available"], "12.00");
    assert_eq!(honey["minimum"], "15.00");
    assert_eq!(honey["shortfall"], "3.00");
    assert_eq!(honey["acknowledged"], false);
    assert_eq!(g["headline_ref"], w.honey.as_str());

    // The summary carries the same rule as an additive field.
    let (s, sum) = common::send(
        &api.app,
        Method::GET,
        &format!("/v1/workspace/{}/attention", api.ws),
        None,
        Some(&api.owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(sum["data"]["stock"]["below_minimum"]["count"], 3, "{sum}");
    assert_eq!(
        sum["data"]["stock"]["below_minimum"]["items"][0]["product_id"],
        w.honey.as_str()
    );

    // Workspace threshold 3: Wax (4) clears, Oil (2) stays, Honey keeps its row minimum.
    invoicing::settings(api, json!({ "product": { "low_stock_threshold": 3 } })).await;
    let p = priorities(api, &api.owner).await;
    assert_eq!(labels(group(&p, "stock.below_minimum")), ["Honey", "Oil"]);
}

/// Impact: draft/open orders holding a below-minimum product, from the same query as the items.
/// Most affected product first. Completed orders never count.
#[sqlx::test]
async fn impact_counts_open_orders_holding_the_product(pool: PgPool) {
    let w = world(pool).await;
    let api = &w.api;
    invoicing::stock(api, &w.honey, &w.warehouse, 4).await;
    invoicing::stock(api, &w.wax, &w.warehouse, 1).await;
    for status in ["draft", "open", "open", "completed"] {
        order(
            &w,
            "2026-11-01T10:00:00Z",
            None,
            &w.eur,
            status,
            "office",
            &[L(&w.honey, None, "1", 100, 9)],
        )
        .await;
    }
    order(
        &w,
        "2026-11-01T10:00:00Z",
        None,
        &w.eur,
        "draft",
        "office",
        &[L(&w.honey, None, "1", 100, 9), L(&w.wax, None, "1", 100, 9)],
    )
    .await;

    let p = priorities(api, &api.owner).await;
    let g = group(&p, "stock.below_minimum");
    assert_eq!(labels(g), ["Honey", "Wax"], "most open orders first: {g}");
    assert_eq!(g["items"][0]["open_orders"], 4);
    assert_eq!(g["items"][1]["open_orders"], 1);
    assert_eq!(
        g["impact"]["open_orders"], 4,
        "distinct orders over all items: {g}"
    );
    assert_eq!(g["headline_ref"], w.honey.as_str());
    assert_eq!(group(&p, "orders.open")["count"], 4);
}

/// Without `order.list` the impact is unknown (null), never 0, and orders are restricted.
#[sqlx::test]
async fn without_order_list_open_orders_are_null_everywhere(pool: PgPool) {
    let w = world(pool).await;
    let api = &w.api;
    invoicing::stock(api, &w.honey, &w.warehouse, 1).await;
    order(
        &w,
        "2026-11-01T10:00:00Z",
        None,
        &w.eur,
        "draft",
        "office",
        &[L(&w.honey, None, "1", 100, 9)],
    )
    .await;
    let clerk = member_with(&api.app, &api.pool, &api.ws, "clerk@x.io", &["stock.list"]).await;

    let p = priorities(api, &clerk).await;
    let g = group(&p, "stock.below_minimum");
    assert_eq!(g["count"], 2, "Honey and Wax (no stock row): {p}");
    assert!(g["impact"]["open_orders"].is_null(), "{g}");
    for item in g["items"].as_array().unwrap() {
        assert!(item["open_orders"].is_null(), "{item}");
    }
    assert!(group(&p, "orders.open").is_null());
    assert_eq!(
        p["restricted"],
        json!([
            "orders.open",
            "invoices.overdue",
            "till.open_shifts",
            "webhooks.failed"
        ])
    );
    assert_eq!(
        g["actions"],
        json!(["review", "acknowledge", "snooze"]),
        "no adjust without a stock write"
    );

    let owner = priorities(api, &api.owner).await;
    assert_eq!(
        group(&owner, "stock.below_minimum")["actions"],
        json!(["review", "acknowledge", "snooze", "adjust"])
    );
    assert_eq!(owner["restricted"], json!([]));
}

/// Groups: fixed kind order; a group whose every item is snoozed drops to the end.
#[sqlx::test]
async fn groups_order_is_deterministic_and_snoozed_go_last(pool: PgPool) {
    let w = world(pool).await;
    let api = &w.api;
    invoicing::stock(api, &w.honey, &w.warehouse, 1).await;
    order(
        &w,
        "2026-11-01T10:00:00Z",
        None,
        &w.eur,
        "draft",
        "office",
        &[L(&w.wax, None, "1", 100, 9)],
    )
    .await;
    let oil = product(&w, "Oil").await;
    let q = lot(api, &oil, "OIL-Q", &w.warehouse, 20).await;
    sql(
        &api.pool,
        "UPDATE lots SET status = 'quarantine' WHERE id = $1::uuid AND $2 = $2",
        &q,
        "",
    )
    .await;

    let kinds = |p: &Value| {
        p["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["kind"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let p = priorities(api, &api.owner).await;
    assert_eq!(
        kinds(&p),
        ["stock.below_minimum", "orders.open", "lots.quarantined"],
        "{p}"
    );
    assert_eq!(p["groups"], priorities(api, &api.owner).await["groups"]);

    // Wax (no stock row) and Oil (only a quarantined lot) have 0 available: below too.
    for r in [&w.honey, &w.wax, &oil] {
        let (s, res) = mark(&w, &api.owner, json!({ "kind": "stock.below_minimum", "ref_id": r, "state": "snoozed", "snooze": "seven_days" })).await;
        assert_eq!(s, StatusCode::OK, "{res}");
    }
    let p = priorities(api, &api.owner).await;
    assert_eq!(
        kinds(&p),
        ["orders.open", "lots.quarantined", "stock.below_minimum"]
    );
    let g = group(&p, "stock.below_minimum");
    assert_eq!(
        (
            g["count"].clone(),
            g["open_count"].clone(),
            g["snoozed_count"].clone(),
            g["snoozed"].clone()
        ),
        (json!(3), json!(0), json!(3), json!(true))
    );
    assert_eq!(g["items"], json!([]), "snoozed items leave the list");
    assert!(g["headline_ref"].is_null());
}

/// Acknowledge keeps the item listed; it leaves only when stock rises above its minimum.
/// Another member never sees my marks.
#[sqlx::test]
async fn acknowledge_does_not_resolve_and_marks_are_per_user(pool: PgPool) {
    let w = world(pool).await;
    let api = &w.api;
    invoicing::stock(api, &w.honey, &w.warehouse, 1).await;
    invoicing::stock(api, &w.wax, &w.warehouse, 2).await;
    let (s, m) = mark(
        &w,
        &api.owner,
        json!({ "kind": "stock.below_minimum", "ref_id": w.honey, "state": "acknowledged" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["data"]["state"], "acknowledged");
    assert!(m["data"]["snoozed_until"].is_null());

    let p = priorities(api, &api.owner).await;
    let g = group(&p, "stock.below_minimum");
    assert_eq!(
        (
            g["count"].clone(),
            g["open_count"].clone(),
            g["acknowledged_count"].clone()
        ),
        (json!(2), json!(2), json!(1))
    );
    let honey = g["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["ref_id"] == w.honey.as_str())
        .unwrap();
    assert_eq!(honey["acknowledged"], true);
    let level: String =
        sqlx::query_scalar("SELECT sum(stock_lvl)::text FROM stocks WHERE product_id = $1::uuid")
            .bind(&w.honey)
            .fetch_one(&api.pool)
            .await
            .unwrap();
    assert_eq!(level, "1.00", "acknowledge never touches stock");

    let mate = member_with(&api.app, &api.pool, &api.ws, "mate@x.io", &["stock.list"]).await;
    let theirs = priorities(api, &mate).await;
    assert_eq!(
        group(&theirs, "stock.below_minimum")["acknowledged_count"],
        0
    );
    // Their delete of my mark is 404.
    let id = m["data"]["id"].as_str().unwrap();
    let (s, _) = call(
        api,
        Method::DELETE,
        &api.ws,
        &format!("/marks/{id}"),
        None,
        Some(&mate),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Condition clears → gone, ack or not.
    invoicing::stock(api, &w.honey, &w.warehouse, 10).await;
    assert_eq!(
        labels(group(
            &priorities(api, &api.owner).await,
            "stock.below_minimum"
        )),
        ["Wax"]
    );

    // My delete: 204, then 404.
    let (s, _) = call(
        api,
        Method::DELETE,
        &api.ws,
        &format!("/marks/{id}"),
        None,
        Some(&api.owner),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(
        api,
        Method::DELETE,
        &api.ws,
        &format!("/marks/{id}"),
        None,
        Some(&api.owner),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

/// Snooze ends are server-computed: next local midnight, or now + 7 days. Re-marking upserts.
#[sqlx::test]
async fn snooze_until_tomorrow_is_next_local_midnight(pool: PgPool) {
    let w = world(pool).await;
    let api = &w.api;
    invoicing::settings(
        api,
        json!({ "general": { "timezone": "Africa/Dar_es_Salaam" } }),
    )
    .await;
    let body = |snooze: &str| json!({ "kind": "stock.below_minimum", "ref_id": w.honey, "state": "snoozed", "snooze": snooze });
    let (s, m) = mark(&w, &api.owner, body("until_tomorrow")).await;
    assert_eq!(s, StatusCode::OK, "{m}");
    let ok: bool = sqlx::query_scalar(
        "SELECT $1::timestamptz = (((now() AT TIME ZONE 'Africa/Dar_es_Salaam')::date + 1)::timestamp AT TIME ZONE 'Africa/Dar_es_Salaam')",
    )
    .bind(m["data"]["snoozed_until"].as_str().unwrap())
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert!(ok, "{m}");
    let (_, m2) = mark(&w, &api.owner, body("seven_days")).await;
    assert_eq!(
        m2["data"]["id"], m["data"]["id"],
        "one mark per user and ref"
    );
    let ok: bool = sqlx::query_scalar("SELECT $1::timestamptz BETWEEN now() + interval '7 days' - interval '1 minute' AND now() + interval '7 days'")
        .bind(m2["data"]["snoozed_until"].as_str().unwrap())
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert!(ok, "{m2}");
}

#[sqlx::test]
async fn marks_refuse_bad_input_foreign_refs_and_missing_permission(pool: PgPool) {
    let w = world(pool).await;
    let api = &w.api;
    let field = |res: &Value, f: &str| res["errors"][f].is_array();
    for (body, f) in [
        (
            json!({ "kind": "stock.other", "ref_id": w.honey, "state": "acknowledged" }),
            "kind",
        ),
        (
            json!({ "kind": "stock.below_minimum", "ref_id": "nope", "state": "acknowledged" }),
            "ref_id",
        ),
        (
            json!({ "kind": "stock.below_minimum", "ref_id": w.honey, "state": "done" }),
            "state",
        ),
        (
            json!({ "kind": "stock.below_minimum", "ref_id": w.honey, "state": "snoozed" }),
            "snooze",
        ),
        (
            json!({ "kind": "stock.below_minimum", "ref_id": w.honey, "state": "snoozed", "snooze": "forever" }),
            "snooze",
        ),
        (
            json!({ "kind": "stock.below_minimum", "ref_id": w.honey, "state": "acknowledged", "snooze": "seven_days" }),
            "snooze",
        ),
    ] {
        let (s, res) = mark(&w, &api.owner, body.clone()).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(field(&res, f), "{body}: {res}");
    }
    // Unknown ref, a ref of another kind, another workspace's product: 404.
    let other = api.as_new_owner("eve@x.io", "evil").await;
    let theirs = other
        .with_base("/products")
        .create(json!({ "name": "Their honey", "price": "1" }), "product_id")
        .await;
    for r in [
        uuid::Uuid::now_v7().to_string(),
        w.alpha.clone(),
        theirs.clone(),
    ] {
        let (s, res) = mark(
            &w,
            &api.owner,
            json!({ "kind": "stock.below_minimum", "ref_id": r, "state": "acknowledged" }),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{r}: {res}");
    }
    // Their token on my workspace: 404 on every route.
    for (m, path, body) in [
        (Method::GET, "/priorities".to_owned(), None),
        (
            Method::PUT,
            "/marks".to_owned(),
            Some(
                json!({ "kind": "stock.below_minimum", "ref_id": w.honey, "state": "acknowledged" }),
            ),
        ),
        (
            Method::DELETE,
            format!("/marks/{}", uuid::Uuid::now_v7()),
            None,
        ),
    ] {
        let (s, _) = call(
            api,
            m.clone(),
            &api.ws,
            &path,
            body.clone(),
            Some(&other.owner),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{m} {path}");
        let (s, _) = call(api, m, &api.ws, &path, body, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{path}");
    }
    // A member without the kind's list permission: 403.
    let clerk = member_with(&api.app, &api.pool, &api.ws, "clerk@x.io", &["order.list"]).await;
    let (s, _) = mark(
        &w,
        &clerk,
        json!({ "kind": "stock.below_minimum", "ref_id": w.honey, "state": "acknowledged" }),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

/// Prune: snoozes over a day past their end, acknowledgements over 30 days, marks whose record
/// is gone. Live marks stay.
#[sqlx::test]
async fn prune_drops_stale_marks(pool: PgPool) {
    let w = world(pool).await;
    let api = &w.api;
    let tea = product(&w, "Tea").await;
    let user: String =
        sqlx::query_scalar("SELECT created_by_user_id::text FROM workspaces WHERE id = $1::uuid")
            .bind(&api.ws)
            .fetch_one(&api.pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO attention_marks (workspace_id, user_id, kind, ref_id, state, snoozed_until, created_at)
         SELECT $1::uuid, $2::uuid, k, r, s, u, c FROM (VALUES
           ('stock.below_minimum', $3::uuid, 'snoozed', now() - interval '25 hours', now() - interval '3 days'),
           ('stock.below_minimum', $4::uuid, 'snoozed', now() - interval '1 hour', now() - interval '3 days'),
           ('stock.below_minimum', $5::uuid, 'acknowledged', NULL::timestamptz, now() - interval '31 days'),
           ('stock.below_minimum', gen_random_uuid(), 'acknowledged', NULL, now()),
           ('orders.open', gen_random_uuid(), 'acknowledged', NULL, now())
         ) AS v(k, r, s, u, c)",
    )
    .bind(&api.ws).bind(&user).bind(&w.honey).bind(&w.wax).bind(&tea)
    .execute(&api.pool).await.unwrap();
    let pruned = genesy_360_api::cleanup::prune(&api.pool).await.unwrap();
    assert_eq!(pruned.attention_marks, 4);
    let left: Vec<String> = sqlx::query_scalar("SELECT ref_id::text FROM attention_marks")
        .fetch_all(&api.pool)
        .await
        .unwrap();
    assert_eq!(left, std::slice::from_ref(&w.wax));
}
