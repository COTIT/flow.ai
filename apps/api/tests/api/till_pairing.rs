//! Till pairing: the office makes a one-time code for a register; the till swaps shop name +
//! code for the device key. Codes are hashed, single use, 10 minutes; failures lock per shop
//! name and address (10) and per shop name (50).

use crate::common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{Api, member_with, send, send_raw};
use genesy_360_api::{build_app, mail::Mailer};
use serde_json::{Value, json};
use sqlx::PgPool;

const PIN: &str = "4821";

/// Ada's `acme` with a store warehouse and register "Till 1"; `(api, register id, first key)`.
async fn world(pool: PgPool) -> (Api, String, String) {
    let api = Api::new(pool, "/till").await;
    let (reg, key) = register(&api, "Till 1").await;
    (api, reg, key)
}

async fn register(api: &Api, name: &str) -> (String, String) {
    let store: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO warehouses (workspace_id, name, slug, is_store)
         VALUES ($1::uuid, $2, $2, true)
         ON CONFLICT DO NOTHING RETURNING id",
    )
    .bind(&api.ws)
    .bind(format!("shop-{}", name.to_lowercase().replace(' ', "-")))
    .fetch_one(&api.pool)
    .await
    .unwrap();
    let body = json!({ "name": name, "warehouse_id": store });
    let (status, res) = api
        .call(Method::POST, "/registers", Some(body), &api.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    (
        res["data"]["id"].as_str().unwrap().to_owned(),
        res["data"]["key"].as_str().unwrap().to_owned(),
    )
}

/// A fresh pairing code for `reg`.
async fn code(api: &Api, reg: &str) -> String {
    let (status, res) = api
        .call(
            Method::POST,
            &format!("/registers/{reg}/pairing"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let shop: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1::uuid")
        .bind(&api.ws)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(res["data"]["workspace"], shop.as_str());
    let code = res["data"]["code"].as_str().unwrap().to_owned();
    assert!(
        code.len() == 9 && code.as_bytes()[4] == b'-',
        "grouped 4-4: {code}"
    );
    code
}

async fn pair(api: &Api, workspace: &str, code: &str) -> (StatusCode, Value) {
    send(
        &api.app,
        Method::POST,
        "/v1/till/pair",
        Some(json!({ "workspace": workspace, "code": code })),
        None,
    )
    .await
}

async fn login_users(api: &Api, key: &str) -> StatusCode {
    api.call(Method::GET, "/login/users", None, key).await.0
}

#[sqlx::test]
async fn shop_name_and_code_give_the_till_its_key_and_retire_the_old_one(pool: PgPool) {
    let (api, reg, old_key) = world(pool).await;
    // A till logged in on the old key.
    let (_, user) = api
        .call(
            Method::POST,
            "/users",
            Some(json!({ "name": "Mary", "pin": PIN })),
            &api.owner,
        )
        .await;
    let (_, session) = api
        .call(
            Method::POST,
            "/login",
            Some(json!({ "till_user_id": user["data"]["id"], "pin": PIN })),
            &old_key,
        )
        .await;
    let token = session["data"]["token"].as_str().unwrap().to_owned();

    let c = code(&api, &reg).await;
    assert_eq!(
        login_users(&api, &old_key).await,
        StatusCode::OK,
        "a code alone changes nothing"
    );

    let (status, res) = pair(&api, "acme", &c).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(d["workspace_id"], api.ws.as_str());
    assert_eq!(d["register"]["id"], reg.as_str());
    assert_eq!(d["register"]["name"], "Till 1");
    let new_key = d["device_key"].as_str().unwrap();
    assert!(new_key.starts_with("gsk_"));

    assert_eq!(login_users(&api, new_key).await, StatusCode::OK);
    assert_eq!(login_users(&api, &old_key).await, StatusCode::UNAUTHORIZED);
    let (status, _) = api.call(Method::GET, "/me", None, &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "old sessions end");

    // Audit trail and the office's "last paired".
    let audit: (bool, bool, bool, Option<String>) = sqlx::query_as(
        "SELECT created_by IS NOT NULL, redeemed_at IS NOT NULL, app_key_id IS NOT NULL,
                redeemed_ip
         FROM till_pairings",
    )
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(audit, (true, true, true, Some("127.0.0.1".into())));
    let (_, list) = api.call(Method::GET, "/registers", None, &api.owner).await;
    assert!(list["data"][0]["paired_at"].is_string(), "{list}");
}

#[sqlx::test]
async fn a_code_works_once_expires_and_is_voided_by_a_newer_one(pool: PgPool) {
    let (api, reg, _) = world(pool).await;
    let gone = json!("This code has expired or was already used. Ask the office for a new one.");

    let c = code(&api, &reg).await;
    assert_eq!(pair(&api, "acme", &c).await.0, StatusCode::OK);
    let (status, res) = pair(&api, "acme", &c).await;
    assert_eq!(status, StatusCode::GONE, "single use");
    assert_eq!(res["message"], gone);

    let old = code(&api, &reg).await;
    let new = code(&api, &reg).await;
    assert_eq!(pair(&api, "acme", &old).await.0, StatusCode::GONE, "voided");
    let ttl: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM max(expires_at - created_at))::float8 FROM till_pairings",
    )
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(ttl, 600.0, "10 minutes");
    sqlx::query("UPDATE till_pairings SET expires_at = now() - interval '1 second'")
        .execute(&api.pool)
        .await
        .unwrap();
    assert_eq!(
        pair(&api, "acme", &new).await.0,
        StatusCode::GONE,
        "expired"
    );
}

#[sqlx::test]
async fn unknown_shop_wrong_code_and_another_shop_answer_alike(pool: PgPool) {
    let (api, reg, _) = world(pool).await;
    let eve = api.as_new_owner("eve@example.com", "evil").await;
    let c = code(&api, &reg).await;

    let (s1, b1) = pair(&api, "acme", "ZZZZ-ZZZZ").await;
    let (s2, b2) = pair(&api, "nosuchshop", &c).await;
    let (s3, b3) = pair(&api, "evil", &c).await;
    assert_eq!(s1, StatusCode::UNAUTHORIZED);
    assert_eq!((s2, s3), (s1, s1));
    assert_eq!((b2, b3), (b1.clone(), b1));
    let _ = eve;
    assert_eq!(
        pair(&api, "acme", &c).await.0,
        StatusCode::OK,
        "still valid"
    );
}

#[sqlx::test]
async fn typing_is_forgiving(pool: PgPool) {
    let (api, reg, _) = world(pool).await;
    let c = code(&api, &reg).await;
    // Lower case, no dash, spaces, and the look-alikes O/0 and I/L/1 swapped back.
    let typed: String = c
        .replace('-', " ")
        .to_lowercase()
        .replace('0', "o")
        .replace('1', "l");
    let (status, res) = pair(&api, "  ACME ", &typed).await;
    assert_eq!(status, StatusCode::OK, "{typed}: {res}");
}

/// Same app and database, but trusting X-Forwarded-For, so a test can pick the client address.
fn behind_proxy(api: &Api) -> Router {
    let mut state = common::state(api.pool.clone(), Mailer::capture().0);
    state.rate_limit.trust_forwarded = true;
    build_app(state)
}

async fn pair_from(app: &Router, ip: &str, workspace: &str, code: &str) -> StatusCode {
    let body = json!({ "workspace": workspace, "code": code });
    let headers = [("x-forwarded-for", ip)];
    send_raw(
        app,
        Method::POST,
        "/v1/till/pair",
        Some(body),
        None,
        &headers,
    )
    .await
    .0
}

#[sqlx::test]
async fn ten_failures_from_one_address_lock_that_name_there_only(pool: PgPool) {
    let (api, reg, _) = world(pool).await;
    let proxy = behind_proxy(&api);
    let other = api.as_new_owner("bob@example.com", "beta").await;
    let (oreg, _) = register(&other, "Till B").await;
    let c = code(&api, &reg).await;
    for n in 0..10 {
        let status = pair_from(&proxy, "203.0.113.7", "acme", "AAAA-AAAA").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "try {n}");
    }
    assert_eq!(
        pair_from(&proxy, "203.0.113.7", "acme", &c).await,
        StatusCode::TOO_MANY_REQUESTS,
        "the right code too"
    );
    let oc = code(&other, &oreg).await;
    assert_eq!(
        pair_from(&proxy, "203.0.113.7", "beta", &oc).await,
        StatusCode::OK,
        "other shop, same address"
    );
    assert_eq!(
        pair_from(&proxy, "198.51.100.9", "acme", &c).await,
        StatusCode::OK,
        "same shop, other address"
    );
    // Unknown names lock the same way: no difference to probe.
    for _ in 0..10 {
        pair_from(&proxy, "203.0.113.7", "ghost", "AAAA-AAAA").await;
    }
    assert_eq!(
        pair_from(&proxy, "203.0.113.7", "ghost", "AAAA-AAAA").await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test]
async fn fifty_failures_across_addresses_lock_the_name(pool: PgPool) {
    let (api, reg, _) = world(pool).await;
    let proxy = behind_proxy(&api);
    let other = api.as_new_owner("bob@example.com", "beta").await;
    let (oreg, _) = register(&other, "Till B").await;
    let c = code(&api, &reg).await;
    for n in 0..50 {
        let status = pair_from(&proxy, &format!("10.0.{n}.1"), "acme", "AAAA-AAAA").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "address {n}");
    }
    assert_eq!(
        pair_from(&proxy, "198.51.100.9", "acme", &c).await,
        StatusCode::TOO_MANY_REQUESTS,
        "fresh address, right code"
    );
    let oc = code(&other, &oreg).await;
    assert_eq!(
        pair_from(&proxy, "198.51.100.9", "beta", &oc).await,
        StatusCode::OK,
        "other shop"
    );
}

#[sqlx::test]
async fn codes_are_stored_hashed_and_need_register_rights(pool: PgPool) {
    let (api, reg, _) = world(pool).await;
    let c = code(&api, &reg).await;
    let plain = c.replace('-', "");
    let leaked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM till_pairings WHERE encode(code_hash, 'escape') LIKE '%' || $1 || '%'
            OR octet_length(code_hash) <> 32",
    )
    .bind(&plain)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(leaked, 0);

    let bob = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "bob@example.com",
        &["cashregister.list"],
    )
    .await;
    let (status, _) = api
        .call(
            Method::POST,
            &format!("/registers/{reg}/pairing"),
            None,
            &bob,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let eve = api.as_new_owner("eve@example.com", "evil").await;
    let (status, _) = eve
        .call(
            Method::POST,
            &format!("/registers/{reg}/pairing"),
            None,
            &eve.owner,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another workspace's register"
    );
    let (status, res) = pair(&api, "acme", "").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}
