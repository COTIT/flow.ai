//! Packing (raw lot -> child lot of jars, stock moved) and supply transfers over a live link
//! (R dispatches lot stock, A receives it into its own lot). Public farmers follow lineage.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{farm::Farm, join_workspace, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

struct Chain {
    f: Farm,
    raw: String,
    warehouse: String,
    jars_product: String,
}

/// Neema 5 kg + Baraka 2.5 kg of maize bought at TZS 8,000/kg; the raw lot holds 7.50 kg.
async fn chain(pool: PgPool) -> Chain {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let (_, baraka) = f.grower("Baraka", "0712000009", &c.maize).await;
    let n = f.actual(&c.neema.cycles[0], "100").await;
    let b = f.actual(&baraka, "100").await;
    let raw_product = f.product("Raw maize").await;
    let tzs = f.tzs().await;
    let batch = f
        .batch(
            "ILU-RAW-1",
            &c.maize,
            &raw_product,
            &tzs,
            800_000,
            &[(&n, "5"), (&b, "2.5")],
        )
        .await;
    let raw = batch["lot"]["id"].as_str().unwrap().to_owned();
    let warehouse = f.warehouse("Iringa store").await;
    f.lot_stock(&raw, &warehouse, "7.50").await;
    let jars_product = f.product("Maize flour 750 g").await;
    Chain {
        f,
        raw,
        warehouse,
        jars_product,
    }
}

fn packing(c: &Chain, code: &str, units: &str, raw_kg: &str) -> Value {
    json!({ "product_id": c.jars_product, "code": code, "units": units, "raw_kg": raw_kg,
            "warehouse_id": c.warehouse, "best_before": "2027-09-30" })
}

async fn pack(c: &Chain, lot: &str, body: Value) -> (StatusCode, Value) {
    c.f.ws_call(
        Method::POST,
        &format!("/lots/{lot}/packings"),
        Some(body),
        &c.f.owner,
    )
    .await
}

/// Live stock level of `lot` (sum over its rows), as text.
async fn level(pool: &PgPool, lot: &str) -> String {
    sqlx::query_scalar(
        "SELECT coalesce(sum(stock_lvl), 0)::numeric(19,2)::text FROM stocks
         WHERE lot_id = $1::uuid AND deleted_at IS NULL",
    )
    .bind(lot)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test]
async fn packing_turns_raw_kg_into_a_child_lot_of_jars(pool: PgPool) {
    let c = chain(pool).await;

    let (status, res) = pack(&c, &c.raw, packing(&c, "ILU-J-1", "10", "7.50")).await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    let jar = res["data"]["id"].as_str().unwrap().to_owned();
    assert_eq!(res["data"]["parent_lot_id"], c.raw.as_str());
    assert_eq!(res["data"]["pack_raw_kg"], "7.50");
    assert_eq!(res["data"]["pack_units"], "10.00");
    assert_eq!(res["data"]["best_before"], "2027-09-30");
    assert_eq!(level(&c.f.pool, &c.raw).await, "0.00");
    assert_eq!(level(&c.f.pool, &jar).await, "10.00");

    let (_, events) =
        c.f.ws_call(
            Method::GET,
            &format!("/lots/{}/events", c.raw),
            None,
            &c.f.owner,
        )
        .await;
    assert_eq!(events["data"][0]["kind"], "packed", "{events}");
}

#[sqlx::test]
async fn packing_is_refused_when_it_cannot_be_traced_or_stocked(pool: PgPool) {
    let c = chain(pool).await;
    let (status, res) = pack(&c, &c.raw, packing(&c, "ILU-J-2", "10", "7.51")).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "more kg than the lot holds: {res}"
    );
    assert!(res["errors"]["raw_kg"].is_array(), "{res}");

    for (field, bad) in [
        ("units", json!("0")),
        ("raw_kg", json!("0")),
        ("units", json!("1.001")),
    ] {
        let mut b = packing(&c, "ILU-J-3", "10", "1");
        b[field] = bad;
        let (status, res) = pack(&c, &c.raw, b).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        assert!(res["errors"][field].is_array(), "{field}: {res}");
    }

    let (_, res) = pack(&c, &c.raw, packing(&c, "ILU-J-4", "4", "3")).await;
    let jar = res["data"]["id"].as_str().unwrap().to_owned();
    let (status, res) = pack(&c, &jar, packing(&c, "ILU-J-5", "1", "1")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], "A packed lot can't be packed again.");

    let (_, plain) =
        c.f.ws_call(
            Method::POST,
            "/lots",
            Some(json!({ "product_id": c.jars_product, "code": "PLAIN-1" })),
            &c.f.owner,
        )
        .await;
    let plain = plain["data"]["id"].as_str().unwrap();
    let (status, res) = pack(&c, plain, packing(&c, "ILU-J-6", "1", "1")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["message"],
        "Only a lot from a supply batch or a supply transfer can be packed."
    );

    let other = Farm::new_named(c.f.pool.clone(), "else.test", "elsewhere").await;
    let theirs = other.product("Their jars").await;
    let mut b = packing(&c, "ILU-J-7", "1", "1");
    b["product_id"] = json!(theirs);
    let (status, res) = pack(&c, &c.raw, b).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["product_id"].is_array());
    let (status, _) = other
        .ws_call(
            Method::POST,
            &format!("/lots/{}/packings", c.raw),
            Some(packing(&c, "X", "1", "1")),
            &other.owner,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "another workspace's lot");
}

struct Shop {
    a: String,
    a_ws: String,
    link: String,
}

/// AFM workspace with a live link from R carrying `fields`.
async fn shop(c: &Chain, fields: Value) -> Shop {
    let (a, a_ws) = signup(&c.f.app, "owner@afm.test", "afm").await;
    let (status, res) =
        c.f.ws_call(
            Method::POST,
            "/supply-links",
            Some(json!({ "to_workspace_id": a_ws, "fields": fields })),
            &c.f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let link = res["data"]["id"].as_str().unwrap().to_owned();
    let (status, _) = a_call(
        &c.f,
        &a_ws,
        Method::POST,
        &format!("/supply-links/{link}/accept"),
        None,
        &a,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    Shop { a, a_ws, link }
}

async fn a_call(
    f: &Farm,
    ws: &str,
    method: Method,
    tail: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    send(
        &f.app,
        method,
        &format!("/v1/workspace/{ws}{tail}"),
        body,
        Some(token),
    )
    .await
}

async fn a_create(f: &Farm, s: &Shop, tail: &str, body: Value, key: &str) -> String {
    let (status, res) = a_call(f, &s.a_ws, Method::POST, tail, Some(body), &s.a).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"][key].as_str().unwrap().to_owned()
}

async fn dispatch(c: &Chain, lot: &str, link: &str, qty: &str) -> (StatusCode, Value) {
    c.f.ws_call(
        Method::POST,
        &format!("/lots/{lot}/transfers"),
        Some(json!({ "link_id": link, "warehouse_id": c.warehouse, "quantity": qty })),
        &c.f.owner,
    )
    .await
}

#[sqlx::test]
async fn r_dispatches_over_a_live_link_and_a_receives_into_its_own_lot(pool: PgPool) {
    let c = chain(pool).await;
    let (_, res) = pack(&c, &c.raw, packing(&c, "ILU-J-1", "10", "7.50")).await;
    let jar = res["data"]["id"].as_str().unwrap().to_owned();
    let s = shop(&c, json!(["village", "harvest_window", "farmer_count"])).await;

    let (status, res) = dispatch(&c, &jar, &s.link, "4").await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let transfer = res["data"]["id"].as_str().unwrap().to_owned();
    assert_eq!(res["data"]["status"], "dispatched");
    assert_eq!(level(&c.f.pool, &jar).await, "6.00");

    let (_, list) = a_call(&c.f, &s.a_ws, Method::GET, "/supply-transfers", None, &s.a).await;
    let t = &list["data"][0];
    assert_eq!(t["id"], transfer.as_str());
    assert_eq!(t["direction"], "incoming");
    assert_eq!(t["quantity"], "4.00");
    assert_eq!(t["from_lot"]["code"], "ILU-J-1");
    let text = list.to_string();
    for private in ["Neema", "Baraka", "800000", "person"] {
        assert!(!text.contains(private), "{private} leaked: {text}");
    }

    let a_product = a_create(
        &c.f,
        &s,
        "/products",
        json!({ "name": "AFM maize flour", "price": 10 }),
        "product_id",
    )
    .await;
    let a_wh = a_create(
        &c.f,
        &s,
        "/warehouses",
        json!({ "name": "US hub" }),
        "warehouse_id",
    )
    .await;
    let receive = json!({ "product_id": a_product, "code": "AFM-ILU-1", "warehouse_id": a_wh, "quantity": "4" });
    let tail = format!("/supply-transfers/{transfer}/receive");
    let (status, res) = a_call(
        &c.f,
        &s.a_ws,
        Method::POST,
        &tail,
        Some(receive.clone()),
        &s.a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["status"], "received");
    let a_lot = res["data"]["to_lot"]["id"].as_str().unwrap().to_owned();
    assert_eq!(level(&c.f.pool, &a_lot).await, "4.00");
    let (status, _) = a_call(&c.f, &s.a_ws, Method::POST, &tail, Some(receive), &s.a).await;
    assert_eq!(status, StatusCode::CONFLICT, "received once");

    // A's page shows R's farmers by the allow-list; 2 farmers: no count.
    let base = format!("/lots/{a_lot}/passport");
    a_call(
        &c.f,
        &s.a_ws,
        Method::PUT,
        &base,
        Some(json!({ "title": "Maize" })),
        &s.a,
    )
    .await;
    a_call(
        &c.f,
        &s.a_ws,
        Method::POST,
        &format!("{base}/publish"),
        None,
        &s.a,
    )
    .await;
    let (_, lot) = a_call(
        &c.f,
        &s.a_ws,
        Method::GET,
        &format!("/lots/{a_lot}"),
        None,
        &s.a,
    )
    .await;
    let code = lot["data"]["public_code"].as_str().unwrap();
    let (status, page) = send(
        &c.f.app,
        Method::GET,
        &format!("/v1/public/lots/{code}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(
        page["data"]["farmers"],
        json!([{ "village": "Ilundo", "farmers": null,
                 "harvest_start": "2026-09-01", "harvest_end": "2026-09-30" }])
    );
}

/// R with a 3-farmer batch (counts show), and A receiving 1 kg of it over a link carrying
/// `fields`: A's published lot's public `farmers`, before and after R revokes the link.
async fn as_page(pool: PgPool, fields: Value) -> (Value, Value) {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let (_, b) = f.grower("Baraka", "0712000009", &c.maize).await;
    let (_, u) = f.grower("Upendo", "0712000010", &c.maize).await;
    let hs = [
        f.actual(&c.neema.cycles[0], "10").await,
        f.actual(&b, "10").await,
        f.actual(&u, "10").await,
    ];
    let raw_product = f.product("Raw maize").await;
    let tzs = f.tzs().await;
    let batch = f
        .batch(
            "ILU-3",
            &c.maize,
            &raw_product,
            &tzs,
            800_000,
            &[(&hs[0], "1"), (&hs[1], "1"), (&hs[2], "1")],
        )
        .await;
    let raw = batch["lot"]["id"].as_str().unwrap().to_owned();
    let warehouse = f.warehouse("Iringa store").await;
    f.lot_stock(&raw, &warehouse, "3").await;
    let chain = Chain {
        f,
        raw,
        warehouse,
        jars_product: String::new(),
    };
    let s = shop(&chain, fields).await;
    let (_, res) = dispatch(&chain, &chain.raw, &s.link, "1").await;
    let transfer = res["data"]["id"].as_str().unwrap().to_owned();
    let f = &chain.f;
    let a_product = a_create(
        f,
        &s,
        "/products",
        json!({ "name": "AFM raw", "price": 10 }),
        "product_id",
    )
    .await;
    let a_wh = a_create(
        f,
        &s,
        "/warehouses",
        json!({ "name": "US hub" }),
        "warehouse_id",
    )
    .await;
    let (_, res) = a_call(
        f,
        &s.a_ws,
        Method::POST,
        &format!("/supply-transfers/{transfer}/receive"),
        Some(json!({ "product_id": a_product, "code": "AFM-RAW-1", "warehouse_id": a_wh, "quantity": "1" })),
        &s.a,
    )
    .await;
    let a_lot = res["data"]["to_lot"]["id"].as_str().unwrap().to_owned();
    let base = format!("/lots/{a_lot}/passport");
    a_call(
        f,
        &s.a_ws,
        Method::PUT,
        &base,
        Some(json!({ "title": "Raw" })),
        &s.a,
    )
    .await;
    a_call(
        f,
        &s.a_ws,
        Method::POST,
        &format!("{base}/publish"),
        None,
        &s.a,
    )
    .await;
    let (_, lot) = a_call(
        f,
        &s.a_ws,
        Method::GET,
        &format!("/lots/{a_lot}"),
        None,
        &s.a,
    )
    .await;
    let url = format!(
        "/v1/public/lots/{}",
        lot["data"]["public_code"].as_str().unwrap()
    );
    let (_, before) = send(&f.app, Method::GET, &url, None, None).await;
    let (status, _) = f
        .ws_call(
            Method::DELETE,
            &format!("/supply-links/{}", s.link),
            None,
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // Even if the link's list changed afterwards, the received lot keeps the frozen copy.
    sqlx::query("UPDATE supply_links SET fields = '{}' WHERE id = $1::uuid")
        .bind(&s.link)
        .execute(&f.pool)
        .await
        .unwrap();
    let (_, after) = send(&f.app, Method::GET, &url, None, None).await;
    (
        before["data"]["farmers"].clone(),
        after["data"]["farmers"].clone(),
    )
}

#[sqlx::test]
async fn all_fields_granted_show_village_count_and_window(pool: PgPool) {
    let (page, after_revoke) =
        as_page(pool, json!(["village", "farmer_count", "harvest_window"])).await;
    let full = json!([{ "village": "Ilundo", "farmers": 3,
                        "harvest_start": "2026-09-01", "harvest_end": "2026-09-30" }]);
    assert_eq!(page, full);
    assert_eq!(
        after_revoke, full,
        "frozen when received: a revoke changes nothing shown"
    );
}

#[sqlx::test]
async fn without_village_nothing_is_shown(pool: PgPool) {
    let (page, _) = as_page(pool, json!(["farmer_count", "harvest_window"])).await;
    assert_eq!(page, json!([]));
}

#[sqlx::test]
async fn without_farmer_count_the_count_is_null(pool: PgPool) {
    let (page, _) = as_page(pool, json!(["village", "harvest_window"])).await;
    assert_eq!(
        page,
        json!([{ "village": "Ilundo", "farmers": null,
                 "harvest_start": "2026-09-01", "harvest_end": "2026-09-30" }])
    );
}

#[sqlx::test]
async fn without_harvest_window_the_dates_are_null(pool: PgPool) {
    let (page, _) = as_page(pool, json!(["village", "farmer_count"])).await;
    assert_eq!(
        page,
        json!([{ "village": "Ilundo", "farmers": 3, "harvest_start": null, "harvest_end": null }])
    );
}

#[sqlx::test]
async fn transfers_need_a_live_link_and_each_side_does_only_its_part(pool: PgPool) {
    let c = chain(pool).await;
    let (status, res) = dispatch(&c, &c.raw, "01900000-0000-7000-8000-000000000001", "1").await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "no such link: {res}"
    );
    assert!(res["errors"]["link_id"].is_array());

    let s = shop(&c, json!([])).await;
    let (status, res) = dispatch(&c, &c.raw, &s.link, "7.51").await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "more than the lot holds: {res}"
    );
    let (status, _) = a_call(
        &c.f,
        &s.a_ws,
        Method::POST,
        &format!("/lots/{}/transfers", c.raw),
        Some(json!({ "link_id": s.link, "warehouse_id": c.warehouse, "quantity": "1" })),
        &s.a,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "A can't dispatch R's lot");

    let (_, res) = dispatch(&c, &c.raw, &s.link, "2").await;
    let transfer = res["data"]["id"].as_str().unwrap().to_owned();
    let tail = format!("/supply-transfers/{transfer}/receive");
    let r_product = c.jars_product.clone();
    let body = json!({ "product_id": r_product, "code": "R-SELF", "warehouse_id": c.warehouse, "quantity": "2" });
    let (status, _) =
        c.f.ws_call(Method::POST, &tail, Some(body), &c.f.owner)
            .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "R can't receive its own dispatch"
    );

    let (third, third_ws) = signup(&c.f.app, "owner@third.test", "third").await;
    let shaped = json!({ "product_id": c.jars_product, "code": "X-1", "warehouse_id": c.warehouse, "quantity": "1" });
    let (status, _) = a_call(&c.f, &third_ws, Method::POST, &tail, Some(shaped), &third).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a third workspace");
    let (_, list) = a_call(
        &c.f,
        &third_ws,
        Method::GET,
        "/supply-transfers",
        None,
        &third,
    )
    .await;
    assert_eq!(list["data"], json!([]));

    let (clerk, _) = signup(&c.f.app, "clerk@afm.test", "clerk-own").await;
    join_workspace(&c.f.pool, &s.a_ws, "clerk@afm.test").await;
    let (status, _) = a_call(&c.f, &s.a_ws, Method::POST, &tail, Some(json!({})), &clerk).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a member without product.update"
    );

    let a_product = a_create(
        &c.f,
        &s,
        "/products",
        json!({ "name": "AFM raw", "price": 10 }),
        "product_id",
    )
    .await;
    let a_wh = a_create(
        &c.f,
        &s,
        "/warehouses",
        json!({ "name": "US hub" }),
        "warehouse_id",
    )
    .await;
    let too_much = json!({ "product_id": a_product, "code": "AFM-1", "warehouse_id": a_wh, "quantity": "2.01" });
    let (status, res) = a_call(&c.f, &s.a_ws, Method::POST, &tail, Some(too_much), &s.a).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["quantity"].is_array());

    let (status, _) = a_call(
        &c.f,
        &s.a_ws,
        Method::DELETE,
        &format!("/supply-links/{}", s.link),
        None,
        &s.a,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let ok =
        json!({ "product_id": a_product, "code": "AFM-1", "warehouse_id": a_wh, "quantity": "2" });
    let (status, res) = a_call(&c.f, &s.a_ws, Method::POST, &tail, Some(ok), &s.a).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "revoked link: {res}");
    let (status, _) = dispatch(&c, &c.raw, &s.link, "1").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "revoked link");
}

/// POST with an Idempotency-Key, as raw bytes so a replay can be compared byte for byte.
async fn keyed(
    f: &Farm,
    ws: &str,
    tail: &str,
    body: Value,
    token: &str,
    key: &str,
) -> (StatusCode, axum::body::Bytes) {
    common::send_raw(
        &f.app,
        Method::POST,
        &format!("/v1/workspace/{ws}{tail}"),
        Some(body),
        Some(token),
        &[("idempotency-key", key)],
    )
    .await
}

/// A dispatch retried after a lost reply sends once: one transfer, one stock movement, one
/// event, the same answer. Another body under the key is refused; another workspace's key is
/// its own.
#[sqlx::test]
async fn a_retried_dispatch_sends_once(pool: PgPool) {
    let c = chain(pool).await;
    let (_, res) = pack(&c, &c.raw, packing(&c, "ILU-J-1", "10", "7.50")).await;
    let jar = res["data"]["id"].as_str().unwrap().to_owned();
    let s = shop(&c, json!(["village"])).await;
    let tail = format!("/lots/{jar}/transfers");
    let body = json!({ "link_id": s.link, "warehouse_id": c.warehouse, "quantity": "4" });

    let first = keyed(&c.f, &c.f.ws, &tail, body.clone(), &c.f.owner, "d-1").await;
    assert_eq!(first.0, StatusCode::CREATED);
    let again = keyed(&c.f, &c.f.ws, &tail, body.clone(), &c.f.owner, "d-1").await;
    assert_eq!(again, first, "byte-identical replay");
    assert_eq!(level(&c.f.pool, &jar).await, "6.00", "stock taken once");
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM supply_transfers WHERE from_lot_id = $1::uuid),
                (SELECT count(*) FROM stock_movements WHERE lot_id = $1::uuid AND related_domain = 'supply_transfer'),
                (SELECT count(*) FROM lot_events WHERE lot_id = $1::uuid AND kind = 'dispatched')",
    )
    .bind(&jar)
    .fetch_one(&c.f.pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1, 1));

    let other = json!({ "link_id": s.link, "warehouse_id": c.warehouse, "quantity": "1" });
    let (status, bytes) = keyed(&c.f, &c.f.ws, &tail, other, &c.f.owner, "d-1").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["message"],
        "Idempotency-Key reused with different request payload"
    );
    // The shop's workspace under the same key gets its own answer: R's lot is not there.
    let (status, _) = keyed(&c.f, &s.a_ws, &tail, body, &s.a, "d-1").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no replay across workspaces");
    assert_eq!(level(&c.f.pool, &jar).await, "6.00");
}

/// Farm ops and admin ship stock without `product.update`; a field officer or a plain member
/// may not.
#[sqlx::test]
async fn farm_ops_dispatch_and_plain_members_do_not(pool: PgPool) {
    let c = chain(pool).await;
    let (_, res) = pack(&c, &c.raw, packing(&c, "ILU-J-1", "10", "7.50")).await;
    let jar = res["data"]["id"].as_str().unwrap().to_owned();
    let s = shop(&c, json!(["village"])).await;
    let tail = format!("/lots/{jar}/transfers");
    let body = json!({ "link_id": s.link, "warehouse_id": c.warehouse, "quantity": "1" });

    let (status, res) =
        c.f.ws_call(Method::POST, &tail, Some(body.clone()), &c.f.ops)
            .await;
    assert_eq!(status, StatusCode::CREATED, "farm ops ship: {res}");
    let plain = c.f.member("plain@ruaha.test").await;
    for token in [&plain, &c.f.officer_ilu] {
        let (status, _) =
            c.f.ws_call(Method::POST, &tail, Some(body.clone()), token)
                .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    assert_eq!(level(&c.f.pool, &jar).await, "9.00");
    // Sharing farmer data stays with workspace admins: farm ops may not grant a link.
    let (status, _) =
        c.f.ws_call(
            Method::POST,
            "/supply-links",
            Some(json!({ "to_workspace_id": s.a_ws, "fields": ["village"] })),
            &c.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn the_transfer_list_filters_by_direction_and_status(pool: PgPool) {
    let c = chain(pool).await;
    let (_, res) = pack(&c, &c.raw, packing(&c, "ILU-J-1", "10", "7.50")).await;
    let jar = res["data"]["id"].as_str().unwrap().to_owned();
    let s = shop(&c, json!(["village"])).await;
    let (_, res) = dispatch(&c, &jar, &s.link, "4").await;
    let transfer = res["data"]["id"].as_str().unwrap().to_owned();
    let total = |ws: String, token: String, q: &'static str| {
        let f = &c.f;
        async move {
            let tail = format!("/supply-transfers{q}");
            let (status, res) = a_call(f, &ws, Method::GET, &tail, None, &token).await;
            assert_eq!(status, StatusCode::OK, "{q}: {res}");
            res["meta"]["total"].clone()
        }
    };
    let (a, a_ws) = (s.a.clone(), s.a_ws.clone());
    let (r, r_ws) = (c.f.owner.clone(), c.f.ws.clone());

    let arriving = "?direction=incoming&status=dispatched";
    assert_eq!(
        total(a_ws.clone(), a.clone(), arriving).await,
        1,
        "A's arriving"
    );
    assert_eq!(total(r_ws.clone(), r.clone(), arriving).await, 0);
    assert_eq!(
        total(
            r_ws.clone(),
            r.clone(),
            "?direction=outgoing&status=dispatched"
        )
        .await,
        1
    );
    assert_eq!(total(a_ws.clone(), a.clone(), "?status=received").await, 0);

    let a_product = a_create(
        &c.f,
        &s,
        "/products",
        json!({ "name": "Jar", "price": 10 }),
        "product_id",
    )
    .await;
    let a_wh = a_create(
        &c.f,
        &s,
        "/warehouses",
        json!({ "name": "Hub" }),
        "warehouse_id",
    )
    .await;
    let body =
        json!({ "product_id": a_product, "code": "AFM-1", "warehouse_id": a_wh, "quantity": "4" });
    let tail = format!("/supply-transfers/{transfer}/receive");
    let (status, _) = a_call(&c.f, &a_ws, Method::POST, &tail, Some(body), &a).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(total(a_ws.clone(), a.clone(), arriving).await, 0);
    assert_eq!(total(a_ws.clone(), a.clone(), "?status=received").await, 1);
    assert_eq!(
        total(
            r_ws.clone(),
            r.clone(),
            "?direction=outgoing&status=received"
        )
        .await,
        1
    );

    let (third, third_ws) = signup(&c.f.app, "owner@third.test", "third").await;
    assert_eq!(
        total(third_ws.clone(), third.clone(), "?status=received").await,
        0
    );
    assert_eq!(total(third_ws, third, "?direction=incoming").await, 0);

    for q in ["?status=sent", "?direction=both"] {
        let (status, res) = a_call(
            &c.f,
            &a_ws,
            Method::GET,
            &format!("/supply-transfers{q}"),
            None,
            &a,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {res}");
    }
}

/// Farm ops dispatch from Field without office rights: one farm-guarded read gives the live
/// outgoing links, the batch lots and the warehouses holding their free stock; the dispatch
/// itself is open to ops.
#[sqlx::test]
async fn farm_ops_read_dispatch_options_and_dispatch(pool: PgPool) {
    let c = chain(pool).await;
    let s = shop(&c, json!(["village"])).await;
    // A pending link to a third shop is no dispatch target.
    let (_, third_ws) = signup(&c.f.app, "owner@third.test", "third").await;
    let (status, _) =
        c.f.ws_call(
            Method::POST,
            "/supply-links",
            Some(json!({ "to_workspace_id": third_ws, "fields": [] })),
            &c.f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let empty = c.f.warehouse("Empty shed").await;

    let (status, res) =
        c.f.call(Method::GET, "/dispatch/options", None, &c.f.ops)
            .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(
        d["links"],
        json!([{ "id": s.link, "partner": { "id": s.a_ws, "name": "afm" } }]),
        "live outgoing links only"
    );
    assert_eq!(d["lots"][0]["id"], c.raw.as_str());
    assert_eq!(d["lots"][0]["code"], "ILU-RAW-1");
    let warehouses: Vec<&str> = d["warehouses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        warehouses,
        [c.warehouse.as_str()],
        "only where batch lots have free stock"
    );
    assert!(!warehouses.contains(&empty.as_str()));

    // The office reads stay closed to farm ops; the farm read is enough.
    let (status, _) =
        c.f.ws_call(Method::GET, "/supply-links", None, &c.f.ops)
            .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, res) =
        c.f.ws_call(
            Method::POST,
            &format!("/lots/{}/transfers", c.raw),
            Some(json!({ "link_id": s.link, "warehouse_id": c.warehouse, "quantity": "1" })),
            &c.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "ops dispatch: {res}");

    for (token, why) in [
        (&c.f.officer_ilu, "field officer"),
        (&c.f.officer_mga, "officer"),
    ] {
        let (status, _) =
            c.f.call(Method::GET, "/dispatch/options", None, token)
                .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{why}");
    }
    let farmer = c.f.farmer("kim@ruaha.test", &c.f.ilundo).await;
    let (status, _) =
        c.f.call(Method::GET, "/dispatch/options", None, &farmer)
            .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "farmer");
    let (status, res) = a_call(
        &c.f,
        &s.a_ws,
        Method::GET,
        "/farm/dispatch/options",
        None,
        &s.a,
    )
    .await;
    assert_ne!(status, StatusCode::OK, "the shop has no farm: {res}");
    let (outsider, _) = signup(&c.f.app, "out@else.test", "outside").await;
    let (status, _) =
        c.f.call(Method::GET, "/dispatch/options", None, &outsider)
            .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "not a member");
}
