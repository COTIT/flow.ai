//! Expiring preview links: issue, list, revoke, and the app-key resolve that ends with its draft.

use crate::common::{self, Api};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;

const RESOLVE: &str = "/v1/storefront/preview";

fn document() -> Value {
    serde_json::from_str(include_str!("../fixtures/storefront-design-v1.json")).unwrap()
}

fn links_uri(api: &Api) -> String {
    format!(
        "/v1/workspace/{}/storefront/pages/home/preview-links",
        api.ws
    )
}

async fn keyed(api: &Api, uri: &str, body: Value, key: &str, token: &str) -> (StatusCode, Value) {
    let (status, bytes) = common::send_raw(
        &api.app,
        Method::POST,
        uri,
        Some(body),
        Some(token),
        &[("idempotency-key", key)],
    )
    .await;
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    common::contract::check(&Method::POST, uri, status, &body);
    (status, body)
}

async fn design(api: &Api, path: &str, body: Value, key: &str) -> Value {
    let uri = format!("/v1/workspace/{}/storefront/pages/home{path}", api.ws);
    let (status, body) = keyed(api, &uri, body, key, &api.owner).await;
    assert!(status.is_success(), "{status} {body}");
    body["data"].clone()
}

/// Saves the fixture as the first draft; returns its revision id.
async fn draft(api: &Api) -> String {
    let state = design(
        api,
        "/revisions",
        json!({"expected_version":0,"document":document()}),
        "draft",
    )
    .await;
    state["draft_revision_id"].as_str().unwrap().to_owned()
}

async fn issue_as(api: &Api, body: Value, key: &str, token: &str) -> (StatusCode, Value) {
    keyed(api, &links_uri(api), body, key, token).await
}

async fn issue(api: &Api, revision: &str, key: &str) -> Value {
    let (status, body) = issue_as(api, json!({"revision_id":revision}), key, &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"].clone()
}

async fn list(api: &Api, revision: &str, token: &str) -> (StatusCode, Value) {
    common::send(
        &api.app,
        Method::GET,
        &format!("{}?revision_id={revision}", links_uri(api)),
        None,
        Some(token),
    )
    .await
}

async fn revoke(api: &Api, link: &str, token: &str) -> (StatusCode, Value) {
    common::send(
        &api.app,
        Method::DELETE,
        &format!("{}/{link}", links_uri(api)),
        None,
        Some(token),
    )
    .await
}

async fn app_key(api: &Api, scopes: Value) -> String {
    let (status, body) = api
        .with_base("/app-keys")
        .call(
            Method::POST,
            "",
            Some(json!({"name":"Shop","kind":"shop","scopes":scopes})),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"]["key"].as_str().unwrap().to_owned()
}

async fn resolve_on(app: &Router, key: &str, token: &str) -> (StatusCode, Value) {
    common::send(
        app,
        Method::POST,
        RESOLVE,
        Some(json!({"token":token})),
        Some(key),
    )
    .await
}

async fn resolve(api: &Api, key: &str, token: &str) -> (StatusCode, Value) {
    resolve_on(&api.app, key, token).await
}

fn token_of(link: &Value) -> String {
    link["token"].as_str().unwrap().to_owned()
}

/// Moves a link's whole lifetime into the past; the immutability trigger is paused for it.
async fn backdate(api: &Api, link: &str) {
    let mut tx = api.pool.begin().await.unwrap();
    for sql in [
        "ALTER TABLE storefront_preview_links DISABLE TRIGGER USER",
        "UPDATE storefront_preview_links SET created_at=created_at-interval '3 days',expires_at=expires_at-interval '3 days' WHERE id=$1::uuid",
        "ALTER TABLE storefront_preview_links ENABLE TRIGGER USER",
    ] {
        sqlx::query(sql).bind(link).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
}

/// Rows anywhere in these tables whose text holds `plain` as text or as hex bytes.
async fn plaintext_rows(pool: &PgPool, plain: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM idempotency_keys i WHERE i::text LIKE '%'||$1||'%' OR i::text LIKE '%'||encode(convert_to($1,'UTF8'),'hex')||'%')
              + (SELECT count(*) FROM storefront_preview_links l WHERE l::text LIKE '%'||$1||'%' OR l::text LIKE '%'||encode(convert_to($1,'UTF8'),'hex')||'%')",
    )
    .bind(plain)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[test]
fn every_preview_link_operation_is_documented() {
    let spec = genesy_360_api::openapi::spec().unwrap();
    let links = "/workspace/{workspace}/storefront/pages/home/preview-links";
    for (method, path, id) in [
        ("post", links, "storefront_design_preview_link_create"),
        ("get", links, "storefront_design_preview_links"),
        (
            "delete",
            "/workspace/{workspace}/storefront/pages/home/preview-links/{link}",
            "storefront_design_preview_link_revoke",
        ),
        (
            "post",
            "/storefront/preview",
            "storefront_design_preview_resolve",
        ),
    ] {
        assert_eq!(spec["paths"][path][method]["operationId"], id, "{path}");
    }
    let create = &spec["paths"][links]["post"];
    assert!(
        create["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "Idempotency-Key")
    );
}

#[sqlx::test]
async fn issue_resolve_and_store_only_the_hash(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let products = api.with_base("/products");
    products
        .create(json!({"name":"Honey","price":10}), "product_id")
        .await;
    let revision = draft(&api).await;
    let key = app_key(&api, json!(["catalog:read"])).await;

    let link = issue(&api, &revision, "issue").await;
    let token = token_of(&link);
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(link["revision_id"], revision.as_str());
    assert_eq!(link["status"], "active");
    assert!(link["revoked_at"].is_null());
    let hours: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM expires_at-created_at)::float8/3600 FROM storefront_preview_links WHERE id=$1::uuid",
    )
    .bind(link["link_id"].as_str().unwrap())
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(hours, 48.0, "default lifetime");
    assert_eq!(plaintext_rows(&api.pool, &token).await, 0);

    let (status, headers, bytes) = common::send_full(
        &api.app,
        Method::POST,
        RESOLVE,
        Some(json!({"token":token})),
        Some(&key),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["cache-control"], "no-store");
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    common::contract::check(&Method::POST, RESOLVE, status, &body);
    let data = body["data"].as_object().unwrap();
    let mut keys: Vec<_> = data.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["collections", "document", "expires_at", "revision_id"],
        "no page version, pointers, actors, link or workspace ids"
    );
    assert_eq!(data["revision_id"], revision.as_str());
    assert_eq!(data["expires_at"], link["expires_at"]);
    assert_eq!(data["document"], document());
    let (_, catalog) = common::send(
        &api.app,
        Method::GET,
        "/v1/storefront/products?page_size=6",
        None,
        Some(&key),
    )
    .await;
    assert_eq!(
        data["collections"][0]["products"], catalog["data"],
        "same public product fields as the live storefront"
    );
    assert!(!String::from_utf8_lossy(&bytes).contains(&token));
}

#[sqlx::test]
async fn hours_bounds_key_rules_and_replay_without_token(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let revision = draft(&api).await;
    for hours in [0, 169, -1] {
        let (status, body) = issue_as(
            &api,
            json!({"revision_id":revision,"hours":hours}),
            &format!("h{hours}"),
            &api.owner,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(body["errors"]["hours"].is_array(), "{body}");
    }
    let (status, body) = common::send(
        &api.app,
        Method::POST,
        &links_uri(&api),
        Some(json!({"revision_id":revision})),
        Some(&api.owner),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    for (status, body) in [
        issue_as(&api, json!({"revision_id":"bad"}), "bad", &api.owner).await,
        issue_as(&api, json!({}), "none", &api.owner).await,
    ] {
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }

    let body = json!({"revision_id":revision,"hours":168});
    let (status, first) = issue_as(&api, body.clone(), "week", &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let token = token_of(&first["data"]);
    let (status, again) = issue_as(&api, body, "week", &api.owner).await;
    assert_eq!(status, StatusCode::CREATED, "{again}");
    assert_eq!(again["data"]["link_id"], first["data"]["link_id"]);
    assert!(
        again["data"]["token"].is_null(),
        "replay never carries the token"
    );
    assert_eq!(plaintext_rows(&api.pool, &token).await, 0);
    let hours: f64 = sqlx::query_scalar("SELECT extract(epoch FROM expires_at-created_at)::float8/3600 FROM storefront_preview_links WHERE workspace_id=$1::uuid")
        .bind(&api.ws)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(hours, 168.0);
}

#[sqlx::test]
async fn admins_only_and_other_workspaces_are_refused(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let revision = draft(&api).await;
    let link = issue(&api, &revision, "issue").await;
    let link_id = link["link_id"].as_str().unwrap();
    let token = token_of(&link);
    let member = common::member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "member@example.com",
        &["setting.get", "setting.update"],
    )
    .await;
    let admin = common::member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "admin@example.com",
        &["workspace_admin"],
    )
    .await;
    assert_eq!(
        issue_as(&api, json!({"revision_id":revision}), "m", &member)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        list(&api, &revision, &member).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        revoke(&api, link_id, &member).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, rows) = list(&api, &revision, &admin).await;
    assert_eq!(status, StatusCode::OK, "{rows}");
    assert_eq!(rows["data"].as_array().unwrap().len(), 1);
    assert!(
        rows["data"][0].get("token").is_none(),
        "list never shows a token"
    );

    let other = api.as_new_owner("other@example.com", "other").await;
    assert_eq!(
        issue_as(
            &other,
            json!({"revision_id":revision}),
            "foreign",
            &other.owner
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (status, rows) = list(&other, &revision, &other.owner).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rows["data"], json!([]));
    assert_eq!(
        revoke(&other, link_id, &other.owner).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        revoke(&api, link_id, &other.owner).await.0,
        StatusCode::NOT_FOUND,
        "foreign workspace in path"
    );

    let foreign_key = app_key(&other, json!(["catalog:read"])).await;
    assert_eq!(
        resolve(&api, &foreign_key, &token).await.0,
        StatusCode::NOT_FOUND,
        "a Shop key of another workspace never resolves this link"
    );
    let unscoped = app_key(&api, json!(["checkout:write"])).await;
    assert_eq!(
        resolve(&api, &unscoped, &token).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        resolve(&api, &api.owner, &token).await.0,
        StatusCode::UNAUTHORIZED,
        "a member login is no app key"
    );
    let key = app_key(&api, json!(["catalog:read"])).await;
    assert_eq!(resolve(&api, &key, &token).await.0, StatusCode::OK);
}

#[sqlx::test]
async fn unknown_tampered_expired_and_revoked_tokens_look_the_same(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let revision = draft(&api).await;
    let key = app_key(&api, json!(["catalog:read"])).await;
    let expired = issue(&api, &revision, "expired").await;
    let revoked = issue(&api, &revision, "revoked").await;
    let live = token_of(&issue(&api, &revision, "live").await);
    backdate(&api, expired["link_id"].as_str().unwrap()).await;
    let (status, row) = revoke(&api, revoked["link_id"].as_str().unwrap(), &api.owner).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["data"]["status"], "revoked");
    assert!(row["data"]["revoked_at"].is_string());
    let again = revoke(&api, revoked["link_id"].as_str().unwrap(), &api.owner).await;
    assert_eq!(again, (status, row.clone()), "revoke twice: same answer");
    let unknown_link = revoke(&api, "019a0000-0000-7000-8000-000000000000", &api.owner).await;
    assert_eq!(unknown_link.0, StatusCode::NOT_FOUND);

    let mut tampered = live.clone();
    let last = if tampered.ends_with('0') { "1" } else { "0" };
    tampered.replace_range(63.., last);
    let (status, generic) = resolve(&api, &key, &"f".repeat(64)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for token in [
        tampered,
        live.to_uppercase(),
        live[..63].to_owned(),
        format!("{live}0"),
        String::new(),
        "not a token".into(),
        token_of(&expired),
        token_of(&revoked),
    ] {
        assert_eq!(
            resolve(&api, &key, &token).await,
            (status, generic.clone()),
            "{token}"
        );
    }
    assert!(!generic.to_string().contains(&live));
    assert_eq!(resolve(&api, &key, &live).await.0, StatusCode::OK);

    let (status, rows) = list(&api, &revision, &api.owner).await;
    assert_eq!(status, StatusCode::OK, "{rows}");
    let statuses: Vec<_> = rows["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["active", "revoked", "expired"], "newest first");
}

#[sqlx::test]
async fn link_ends_when_draft_is_superseded_or_published(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let first = draft(&api).await;
    let key = app_key(&api, json!(["catalog:read"])).await;
    let old = token_of(&issue(&api, &first, "old").await);
    let mut changed = document();
    changed["theme"] = json!("neutral");
    let state = design(
        &api,
        "/revisions",
        json!({"expected_version":1,"document":changed}),
        "second",
    )
    .await;
    let second = state["draft_revision_id"].as_str().unwrap().to_owned();
    assert_eq!(
        resolve(&api, &key, &old).await.0,
        StatusCode::NOT_FOUND,
        "superseded"
    );
    assert_eq!(
        list(&api, &first, &api.owner).await.1["data"][0]["status"],
        "ended"
    );
    let (status, body) = issue_as(&api, json!({"revision_id":first}), "stale", &api.owner).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, _) = issue_as(
        &api,
        json!({"revision_id":"019a0000-0000-7000-8000-000000000000"}),
        "unknown",
        &api.owner,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let current = token_of(&issue(&api, &second, "current").await);
    assert_eq!(resolve(&api, &key, &current).await.0, StatusCode::OK);
    design(
        &api,
        "/publish",
        json!({"expected_version":2,"revision_id":second}),
        "publish",
    )
    .await;
    assert_eq!(
        resolve(&api, &key, &current).await.0,
        StatusCode::NOT_FOUND,
        "published"
    );
    assert_eq!(
        list(&api, &second, &api.owner).await.1["data"][0]["status"],
        "ended"
    );
    let (status, body) = issue_as(&api, json!({"revision_id":second}), "live", &api.owner).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}

#[sqlx::test]
async fn at_most_five_active_links_per_revision(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let revision = draft(&api).await;
    let mut links = Vec::new();
    for n in 0..5 {
        links.push(issue(&api, &revision, &format!("k{n}")).await);
    }
    let (status, body) = issue_as(&api, json!({"revision_id":revision}), "k5", &api.owner).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    revoke(&api, links[0]["link_id"].as_str().unwrap(), &api.owner).await;
    backdate(&api, links[1]["link_id"].as_str().unwrap()).await;
    issue(&api, &revision, "k6").await;
    issue(&api, &revision, "k7").await;
    let (status, _) = issue_as(&api, json!({"revision_id":revision}), "k8", &api.owner).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test]
async fn link_rows_are_append_only_except_one_revoke(pool: PgPool) {
    let api = Api::new(pool, "/storefront/pages/home").await;
    let revision = draft(&api).await;
    let link = issue(&api, &revision, "issue").await;
    let id = link["link_id"].as_str().unwrap();
    for sql in [
        "UPDATE storefront_preview_links SET expires_at=expires_at+interval '1 hour' WHERE id=$1::uuid",
        "UPDATE storefront_preview_links SET token_hash=sha256('x') WHERE id=$1::uuid",
        "DELETE FROM storefront_preview_links WHERE id=$1::uuid",
    ] {
        assert!(
            sqlx::query(sql).bind(id).execute(&api.pool).await.is_err(),
            "{sql}"
        );
    }
    revoke(&api, id, &api.owner).await;
    assert!(
        sqlx::query("UPDATE storefront_preview_links SET revoked_at=NULL,revoked_by=NULL,revoked_login_id=NULL WHERE id=$1::uuid")
            .bind(id)
            .execute(&api.pool)
            .await
            .is_err(),
        "no un-revoke"
    );
    sqlx::query("DELETE FROM workspaces WHERE id=$1::uuid")
        .bind(&api.ws)
        .execute(&api.pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn resolve_has_its_own_rate_limit(pool: PgPool) {
    let api = Api::new(pool.clone(), "/storefront/pages/home").await;
    let revision = draft(&api).await;
    let key = app_key(&api, json!(["catalog:read"])).await;
    let token = token_of(&issue(&api, &revision, "issue").await);
    let (mailer, _) = genesy_360_api::mail::Mailer::capture();
    let mut state = common::state(pool, mailer);
    state.preview_rate_limit = genesy_360_api::RateLimit {
        burst: 3,
        replenish: std::time::Duration::from_secs(3600),
        trust_forwarded: false,
    };
    let app = genesy_360_api::build_app(state);
    for _ in 0..3 {
        assert_eq!(resolve_on(&app, &key, &token).await.0, StatusCode::OK);
    }
    let (status, headers, _) = common::send_full(
        &app,
        Method::POST,
        RESOLVE,
        Some(json!({"token":token})),
        Some(&key),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let wait: u64 = headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(wait >= 1, "Retry-After {wait}");
    let (status, _) = common::send(
        &app,
        Method::GET,
        "/v1/storefront/pages/home",
        None,
        Some(&key),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "other storefront reads keep their own limit"
    );
}
