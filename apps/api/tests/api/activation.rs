//! Activation (Flow ERP M8, masterplan §14 ACTIVATION, §15 `POST /accounting/activate`):
//! the opening review is the preflight (every step-6 refusal is an issue), activation runs the
//! reviewed opening and flips the book in one transaction, refusals write nothing, replay is
//! permanent, permission and isolation hold; the opening baselines read lists what it carried.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    common::member_with,
    opening::{O, opened},
};

fn codes(review: &Value) -> Vec<String> {
    let mut c: Vec<String> = review["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["code"].as_str().unwrap().to_owned())
        .collect();
    c.sort();
    c
}

/// Review the batches as the owner; the review data.
async fn review(o: &O, batches: &[String]) -> Value {
    let ids: Vec<&str> = batches.iter().map(String::as_str).collect();
    let (s, r) = o.review(&o.w.owner.clone(), &ids).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    r["data"].clone()
}

async fn state(o: &O) -> String {
    o.w.setup().await["state"].as_str().unwrap().to_owned()
}

/// Fixture rows the preflight must see, planted with FK and append-only triggers off (the
/// check reads only their state; building each through its workflow is its own suite's job).
async fn plant(pool: &PgPool, sql: &'static str, ws: &str, owner: Uuid) {
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("SET session_replication_role = replica")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(sql)
        .bind(ws)
        .bind(owner)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("SET session_replication_role = origin")
        .execute(&mut *conn)
        .await
        .unwrap();
}

#[sqlx::test]
async fn activation_opens_the_books_once_and_replays(pool: PgPool) {
    let (o, vault, batches) = opened(pool).await;
    let r = review(&o, &batches).await;
    assert_eq!(r["ready"], true, "{r}");
    assert!(
        r["cutover_date"].as_str().unwrap().starts_with("2026-"),
        "{r}"
    );
    let fp = r["opening_fingerprint"].as_str().unwrap().to_owned();
    let version = o.w.setup().await["version"].clone();
    let body = json!({ "client_ref": Uuid::now_v7(), "expected_version": version,
                       "payload": { "batch_ids": batches, "opening_fingerprint": fp } });
    let key = Uuid::now_v7().to_string();
    let (s, first) =
        o.w.raw(
            Method::POST,
            "/activate",
            Some(body.clone()),
            &o.w.owner,
            &key,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&first));
    let b: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(b["data"]["state"], "active");
    assert!(b["data"]["activated_at"].is_string(), "{b}");
    // Steps 7-10 together: stock 10, AR 60 opening journal, place opening, audit columns.
    assert_eq!(o.stock_level().await, "10.00");
    assert_eq!(
        o.scalar("SELECT count(*) FROM journal_entries WHERE workspace_id = $1::uuid")
            .await,
        1
    );
    assert_eq!(o.place_balance(&vault).await, 5_000);
    let (by, fingerprint): (Uuid, Vec<u8>) = sqlx::query_as(
        "SELECT activated_by, opening_fingerprint FROM accounting_settings WHERE workspace_id = $1::uuid",
    )
    .bind(&o.w.ws)
    .fetch_one(&o.w.pool)
    .await
    .unwrap();
    assert_eq!(by, o.owner_id().await);
    assert_eq!(fingerprint.len(), 32);
    // Same command again: the stored answer, nothing new.
    let (s, again) =
        o.w.raw(Method::POST, "/activate", Some(body), &o.w.owner, &key)
            .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(again, first);
    // A new command on an active book: 409, still one journal and stock 10.
    let (s, b) = o.activate(&batches, &fp, &o.w.owner.clone()).await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert_eq!(b["code"], "already_active");
    assert_eq!(o.stock_level().await, "10.00");
    assert_eq!(o.count("journal_entries").await, 1);
    // Setup is read-only now.
    let setup = o.w.setup().await;
    let (s, _) =
        o.w.call(
            Method::PUT,
            "/setup",
            Some(
                json!({ "client_ref": Uuid::now_v7(), "expected_version": setup["version"],
                         "payload": { "timezone": "Europe/Paris" } }),
            ),
            &o.w.owner,
        )
        .await;
    assert!(s.is_client_error(), "{s}");
}

#[sqlx::test]
async fn changed_stale_or_unready_openings_write_nothing(pool: PgPool) {
    let (o, _, batches) = opened(pool.clone()).await;
    let fp = review(&o, &batches).await["opening_fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    // Step 5: another fingerprint is stale.
    let (s, b) = o
        .activate(&batches, &"0".repeat(64), &o.w.owner.clone())
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    // Stale setup version.
    let (s, b) =
        o.w.call(
            Method::POST,
            "/activate",
            Some(
                json!({ "client_ref": Uuid::now_v7(), "expected_version": 999,
                         "payload": { "batch_ids": batches, "opening_fingerprint": fp } }),
            ),
            &o.w.owner,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    // A later change (an open till shift) moves the review: the old fingerprint is stale and
    // the new review is not ready, so activation refuses with its issue.
    plant(
        &pool,
        "INSERT INTO accounting_exceptions (workspace_id, observation_id, kind, reason_code, message)
         VALUES ($1::uuid, uuidv7(), 'pending_posting', 'no_receivable', 'planted')
         RETURNING $2::uuid",
        &o.w.ws,
        o.owner_id().await,
    )
    .await;
    let (s, b) = o.activate(&batches, &fp, &o.w.owner.clone()).await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let r = review(&o, &batches).await;
    assert_eq!(codes(&r), ["unknown_money"]);
    let (s, b) = o
        .activate(
            &batches,
            r["opening_fingerprint"].as_str().unwrap(),
            &o.w.owner.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b.to_string().contains("unknown_money"), "{b}");
    assert_eq!(state(&o).await, "setup");
    for table in [
        "journal_entries",
        "accounting_open_items",
        "accounting_opening_baselines",
        "accounting_source_links",
        "cash_place_openings",
    ] {
        assert_eq!(o.count(table).await, 0, "{table}");
    }
    assert_eq!(o.stock_level().await, "0.00");
}

#[sqlx::test]
async fn activation_needs_its_permission_and_its_workspace(pool: PgPool) {
    let (o, _, batches) = opened(pool.clone()).await;
    let fp = review(&o, &batches).await["opening_fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    let configure = member_with(
        &o.w.app,
        &pool,
        &o.w.ws,
        "c@x.io",
        &["accounting.read", "accounting.configure", "accounting.post"],
    )
    .await;
    let (s, _) = o.activate(&batches, &fp, &configure).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (other, _) = crate::common::signup(&o.w.app, "bob@example.com", "bobco").await;
    let (s, _) = o.activate(&batches, &fp, &other).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(state(&o).await, "setup");
    let activator = member_with(
        &o.w.app,
        &pool,
        &o.w.ws,
        "a@x.io",
        &["accounting.read", "accounting.activate"],
    )
    .await;
    let (s, b) = o.activate(&batches, &fp, &activator).await;
    assert_eq!(s, StatusCode::OK, "{b}");
}

/// §14 step 6: each refusal the active book can't carry is a review issue that blocks.
#[sqlx::test]
async fn preflight_names_every_blocking_condition(pool: PgPool) {
    let (o, _, batches) = opened(pool.clone()).await;
    assert_eq!(codes(&review(&o, &batches).await), Vec::<String>::new());
    let (ws, owner) = (o.w.ws.clone(), o.owner_id().await);
    for sql in [
        // unknown money: an open online payment, a pending refund reservation
        "INSERT INTO order_payments (workspace_id, order_id, kind, provider, amount, currency, status)
         VALUES ($1::uuid, uuidv7(), 'payment', 'fake', 100, 'EUR', 'open') RETURNING $2::uuid",
        "INSERT INTO credit_refund_reservations (workspace_id, credit_item_id, source_kind, source_id, amount)
         VALUES ($1::uuid, uuidv7(), 'till_refund', uuidv7(), 1) RETURNING $2::uuid",
        // an accepted, unbooked purchase receipt
        "INSERT INTO purchase_receipt_lines (workspace_id, receipt_id, po_line_id, product_id,
                                             warehouse_id, base_quantity, effective_date, recorded_at)
         SELECT $1::uuid, uuidv7(), uuidv7(), uuidv7(), uuidv7(), 1, current_date, now()
         WHERE $2::uuid IS NOT NULL",
        // a supply transfer still on its way out
        "INSERT INTO supply_transfers (link_id, from_workspace_id, from_lot_id, from_warehouse_id,
                                       quantity, dispatched_by, to_workspace_id)
         VALUES (uuidv7(), $1::uuid, uuidv7(), uuidv7(), 1, $2, uuidv7())",
        // negative stock
        "UPDATE stocks SET stock_lvl = -1 WHERE workspace_id = $1::uuid AND $2::uuid IS NOT NULL",
        // an unmapped role
        "DELETE FROM account_mappings WHERE workspace_id = $1::uuid AND role = 'rounding'
           AND $2::uuid IS NOT NULL",
        // a second verified profile: every product now has two (ambiguous)
        "INSERT INTO tax_profiles SELECT (jsonb_populate_record(t, jsonb_build_object(
             'id', uuidv7(), 'lineage_id', uuidv7()))).* FROM tax_profiles t
         WHERE t.workspace_id = $1::uuid AND $2::uuid IS NOT NULL",
        // a gift card and a stocked size of the fixture product
        "INSERT INTO products SELECT (jsonb_populate_record(p, jsonb_build_object(
             'id', uuidv7(), 'type', 'giftcard', 'product_number', p.product_number + 1000))).* FROM products p
         WHERE p.workspace_id = $1::uuid AND $2::uuid IS NOT NULL",
        "INSERT INTO product_variants SELECT (jsonb_populate_record(v, jsonb_build_object(
             'id', uuidv7(), 'is_default', false, 'sku', NULL, 'barcode', NULL))).*
         FROM product_variants v JOIN products p ON p.id = v.product_id AND p.type = 'physical'
         WHERE v.workspace_id = $1::uuid AND v.is_default AND $2::uuid IS NOT NULL",
        // the cutover falls outside every open period
        "UPDATE accounting_settings SET cutover_at = '2031-06-01T00:00:00Z'
         WHERE workspace_id = $1::uuid AND $2::uuid IS NOT NULL",
    ] {
        plant(&pool, sql, &ws, owner).await;
    }
    let r = review(&o, &batches).await;
    assert_eq!(r["ready"], false);
    let got = codes(&r);
    for code in [
        "in_transit",
        "negative_stock",
        "period_missing",
        "receipts_unbooked",
        "role_mapping_missing",
        "sku_split_required",
        "tax_profile_missing",
        "unknown_money",
        "unsupported_products",
    ] {
        assert!(got.contains(&code.to_owned()), "{code} in {got:?}");
    }
    assert_eq!(r["cutover_date"], "2031-06-01");
}

#[sqlx::test]
async fn opening_baselines_list_what_the_opening_carried(pool: PgPool) {
    let (o, _, batches) = opened(pool.clone()).await;
    let (s, b) =
        o.w.call(Method::GET, "/opening-baselines", None, &o.w.owner)
            .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"], json!([]), "nothing before activation");
    let fp = o.open_books(&batches).await;
    o.go_active(&batches, &fp).await;
    let (_, b) =
        o.w.call(Method::GET, "/opening-baselines", None, &o.w.owner)
            .await;
    let rows = b["data"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{b}");
    let r = &rows[0];
    assert_eq!(r["source_kind"], "invoice");
    assert_eq!(r["number"], o.number.to_string());
    assert_eq!(
        (&r["face"], &r["settled"], &r["outstanding"]),
        (&json!("100.00"), &json!("40.00"), &json!("60.00"))
    );
    assert_eq!(r["corrected_outstanding"], "60.00");
    assert_eq!(r["corrections"], 0);
    let reader = member_with(&o.w.app, &pool, &o.w.ws, "r@x.io", &["accounting.read"]).await;
    let (s, _) =
        o.w.call(Method::GET, "/opening-baselines", None, &reader)
            .await;
    assert_eq!(s, StatusCode::OK);
    let (other, _) = crate::common::signup(&o.w.app, "bob@example.com", "bobco").await;
    let (s, _) =
        o.w.call(Method::GET, "/opening-baselines", None, &other)
            .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let nobody = member_with(&o.w.app, &pool, &o.w.ws, "n@x.io", &["product.list"]).await;
    let (s, _) =
        o.w.call(Method::GET, "/opening-baselines", None, &nobody)
            .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn opening_baselines_page_in_source_order(pool: PgPool) {
    let (o, _, batches) = opened(pool.clone()).await;
    let fp = o.open_books(&batches).await;
    o.go_active(&batches, &fp).await;
    // Second baseline: a settled payable cloned from the invoice row (sorts after invoices).
    sqlx::query(
        "INSERT INTO accounting_opening_baselines (workspace_id, line_id, source_kind, source_id,
             currency_id, face, settled, credited, refunded, outstanding, carrying, effect_id, coverage)
         SELECT workspace_id, line_id, 'supplier_bill', uuidv7(), currency_id, face, face, 0, 0,
                0, 0, effect_id, coverage
         FROM accounting_opening_baselines WHERE workspace_id = $1::uuid",
    )
    .bind(&o.w.ws)
    .execute(&pool)
    .await
    .unwrap();
    let get = |q: &'static str| {
        let w = &o.w;
        async move {
            let (s, b) = w
                .call(
                    Method::GET,
                    &format!("/opening-baselines{q}"),
                    None,
                    &w.owner,
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{b}");
            b
        }
    };
    let b = get("").await;
    assert_eq!(b["data"].as_array().unwrap().len(), 2, "{b}");
    assert_eq!(
        b["meta"],
        json!({"current_page": 1, "from": 1, "to": 2, "last_page": 1, "per_page": 50, "total": 2})
    );
    let b = get("?page_size=1").await;
    assert_eq!(b["data"][0]["source_kind"], "invoice", "{b}");
    assert_eq!(
        (&b["meta"]["total"], &b["meta"]["last_page"]),
        (&json!(2), &json!(2))
    );
    let b = get("?page_size=1&page=2").await;
    assert_eq!(b["data"].as_array().unwrap().len(), 1, "{b}");
    assert_eq!(b["data"][0]["source_kind"], "supplier_bill", "{b}");
    // Past the end: empty page, total still counted.
    let b = get("?page_size=1&page=3").await;
    assert_eq!(b["data"], json!([]), "{b}");
    assert_eq!(
        (&b["meta"]["total"], &b["meta"]["from"]),
        (&json!(2), &json!(null))
    );
    // Junk falls back to the defaults.
    let b = get("?page=x&page_size=-4").await;
    assert_eq!(
        (&b["meta"]["current_page"], &b["meta"]["per_page"]),
        (&json!(1), &json!(50))
    );
}
