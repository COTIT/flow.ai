//! Till login: registers with a device key, till users with a PIN, PIN login with lockout,
//! the till session token and what kills it.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send};
use serde_json::{Value, json};
use sqlx::PgPool;

const PIN: &str = "4821";

/// Ada's `acme` with a store warehouse.
async fn world(pool: PgPool) -> Api {
    let api = Api::new(pool, "/till").await;
    store(&api).await;
    api
}

async fn store(api: &Api) -> String {
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO warehouses (workspace_id, name, slug, is_store)
         VALUES ($1::uuid, 'Shop', 'shop', true) RETURNING id",
    )
    .bind(&api.ws)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    id.to_string()
}

async fn store_id(api: &Api) -> String {
    let id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM warehouses WHERE workspace_id = $1::uuid AND is_store")
            .bind(&api.ws)
            .fetch_one(&api.pool)
            .await
            .unwrap();
    id.to_string()
}

/// New register; `(id, device key)`.
async fn register(api: &Api, name: &str) -> (String, String) {
    let body = json!({ "name": name, "warehouse_id": store_id(api).await });
    let (status, res) = api
        .call(Method::POST, "/registers", Some(body), &api.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    (
        res["data"]["id"].as_str().unwrap().to_owned(),
        res["data"]["key"].as_str().unwrap().to_owned(),
    )
}

async fn till_user(api: &Api, name: &str, pin: &str) -> String {
    let (status, res) = api
        .call(
            Method::POST,
            "/users",
            Some(json!({ "name": name, "pin": pin })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["id"].as_str().unwrap().to_owned()
}

async fn login(api: &Api, key: &str, user: &str, pin: &str) -> (StatusCode, Value) {
    api.call(
        Method::POST,
        "/login",
        Some(json!({ "till_user_id": user, "pin": pin })),
        key,
    )
    .await
}

async fn token(api: &Api, key: &str, user: &str) -> String {
    let (status, res) = login(api, key, user, PIN).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"]["token"].as_str().unwrap().to_owned()
}

async fn me(api: &Api, token: &str) -> StatusCode {
    api.call(Method::GET, "/me", None, token).await.0
}

#[sqlx::test]
async fn login_with_device_key_and_pin_opens_a_session(pool: PgPool) {
    let api = world(pool).await;
    let (reg, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;

    let (status, res) = api.call(Method::GET, "/login/users", None, &key).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"], json!([{ "id": mary, "name": "Mary" }]));

    let (status, res) = login(&api, &key, &mary, PIN).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let token = res["data"]["token"].as_str().unwrap();
    assert_eq!(token.len(), 64);
    assert_eq!(res["data"]["register"]["id"], reg.as_str());
    assert_eq!(res["data"]["user"]["name"], "Mary");

    let (status, res) = api.call(Method::GET, "/me", None, token).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["register"]["name"], "Till 1");
    assert_eq!(res["data"]["user"]["id"], mary.as_str());

    let (status, _) = api.call(Method::POST, "/logout", None, token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(me(&api, token).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn pin_and_token_are_stored_hashed_only(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let token = token(&api, &key, &mary).await;
    let (pin_hash, token_hash): (String, Vec<u8>) = sqlx::query_as(
        "SELECT u.pin_hash, s.token_hash FROM till_users u JOIN till_sessions s
           ON s.till_user_id = u.id",
    )
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert!(pin_hash.starts_with("$argon2id$"), "{pin_hash}");
    assert!(!pin_hash.contains(PIN));
    assert_ne!(token_hash, token.as_bytes());
    assert_eq!(token_hash.len(), 32);
}

#[sqlx::test]
async fn wrong_pin_unknown_and_inactive_user_answer_alike(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let gone = till_user(&api, "Gone", PIN).await;
    let (status, _) = api
        .call(
            Method::PATCH,
            &format!("/users/{gone}"),
            Some(json!({ "active": false })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (wrong, wrong_body) = login(&api, &key, &mary, "0000").await;
    let (unknown, unknown_body) =
        login(&api, &key, "0190a000-0000-7000-8000-000000000000", PIN).await;
    let (inactive, inactive_body) = login(&api, &key, &gone, PIN).await;
    assert_eq!(wrong, StatusCode::UNAUTHORIZED);
    assert_eq!((unknown, inactive), (wrong, wrong));
    assert_eq!(unknown_body, wrong_body);
    assert_eq!(inactive_body, wrong_body);

    let (status, res) = api.call(Method::GET, "/login/users", None, &key).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        res["data"].as_array().unwrap().len(),
        1,
        "inactive not listed"
    );
}

#[sqlx::test]
async fn five_wrong_pins_lock_the_user_not_the_others(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let john = till_user(&api, "John", "7777").await;
    for _ in 0..5 {
        assert_eq!(
            login(&api, &key, &mary, "0000").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        login(&api, &key, &mary, PIN).await.0,
        StatusCode::TOO_MANY_REQUESTS,
        "locked even with the right PIN"
    );
    assert_eq!(
        login(&api, &key, &john, "7777").await.0,
        StatusCode::OK,
        "another user still logs in"
    );

    sqlx::query("UPDATE till_users SET locked_until = now() - interval '1 second'")
        .execute(&api.pool)
        .await
        .unwrap();
    assert_eq!(login(&api, &key, &mary, PIN).await.0, StatusCode::OK);
}

#[sqlx::test]
async fn twenty_wrong_pins_lock_that_register_only(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let (_, other_key) = register(&api, "Till 2").await;
    let users: Vec<String> = {
        let mut v = Vec::new();
        for n in 0..5 {
            v.push(till_user(&api, &format!("U{n}"), PIN).await);
        }
        v
    };
    for n in 0..20 {
        let (status, _) = login(&api, &key, &users[n % 5], "0000").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "try {n}");
        // Keep the users themselves under their own limit.
        sqlx::query("UPDATE till_users SET failed_pins = 0, locked_until = NULL")
            .execute(&api.pool)
            .await
            .unwrap();
    }
    assert_eq!(
        login(&api, &key, &users[0], PIN).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        login(&api, &other_key, &users[0], PIN).await.0,
        StatusCode::OK,
        "another register is not locked"
    );
}

#[sqlx::test]
async fn revoked_or_rotated_key_and_deactivated_user_kill_live_sessions(pool: PgPool) {
    let api = world(pool).await;
    let (reg, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let john = till_user(&api, "John", PIN).await;

    let t1 = token(&api, &key, &mary).await;
    let t2 = token(&api, &key, &john).await;
    let (status, res) = api
        .call(
            Method::PATCH,
            &format!("/users/{john}"),
            Some(json!({ "active": false })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(me(&api, &t2).await, StatusCode::UNAUTHORIZED, "user off");
    assert_eq!(me(&api, &t1).await, StatusCode::OK);

    let (status, res) = api
        .call(
            Method::POST,
            &format!("/registers/{reg}/key"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let new_key = res["data"]["key"].as_str().unwrap().to_owned();
    assert_eq!(me(&api, &t1).await, StatusCode::UNAUTHORIZED, "key rotated");
    assert_eq!(
        login(&api, &key, &mary, PIN).await.0,
        StatusCode::UNAUTHORIZED
    );

    let t3 = token(&api, &new_key, &mary).await;
    let key_id: uuid::Uuid =
        sqlx::query_scalar("SELECT app_key_id FROM till_registers WHERE id = $1::uuid")
            .bind(&reg)
            .fetch_one(&api.pool)
            .await
            .unwrap();
    let (status, _) = api
        .with_base("/app-keys")
        .call(Method::DELETE, &format!("/{key_id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(me(&api, &t3).await, StatusCode::UNAUTHORIZED, "key revoked");
}

#[sqlx::test]
async fn expired_session_is_refused(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let t = token(&api, &key, &mary).await;
    let ttl: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM expires_at - created_at)::float8 FROM till_sessions",
    )
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(ttl, 8.0 * 3600.0);
    sqlx::query("UPDATE till_sessions SET expires_at = now() - interval '1 second'")
        .execute(&api.pool)
        .await
        .unwrap();
    assert_eq!(me(&api, &t).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn tokens_and_keys_stay_in_their_lane(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let t = token(&api, &key, &mary).await;

    // Till token on an office route, member token and device key on a till route.
    let (status, _) = api.call(Method::GET, "/users", None, &t).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&api, &api.owner).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&api, &key).await, StatusCode::UNAUTHORIZED);
    // Member token can't log in a till.
    assert_eq!(
        login(&api, &api.owner, &mary, PIN).await.0,
        StatusCode::UNAUTHORIZED
    );
    // A shop key (no `till:login`) can't either.
    let (status, res) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "Shop", "kind": "shop", "scopes": ["catalog:read"] })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let shop = res["data"]["key"].as_str().unwrap();
    assert_eq!(login(&api, shop, &mary, PIN).await.0, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn another_workspace_is_refused_everywhere(pool: PgPool) {
    let api = world(pool).await;
    let (reg, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let t = token(&api, &key, &mary).await;

    let eve = api.as_new_owner("eve@example.com", "evil").await;
    store(&eve).await;
    let (_, eve_key) = register(&eve, "Evil till").await;
    let eve_user = till_user(&eve, "Eve", PIN).await;

    // Device key or session of acme under evil's path: 404.
    assert_eq!(
        eve.call(Method::GET, "/login/users", None, &key).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(login(&eve, &key, &mary, PIN).await.0, StatusCode::NOT_FOUND);
    assert_eq!(me(&eve, &t).await, StatusCode::NOT_FOUND);
    // Evil's key can't log in acme's user.
    assert_eq!(
        login(&eve, &eve_key, &mary, PIN).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&api, &key, &eve_user, PIN).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Office routes: evil's owner sees none of acme's rows.
    let uri = format!("/v1/workspace/{}/till/registers", api.ws);
    let (status, _) = send(&api.app, Method::GET, &uri, None, Some(&eve.owner)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for path in [format!("/registers/{reg}/key")] {
        let (status, _) = eve.call(Method::POST, &path, None, &eve.owner).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    let (status, _) = eve
        .call(
            Method::PATCH,
            &format!("/users/{mary}"),
            Some(json!({ "active": false })),
            &eve.owner,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, listed) = eve.call(Method::GET, "/users", None, &eve.owner).await;
    assert_eq!(listed["data"].as_array().unwrap().len(), 1);
    // A register can't point at another workspace's warehouse.
    let body = json!({ "name": "x", "warehouse_id": store_id(&eve).await });
    let (status, res) = api
        .call(Method::POST, "/registers", Some(body), &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn office_routes_need_cashregister_rights_and_valid_input(pool: PgPool) {
    let api = world(pool).await;
    let bob = member_with(&api.app, &api.pool, &api.ws, "bob@example.com", &[]).await;
    for (method, path) in [
        (Method::GET, "/registers"),
        (Method::POST, "/registers"),
        (Method::GET, "/users"),
        (Method::POST, "/users"),
    ] {
        let (status, _) = api.call(method.clone(), path, Some(json!({})), &bob).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }
    let carl = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "carl@example.com",
        &["cashregister.create", "cashregister.list"],
    )
    .await;
    let (status, res) = api
        .call(
            Method::POST,
            "/users",
            Some(json!({ "name": "Mary", "pin": PIN })),
            &carl,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");

    for pin in ["123", "123456789", "12a4", " 1234"] {
        let (status, res) = api
            .call(
                Method::POST,
                "/users",
                Some(json!({ "name": "X", "pin": pin })),
                &api.owner,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{pin}");
        assert!(res["errors"]["pin"].is_array(), "{res}");
    }
    // Not a store warehouse.
    let plain: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO warehouses (workspace_id, name, slug, is_store)
         VALUES ($1::uuid, 'Back', 'back', false) RETURNING id",
    )
    .bind(&api.ws)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    let (status, res) = api
        .call(
            Method::POST,
            "/registers",
            Some(json!({ "name": "x", "warehouse_id": plain })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(res["errors"]["warehouse_id"].is_array(), "{res}");

    let (_, key) = register(&api, "Till 1").await;
    let (status, res) = api.call(Method::GET, "/registers", None, &api.owner).await;
    assert_eq!(status, StatusCode::OK);
    let row = &res["data"][0];
    assert_eq!(row["name"], "Till 1");
    assert_eq!(row["key_prefix"], &key[..8]);
    assert!(row.get("key").is_none(), "secret only on create");
}

#[sqlx::test]
async fn till_key_reads_its_own_catalog_only(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let products = api.with_base("/products");
    products
        .create(json!({ "name": "Honey 500 g", "price": 10 }), "product_id")
        .await;
    let eve = api.as_new_owner("eve@example.com", "evil").await;
    eve.with_base("/products")
        .create(json!({ "name": "Evil honey", "price": 10 }), "product_id")
        .await;

    let (status, res) = send(
        &api.app,
        Method::GET,
        "/v1/storefront/products",
        None,
        Some(&key),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let names: Vec<&str> = res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Honey 500 g"]);
}

#[sqlx::test]
async fn a_right_pin_does_not_reset_the_register_count(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let users: Vec<String> = {
        let mut v = Vec::new();
        for n in 0..4 {
            v.push(till_user(&api, &format!("U{n}"), PIN).await);
        }
        v
    };
    for n in 0..19 {
        assert_eq!(
            login(&api, &key, &users[n % 4], "0000").await.0,
            StatusCode::UNAUTHORIZED
        );
        sqlx::query("UPDATE till_users SET failed_pins = 0, locked_until = NULL")
            .execute(&api.pool)
            .await
            .unwrap();
    }
    assert_eq!(login(&api, &key, &mary, PIN).await.0, StatusCode::OK);
    assert_eq!(
        login(&api, &key, &users[0], "0000").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&api, &key, &mary, PIN).await.0,
        StatusCode::TOO_MANY_REQUESTS,
        "20 wrong in the hour, a right one between"
    );
    // Wrong PINs older than an hour start a new count.
    sqlx::query(
        "UPDATE till_registers SET locked_until = NULL, failed_pins = 19,
                                   failed_since = now() - interval '2 hours'",
    )
    .execute(&api.pool)
    .await
    .unwrap();
    login(&api, &key, &users[1], "0000").await;
    assert_eq!(login(&api, &key, &mary, PIN).await.0, StatusCode::OK);
}

#[sqlx::test]
async fn each_lock_lasts_twice_as_long_until_a_right_pin(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let lock = || async {
        let minutes: f64 = sqlx::query_scalar(
            "SELECT round(extract(epoch FROM locked_until - now()) / 60)::float8 FROM till_users",
        )
        .fetch_one(&api.pool)
        .await
        .unwrap();
        sqlx::query("UPDATE till_users SET locked_until = now() - interval '1 second'")
            .execute(&api.pool)
            .await
            .unwrap();
        minutes
    };
    let mut seen = Vec::new();
    for _ in 0..3 {
        for _ in 0..5 {
            login(&api, &key, &mary, "0000").await;
        }
        seen.push(lock().await);
    }
    assert_eq!(seen, [15.0, 30.0, 60.0]);
    assert_eq!(login(&api, &key, &mary, PIN).await.0, StatusCode::OK);
    for _ in 0..5 {
        login(&api, &key, &mary, "0000").await;
    }
    assert_eq!(lock().await, 15.0, "a right PIN starts over");
}

#[sqlx::test]
async fn new_pin_deactivation_and_new_login_end_old_sessions(pool: PgPool) {
    let api = world(pool).await;
    let (_, key) = register(&api, "Till 1").await;
    let mary = till_user(&api, "Mary", PIN).await;
    let patch = |body: Value| {
        let api = &api;
        let mary = mary.clone();
        async move {
            let (status, res) = api
                .call(
                    Method::PATCH,
                    &format!("/users/{mary}"),
                    Some(body),
                    &api.owner,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{res}");
        }
    };

    let t = token(&api, &key, &mary).await;
    patch(json!({ "active": false })).await;
    patch(json!({ "active": true })).await;
    assert_eq!(me(&api, &t).await, StatusCode::UNAUTHORIZED, "stays ended");

    let t = token(&api, &key, &mary).await;
    patch(json!({ "name": "Mary K" })).await;
    assert_eq!(me(&api, &t).await, StatusCode::OK, "a rename keeps it");
    patch(json!({ "pin": "9999" })).await;
    assert_eq!(me(&api, &t).await, StatusCode::UNAUTHORIZED, "new PIN");

    let (status, res) = login(&api, &key, &mary, "9999").await;
    assert_eq!(status, StatusCode::OK);
    let first = res["data"]["token"].as_str().unwrap().to_owned();
    let (_, res) = login(&api, &key, &mary, "9999").await;
    let second = res["data"]["token"].as_str().unwrap();
    assert_eq!(
        me(&api, &first).await,
        StatusCode::UNAUTHORIZED,
        "one per register"
    );
    assert_eq!(me(&api, second).await, StatusCode::OK);
}

#[sqlx::test]
async fn till_login_scope_comes_only_with_a_register(pool: PgPool) {
    let api = world(pool).await;
    let (status, res) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "x", "kind": "till", "scopes": ["till:login"] })),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["scopes"].is_array(), "{res}");
}
