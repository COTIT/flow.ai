//! /v1/workspace/{ws}/currencies: CRUD, default switching, member reads (vimdesk-web
//! currency.service.js; every page load reads the list to format money).

mod common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{app_with_mail, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const NOT_FOUND: &str = "The requested resource could not be found.";
const UNAUTHORIZED: &str = "This action is unauthorized.";

struct World {
    app: Router,
    pool: PgPool,
    owner: String,
    ws: String,
}

/// Ada owns `acme`.
async fn world(pool: PgPool) -> World {
    let (app, _) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    World {
        app,
        pool,
        owner,
        ws,
    }
}

fn dollar() -> Value {
    json!({
        "name": "Dollar", "asset_type": "usd", "symbol_prefix": "$", "symbol_suffix": "USD",
        "decimals": 2, "default_currency": false, "active": true, "meta": { "k": "v" },
    })
}

impl World {
    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}/currencies{path}", self.ws);
        send(&self.app, method, &uri, body, Some(token)).await
    }

    async fn create(&self, body: Value) -> String {
        let (status, res) = self.call(Method::POST, "", Some(body), &self.owner).await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["currency_id"].as_str().unwrap().to_owned()
    }

    async fn get(&self, id: &str) -> Value {
        let (status, res) = self
            .call(Method::GET, &format!("/{id}"), None, &self.owner)
            .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        res["data"].clone()
    }

    async fn list(&self, query: &str) -> Value {
        let (status, res) = self.call(Method::GET, query, None, &self.owner).await;
        assert_eq!(status, StatusCode::OK, "{res}");
        res
    }

    async fn defaults(&self) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT id::text FROM currencies
             WHERE workspace_id = $1::uuid AND default_currency AND deleted_at IS NULL",
        )
        .bind(&self.ws)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }
}

// ================= happy path

#[sqlx::test]
async fn create_then_get_in_currency_resource_shape(pool: PgPool) {
    let w = world(pool).await;

    let id = w.create(dollar()).await;

    let c = w.get(&id).await;
    let keys: Vec<&str> = c.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "active",
            "asset_type",
            "created_at",
            "decimals",
            "default_currency",
            "id",
            "last_updated_at",
            "meta",
            "name",
            "symbol_prefix",
            "symbol_suffix"
        ]
    );
    assert_eq!(c["id"], id.as_str());
    assert_eq!(c["name"], "Dollar");
    assert_eq!(c["asset_type"], "usd");
    assert_eq!(c["symbol_prefix"], "$");
    assert_eq!(c["symbol_suffix"], "USD");
    assert_eq!(c["decimals"], 2);
    // vimdesk-web matches the default with `=== true`.
    assert_eq!(c["default_currency"], false);
    assert_eq!(c["active"], true);
    assert_eq!(c["meta"], json!({ "k": "v" }));
    assert!(c["created_at"].is_string() && c["last_updated_at"].is_string());
}

#[sqlx::test]
async fn list_pages_with_laravel_meta(pool: PgPool) {
    let w = world(pool).await;
    let before = w.list("").await["meta"]["total"].as_i64().unwrap();
    for i in 0..3 {
        w.create(json!({
            "name": format!("C{i}"), "asset_type": "eur", "decimals": 2,
            "default_currency": false, "active": true,
        }))
        .await;
    }

    let res = w.list(&format!("?page=2&page_size={}", before + 2)).await;

    assert_eq!(res["meta"]["total"], before + 3);
    assert_eq!(res["meta"]["current_page"], 2);
    assert_eq!(res["meta"]["last_page"], 2);
    assert_eq!(res["data"].as_array().unwrap().len(), 1);
    assert_eq!(res["data"][0]["name"], "C2", "oldest first");
}

#[sqlx::test]
async fn update_is_a_patch_and_returns_message_ok(pool: PgPool) {
    let w = world(pool).await;
    let id = w.create(dollar()).await;

    // The settings form echoes the whole record back, read-only keys included.
    let (status, res) = w
        .call(
            Method::POST,
            &format!("/{id}"),
            Some(json!({
                "id": "ignored", "created_at": "ignored", "name": "Euro",
                "asset_type": "EUR", "symbol_prefix": null, "symbol_suffix": "  ",
                "decimals": "3", "active": 0,
            })),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res, json!({ "message": "ok" }));
    let c = w.get(&id).await;
    assert_eq!(c["id"], id.as_str());
    assert_eq!(c["name"], "Euro");
    assert_eq!(c["asset_type"], "EUR");
    assert_eq!(c["symbol_prefix"], Value::Null, "null clears");
    assert_eq!(c["symbol_suffix"], Value::Null, "blank clears");
    assert_eq!(c["decimals"], 3);
    assert_eq!(c["active"], false);
    assert_eq!(c["meta"], json!({ "k": "v" }), "absent keeps");
    assert_eq!(c["default_currency"], false);
}

#[sqlx::test]
async fn delete_is_soft_and_hides_the_row(pool: PgPool) {
    let w = world(pool).await;
    let id = w.create(dollar()).await;

    let (status, res) = w
        .call(Method::DELETE, &format!("/{id}"), None, &w.owner)
        .await;

    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let (status, _) = w.call(Method::GET, &format!("/{id}"), None, &w.owner).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let listed = w.list("?page_size=100").await;
    assert!(
        listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != id.as_str())
    );
    let deleted: bool =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM currencies WHERE id = $1::uuid")
            .bind(&id)
            .fetch_one(&w.pool)
            .await
            .unwrap();
    assert!(deleted, "row kept for referencing documents");
    let (status, _) = w
        .call(Method::DELETE, &format!("/{id}"), None, &w.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "second delete");
}

// ================= default currency

#[sqlx::test]
async fn new_default_replaces_old_one(pool: PgPool) {
    let w = world(pool).await;
    let first = w
        .create(json!({ "name": "A", "asset_type": "eur", "decimals": 2, "default_currency": true, "active": true }))
        .await;
    assert_eq!(w.defaults().await, std::slice::from_ref(&first));

    let second = w
        .create(json!({ "name": "B", "asset_type": "usd", "decimals": 2, "default_currency": "1", "active": true }))
        .await;
    assert_eq!(w.defaults().await, std::slice::from_ref(&second));
    assert_eq!(w.get(&first).await["default_currency"], false);

    // Switching back through update.
    let (status, _) = w
        .call(
            Method::POST,
            &format!("/{first}"),
            Some(json!({ "default_currency": true })),
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(w.defaults().await, std::slice::from_ref(&first));

    // Re-saving the current default keeps it.
    let (status, _) = w
        .call(
            Method::POST,
            &format!("/{first}"),
            Some(json!({ "default_currency": true, "name": "A2" })),
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(w.defaults().await, std::slice::from_ref(&first));

    // null = false, as Laravel's FilterValidateBoolean cast did.
    let (status, _) = w
        .call(
            Method::POST,
            &format!("/{first}"),
            Some(json!({ "default_currency": null })),
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(w.defaults().await.is_empty());
}

#[sqlx::test]
async fn failed_default_switch_rolls_back(pool: PgPool) {
    let w = world(pool).await;
    let first = w
        .create(json!({ "name": "A", "asset_type": "eur", "decimals": 2, "default_currency": true, "active": true }))
        .await;
    let other = uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0001);

    let (status, _) = w
        .call(
            Method::POST,
            &format!("/{other}"),
            Some(json!({ "default_currency": true })),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(w.defaults().await, [first], "old default kept");
}

#[sqlx::test]
async fn database_allows_one_live_default(pool: PgPool) {
    let w = world(pool).await;
    let insert =
        "INSERT INTO currencies (workspace_id, name, asset_type, default_currency, deleted_at)
                  VALUES ($1::uuid, 'X', 'eur', true, $2)";
    sqlx::query("UPDATE currencies SET default_currency = false WHERE workspace_id = $1::uuid")
        .bind(&w.ws)
        .execute(&w.pool)
        .await
        .unwrap();
    sqlx::query(insert)
        .bind(&w.ws)
        .bind(None::<chrono::DateTime<chrono::Utc>>)
        .execute(&w.pool)
        .await
        .unwrap();
    // A soft-deleted default doesn't count.
    sqlx::query(insert)
        .bind(&w.ws)
        .bind(Some(chrono::Utc::now()))
        .execute(&w.pool)
        .await
        .unwrap();

    let err = sqlx::query(insert)
        .bind(&w.ws)
        .bind(None::<chrono::DateTime<chrono::Utc>>)
        .execute(&w.pool)
        .await
        .unwrap_err();

    assert_eq!(
        err.as_database_error().and_then(|e| e.constraint()),
        Some("currencies_one_default")
    );
}

// ================= validation

#[sqlx::test]
async fn empty_body_lists_every_required_field(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w.call(Method::POST, "", Some(json!({})), &w.owner).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "active": ["The active field is required."],
            "asset_type": ["The asset type field is required."],
            "decimals": ["The decimals field is required."],
            "default_currency": ["The default currency field is required."],
            "name": ["The name field is required."],
        })
    );
}

#[sqlx::test]
async fn field_rules_use_laravel_wording(pool: PgPool) {
    let w = world(pool).await;
    let mut body = dollar();
    body["asset_type"] = json!("xyz");
    body["symbol_prefix"] = json!("12345678901");
    body["decimals"] = json!(9);
    body["default_currency"] = json!("yes");
    body["meta"] = json!([1]);
    body["name"] = json!("é".repeat(256));

    let (status, res) = w.call(Method::POST, "", Some(body), &w.owner).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "asset_type": ["Invalid asset type."],
            "decimals": ["The decimals must not be greater than 8."],
            "default_currency": ["The default currency field must be true or false."],
            "meta": ["The meta must be an array."],
            "name": ["The name must not be greater than 255 characters."],
            "symbol_prefix": ["The symbol prefix must not be greater than 10 characters."],
        })
    );
}

#[sqlx::test]
async fn boundaries_pass(pool: PgPool) {
    let w = world(pool).await;
    let mut body = dollar();
    body["name"] = json!("é".repeat(255));
    body["symbol_prefix"] = json!("1234567890");
    body["decimals"] = json!(0);
    body["asset_type"] = json!("JPY");

    let id = w.create(body).await;

    let c = w.get(&id).await;
    assert_eq!(c["decimals"], 0);
    assert_eq!(c["name"].as_str().unwrap().chars().count(), 255);
}

#[sqlx::test]
async fn update_null_on_required_field_is_422(pool: PgPool) {
    let w = world(pool).await;
    let id = w.create(dollar()).await;

    let (status, res) = w
        .call(
            Method::POST,
            &format!("/{id}"),
            Some(json!({ "name": null, "asset_type": "", "decimals": null, "active": null })),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"],
        json!({
            "active": ["The active field is required."],
            "asset_type": ["The asset type field is required."],
            "decimals": ["The decimals field is required."],
            "name": ["The name field is required."],
        })
    );
    assert_eq!(w.get(&id).await["name"], "Dollar");
}

#[sqlx::test]
async fn search_query_is_capped_and_literal(pool: PgPool) {
    let w = world(pool).await;
    w.create(json!({ "name": "100% Coin", "asset_type": "eur", "decimals": 2, "default_currency": false, "active": true }))
        .await;
    w.create(json!({ "name": "1000 Coin", "asset_type": "eur", "decimals": 2, "default_currency": false, "active": true }))
        .await;

    let res = w.list("?q=100%25").await;
    let names: Vec<&str> = res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["100% Coin"]);

    let (status, res) = w
        .call(
            Method::GET,
            &format!("?q={}", "a".repeat(101)),
            None,
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["q"],
        json!(["The q must not be greater than 100 characters."])
    );
}

#[sqlx::test]
async fn page_past_the_end_still_reports_total(pool: PgPool) {
    let w = world(pool).await;
    w.create(dollar()).await;
    let total = w.list("").await["meta"]["total"].clone();

    let res = w.list("?page=99&tab=x").await;

    assert_eq!(res["data"], json!([]));
    assert_eq!(res["meta"]["total"], total);
    assert_eq!(res["meta"]["from"], Value::Null);
}

// ================= access

#[sqlx::test]
async fn member_reads_without_any_permission(pool: PgPool) {
    let w = world(pool).await;
    let id = w.create(dollar()).await;
    let bob = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;

    let (status, res) = w.call(Method::GET, "", None, &bob).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = w.call(Method::GET, &format!("/{id}"), None, &bob).await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn writes_need_permission_and_403_comes_before_422(pool: PgPool) {
    let w = world(pool).await;
    let id = w.create(dollar()).await;
    let bob = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;

    for (method, path) in [
        (Method::POST, String::new()),
        (Method::POST, format!("/{id}")),
        (Method::DELETE, format!("/{id}")),
    ] {
        let (status, res) = w
            .call(method.clone(), &path, Some(json!({ "name": null })), &bob)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {res}");
        assert_eq!(res["message"], UNAUTHORIZED);
    }
}

#[sqlx::test]
async fn granted_member_can_write(pool: PgPool) {
    let w = world(pool).await;
    let bob = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "bob@example.com",
        &["currency.create", "currency.update", "currency.delete"],
    )
    .await;

    let (status, res) = w.call(Method::POST, "", Some(dollar()), &bob).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let id = res["data"]["currency_id"].as_str().unwrap();
    let (status, _) = w
        .call(
            Method::POST,
            &format!("/{id}"),
            Some(json!({ "name": "D" })),
            &bob,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = w.call(Method::DELETE, &format!("/{id}"), None, &bob).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn other_workspace_rows_are_404(pool: PgPool) {
    let w = world(pool).await;
    let id = w.create(dollar()).await;
    let (eve, eve_ws) = signup(&w.app, "eve@example.com", "evil").await;

    // Eve can't reach acme at all.
    let (status, res) = w.call(Method::GET, "", None, &eve).await;
    assert_eq!(
        (status, res["message"].clone()),
        (StatusCode::NOT_FOUND, json!(NOT_FOUND))
    );

    // Acme's currency id through Eve's own workspace.
    for method in [Method::GET, Method::POST, Method::DELETE] {
        let uri = format!("/v1/workspace/{eve_ws}/currencies/{id}");
        let body = (method == Method::POST).then(|| json!({ "name": "pwned" }));
        let (status, _) = send(&w.app, method.clone(), &uri, body, Some(&eve)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}");
    }
    assert_eq!(w.get(&id).await["name"], "Dollar");
    let listed = send(
        &w.app,
        Method::GET,
        &format!("/v1/workspace/{eve_ws}/currencies?page_size=100"),
        None,
        Some(&eve),
    )
    .await
    .1;
    assert!(
        listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != id.as_str())
    );
}

#[sqlx::test]
async fn malformed_and_unknown_ids_are_404(pool: PgPool) {
    let w = world(pool).await;

    for id in [
        "nope",
        "123",
        &uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0001).to_string(),
    ] {
        for method in [Method::GET, Method::POST, Method::DELETE] {
            let body = (method == Method::POST).then(|| json!({ "name": "x" }));
            let (status, _) = w
                .call(method.clone(), &format!("/{id}"), body, &w.owner)
                .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {id}");
        }
    }
}

#[sqlx::test]
async fn no_token_is_401(pool: PgPool) {
    let w = world(pool).await;

    let (status, _) = send(
        &w.app,
        Method::GET,
        &format!("/v1/workspace/{}/currencies", w.ws),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
