//! Done when: an Ilundo harvest bought at an agreed price per kg becomes a lot of jars; a sale
//! of those jars credits each farmer's passbook exactly (price × their kg share of the jars
//! sold, largest remainder), a return debits the same split back, a full return nets every
//! farmer to 0, and the farmer is paid from the balance under four eyes. Run with the farm
//! and the shop in two workspaces (over a supply link) and in one.

mod common;

use axum::http::{Method, StatusCode};
use chrono::{FixedOffset, Utc};
use common::{
    farm::{Farm, client_ref},
    send, signup,
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// TZS 8,000.00 per kg.
const PRICE: i64 = 800_000;

struct Sold {
    f: Farm,
    /// Workspace and owner token that sell (A, or R itself).
    shop_ws: String,
    shop: String,
    order: String,
    /// Persons, in the order Neema (250 kg), Baraka (120.50 kg), Upendo (30 kg).
    persons: [String; 3],
    tzs: String,
    /// The shop's jar product and the warehouse holding the jars.
    product: String,
    warehouse: String,
}

async fn ws_call(
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

async fn created(f: &Farm, ws: &str, tail: &str, body: Value, token: &str, key: &str) -> String {
    let (status, res) = ws_call(f, ws, Method::POST, tail, Some(body), token).await;
    assert_eq!(status, StatusCode::CREATED, "{tail}: {res}");
    res["data"][key].as_str().unwrap().to_owned()
}

/// A completed order of 3 jars from a lot packed 10 jars from 7.50 kg of a 400.50 kg batch.
async fn sold(pool: PgPool, two_workspaces: bool) -> Sold {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let (baraka, b_cycle) = f.grower("Baraka", "0712000009", &c.maize).await;
    let (upendo, u_cycle) = f.grower("Upendo", "0712000010", &c.maize).await;
    let hs = [
        f.actual(&c.neema.cycles[0], "300").await,
        f.actual(&b_cycle, "200").await,
        f.actual(&u_cycle, "50").await,
    ];
    let raw_product = f.product("Raw honey").await;
    let tzs = f.tzs().await;
    let batch = f
        .batch(
            "ILU-H-1",
            &c.maize,
            &raw_product,
            &tzs,
            PRICE,
            &[(&hs[0], "250"), (&hs[1], "120.50"), (&hs[2], "30")],
        )
        .await;
    let raw = batch["lot"]["id"].as_str().unwrap().to_owned();
    let store = created(
        &f,
        &f.ws,
        "/warehouses",
        json!({ "name": "Iringa", "default_warehouse": true, "is_store": true }),
        &f.owner,
        "warehouse_id",
    )
    .await;
    f.lot_stock(&raw, &store, "400.50").await;
    let jar_product = f.product("Honey 750 g").await;
    let (status, res) = f
        .ws_call(
            Method::POST,
            &format!("/lots/{raw}/packings"),
            Some(
                json!({ "product_id": jar_product, "code": "ILU-J-1", "units": "10",
                         "raw_kg": "7.50", "warehouse_id": store }),
            ),
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let jars = res["data"]["id"].as_str().unwrap().to_owned();

    let (shop_ws, shop, product, warehouse) = if two_workspaces {
        let (a, a_ws) = signup(&f.app, "owner@afm.test", "afm").await;
        let link = created(
            &f,
            &f.ws,
            "/supply-links",
            json!({ "to_workspace_id": a_ws, "fields": ["village"] }),
            &f.owner,
            "id",
        )
        .await;
        let (status, _) = ws_call(
            &f,
            &a_ws,
            Method::POST,
            &format!("/supply-links/{link}/accept"),
            None,
            &a,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let transfer = created(
            &f,
            &f.ws,
            &format!("/lots/{jars}/transfers"),
            json!({ "link_id": link, "warehouse_id": store, "quantity": "10" }),
            &f.owner,
            "id",
        )
        .await;
        let hub = created(
            &f,
            &a_ws,
            "/warehouses",
            json!({ "name": "US hub", "default_warehouse": true, "is_store": true }),
            &a,
            "warehouse_id",
        )
        .await;
        let a_jar = created(
            &f,
            &a_ws,
            "/products",
            json!({ "name": "Ilundo honey", "price": "24.00" }),
            &a,
            "product_id",
        )
        .await;
        let (status, res) = ws_call(
            &f,
            &a_ws,
            Method::POST,
            &format!("/supply-transfers/{transfer}/receive"),
            Some(json!({ "product_id": a_jar, "code": "AFM-ILU-1", "warehouse_id": hub, "quantity": "10" })),
            &a,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        (a_ws, a, a_jar, hub)
    } else {
        (f.ws.clone(), f.owner.clone(), jar_product, store)
    };

    // Both workspaces listen for sale.attributed; only the farm's may get it.
    for ws in [&f.ws, &shop_ws] {
        sqlx::query(
            "INSERT INTO webhook_endpoints (workspace_id, url, events, secret_sealed, secret_key_id)
             VALUES ($1::uuid, 'https://hooks.example.com/h', '{sale.attributed}', '\\x00', 'k')",
        )
        .bind(ws)
        .execute(&f.pool)
        .await
        .unwrap();
    }
    let order = created(
        &f,
        &shop_ws,
        "/orders",
        json!({ "lines": [{ "product_id": product, "quantity": "3" }] }),
        &shop,
        "order_id",
    )
    .await;
    act(&f, &shop_ws, &shop, &order, "confirm", None).await;
    let lines = order_lines(&f, &shop_ws, &shop, &order).await;
    act(
        &f,
        &shop_ws,
        &shop,
        &order,
        "pick",
        Some(json!({ "lines": [{ "line_id": lines[0], "quantity": "3" }] })),
    )
    .await;
    act(&f, &shop_ws, &shop, &order, "complete", None).await;
    Sold {
        f,
        shop_ws,
        shop,
        order,
        persons: [c.neema.person, baraka, upendo],
        tzs,
        product,
        warehouse,
    }
}

/// A till signed in on a register of the jars' warehouse; its token.
async fn till(s: &Sold) -> String {
    let (status, reg) = ws_call(
        &s.f,
        &s.shop_ws,
        Method::POST,
        "/till/registers",
        Some(json!({ "name": "Till", "warehouse_id": s.warehouse })),
        &s.shop,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reg}");
    let key = reg["data"]["key"].as_str().unwrap().to_owned();
    let (_, user) = ws_call(
        &s.f,
        &s.shop_ws,
        Method::POST,
        "/till/users",
        Some(json!({ "name": "Clerk", "pin": "4821" })),
        &s.shop,
    )
    .await;
    let (status, res) = ws_call(
        &s.f,
        &s.shop_ws,
        Method::POST,
        "/till/login",
        Some(json!({ "till_user_id": user["data"]["id"], "pin": "4821" })),
        &key,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"]["token"].as_str().unwrap().to_owned()
}

async fn act(f: &Farm, ws: &str, token: &str, order: &str, action: &str, body: Option<Value>) {
    let (status, res) = ws_call(
        f,
        ws,
        Method::POST,
        &format!("/orders/{order}/{action}"),
        body,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{action}: {res}");
}

async fn order_lines(f: &Farm, ws: &str, token: &str, order: &str) -> Vec<String> {
    let (_, o) = ws_call(f, ws, Method::GET, &format!("/orders/{order}"), None, token).await;
    o["data"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap().to_owned())
        .collect()
}

async fn give_back(s: &Sold, jars: &str) {
    let line = order_lines(&s.f, &s.shop_ws, &s.shop, &s.order)
        .await
        .remove(0);
    act(
        &s.f,
        &s.shop_ws,
        &s.shop,
        &s.order,
        "returns",
        Some(json!({ "lines": [{ "line_id": line, "quantity": jars }] })),
    )
    .await;
}

/// Each farmer's passbook balance (minor units), read through the API by ops.
async fn balances(s: &Sold) -> [i64; 3] {
    let mut out = [0; 3];
    for (i, p) in s.persons.iter().enumerate() {
        let (status, res) =
            s.f.call(
                Method::GET,
                &format!("/persons/{p}/passbook"),
                None,
                &s.f.ops,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        out[i] = res["data"][0]["balance_after"]["minor"]
            .as_i64()
            .unwrap_or(0);
    }
    out
}

// 3 jars × 7.50 kg / 10 = 2.25 kg × TZS 8,000.00 = 1,800,000 minor, split 250 : 120.50 : 30
// = 1,123,595.51 / 541,573.03 / 134,831.46 -> the leftover unit to Neema.
const SALE: [i64; 3] = [1_123_596, 541_573, 134_831];
// 1 jar = 600,000: 374,531.83 / 180,524.34 / 44,943.82 -> leftovers to Neema and Upendo.
const ONE_BACK: [i64; 3] = [374_532, 180_524, 44_944];
// A till sale of 5 jars = 3,000,000: 1,872,659.18 / 902,621.72 / 224,719.10 -> Baraka.
const TILL: [i64; 3] = [1_872_659, 902_622, 224_719];

async fn done_when(pool: PgPool, two_workspaces: bool) {
    let s = sold(pool, two_workspaces).await;
    assert_eq!(balances(&s).await, SALE);
    assert_eq!(SALE.iter().sum::<i64>(), 1_800_000);

    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT kind, total, workspace_id::text FROM farm_sale_attributions ORDER BY created_at",
    )
    .fetch_all(&s.f.pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        [("sale".to_owned(), 1_800_000, s.f.ws.clone())],
        "kept in the farm workspace"
    );

    give_back(&s, "1").await;
    let after_one = balances(&s).await;
    for i in 0..3 {
        assert_eq!(after_one[i], SALE[i] - ONE_BACK[i], "farmer {i}");
    }
    give_back(&s, "2").await;
    assert_eq!(
        balances(&s).await,
        [0, 0, 0],
        "a full return nets every farmer to 0"
    );

    // The till sells 5 jars, the 3 returned ones among them (back in their lot): all 5 are
    // credited. An offline replay credits nothing more.
    let t = till(&s).await;
    let body = json!({ "client_ref": client_ref(), "payment": { "method": "card" },
                       "lines": [{ "product_id": s.product, "quantity": "5", "unit_price": 2400 }] });
    for expected in [StatusCode::CREATED, StatusCode::OK] {
        let (status, res) = ws_call(
            &s.f,
            &s.shop_ws,
            Method::POST,
            "/till/sales",
            Some(body.clone()),
            &t,
        )
        .await;
        assert_eq!(status, expected, "{res}");
    }
    assert_eq!(balances(&s).await, TILL);
    assert_eq!(TILL.iter().sum::<i64>(), 3_000_000);

    let kinds: Vec<(String, String)> = sqlx::query_as(
        "SELECT e.kind, coalesce(a.kind, '-') FROM farm_passbook_entries e
         LEFT JOIN farm_sale_attributions a ON a.id = e.attribution_id
         WHERE e.person_id = $1::uuid ORDER BY e.created_at, e.id",
    )
    .bind(&s.persons[0])
    .fetch_all(&s.f.pool)
    .await
    .unwrap();
    assert_eq!(
        kinds,
        [
            ("sale_credit".to_owned(), "sale".to_owned()),
            ("sale_reversal".to_owned(), "return".to_owned()),
            ("sale_reversal".to_owned(), "return".to_owned()),
            ("sale_credit".to_owned(), "sale".to_owned()),
        ],
        "web sale, two returns, till sale"
    );
}

#[sqlx::test]
async fn done_when_the_shop_is_another_workspace(pool: PgPool) {
    done_when(pool, true).await;
}

#[sqlx::test]
async fn done_when_farm_and_shop_are_one_workspace(pool: PgPool) {
    done_when(pool, false).await;
}

#[sqlx::test]
async fn a_credited_farmer_is_paid_and_the_day_reconciles(pool: PgPool) {
    let s = sold(pool, true).await;
    let f = &s.f;
    let ilu = f.ilundo.clone();
    let juma = f
        .staff("juma@ruaha.test", "field_officer", Some(&ilu))
        .await;
    let juma_id = f.user_id("juma@ruaha.test").await;
    let float = json!({ "kind": "float_out", "staff_user_id": juma_id, "village_id": f.ilundo,
                        "currency_id": s.tzs, "amount": 2_000_000 });
    let (status, _) = f
        .call(Method::POST, "/cash/movements", Some(float), &f.ops)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let pay = json!({ "client_ref": client_ref(), "person_id": s.persons[0], "amount": SALE[0],
                      "currency_id": s.tzs, "id_type": "nida", "name_confirmed": true });
    // Salima registered Neema's household: four eyes send her away.
    let (status, _) = f
        .call(Method::POST, "/payouts", Some(pay.clone()), &f.officer_ilu)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, res) = f
        .call(Method::POST, "/payouts", Some(pay.clone()), &juma)
        .await;
    // TZS 11,235.96 is under the default officer limit (TZS 100,000.00).
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(balances(&s).await[0], 0);

    let eat = FixedOffset::east_opt(3 * 3600).unwrap();
    let day = Utc::now().with_timezone(&eat).date_naive();
    let (_, rec) = f
        .call(
            Method::GET,
            &format!("/cash/reconciliation?day={day}"),
            None,
            &f.ops,
        )
        .await;
    let row = &rec["data"][0];
    assert_eq!(row["paid"]["minor"], SALE[0]);
    assert_eq!(row["expected"]["minor"], 2_000_000 - SALE[0]);
}

#[sqlx::test]
async fn the_shop_never_sees_farmers_and_the_event_goes_to_the_farm_only(pool: PgPool) {
    let s = sold(pool, true).await;
    // The shop's members reach none of the farm's records.
    for tail in [
        format!("/persons/{}/passbook", s.persons[0]),
        "/supply-batches".to_owned(),
    ] {
        let (status, _) = s.f.call(Method::GET, &tail, None, &s.shop).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{tail}");
    }
    let events: Vec<(String, Value)> = sqlx::query_as(
        "SELECT workspace_id::text, payload FROM webhook_events WHERE type = 'sale.attributed'",
    )
    .fetch_all(&s.f.pool)
    .await
    .unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].0, s.f.ws, "emitted in the farm workspace only");
    assert_eq!(events[0].1["kind"], "sale");
    assert_eq!(events[0].1["farmers"], 3);
    assert_eq!(events[0].1["jars"], "3.00");
    assert!(
        !events[0].1.to_string().contains(&s.persons[0]),
        "no person ids in the event"
    );
    assert_eq!(events[0].1["total"], 1_800_000);
}

#[sqlx::test]
async fn attributions_are_append_only_and_a_sale_is_credited_once(pool: PgPool) {
    let s = sold(pool, false).await;
    for sql in [
        "UPDATE farm_sale_attributions SET total = 1",
        "DELETE FROM farm_sale_attributions",
        "TRUNCATE farm_sale_attributions CASCADE",
    ] {
        assert!(sqlx::query(sql).execute(&s.f.pool).await.is_err(), "{sql}");
    }
    let (status, _) = ws_call(
        &s.f,
        &s.shop_ws,
        Method::POST,
        &format!("/orders/{}/complete", s.order),
        None,
        &s.shop,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "completed once");
    assert_eq!(balances(&s).await, SALE, "credited once");
}

#[sqlx::test]
async fn returned_jars_go_back_into_their_lot_and_sell_with_credit_again(pool: PgPool) {
    let s = sold(pool, false).await;
    give_back(&s, "3").await;
    assert_eq!(balances(&s).await, [0, 0, 0]);
    let level: String = sqlx::query_scalar(
        "SELECT coalesce(sum(s.stock_lvl), 0)::numeric(19,2)::text FROM stocks s
         JOIN lots l ON l.id = s.lot_id WHERE l.code = 'ILU-J-1' AND s.deleted_at IS NULL",
    )
    .fetch_one(&s.f.pool)
    .await
    .unwrap();
    assert_eq!(level, "10.00", "the 3 returned jars are back in their lot");

    let again = created(
        &s.f,
        &s.shop_ws,
        "/orders",
        json!({ "lines": [{ "product_id": s.product, "quantity": "3" }] }),
        &s.shop,
        "order_id",
    )
    .await;
    act(&s.f, &s.shop_ws, &s.shop, &again, "confirm", None).await;
    let lines = order_lines(&s.f, &s.shop_ws, &s.shop, &again).await;
    act(
        &s.f,
        &s.shop_ws,
        &s.shop,
        &again,
        "pick",
        Some(json!({ "lines": [{ "line_id": lines[0], "quantity": "3" }] })),
    )
    .await;
    act(&s.f, &s.shop_ws, &s.shop, &again, "complete", None).await;

    assert_eq!(balances(&s).await, SALE, "the same credit as one sale of 3");
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM farm_sale_attributions ORDER BY created_at, id")
            .fetch_all(&s.f.pool)
            .await
            .unwrap();
    assert_eq!(kinds, ["sale", "return", "sale"]);
}
