//! /v1/workspace/{ws}/integrations: connections (secrets write-only, probe, expiry status,
//! address guard), pairs and mappings (contracts §5.2).

use crate::common;

use axum::{
    Router,
    http::{HeaderMap, Method, StatusCode},
    routing::post,
};
use common::{app_with_mail, member_with, send, signup, state};
use serde_json::{Value, json};
use sqlx::PgPool;
use verzo_360_api::{
    build_app,
    integrations::{
        Runtime,
        capability::{CapabilityCheck, ErrorClass},
        connections::service as connections,
        probe,
        providers::{Mode, fake::Fake},
    },
    mail::Mailer,
    outbound::Outbound,
};

const NOT_FOUND: &str = "The requested resource could not be found.";
const UNAUTHORIZED: &str = "This action is unauthorized.";
const SECRET: &str = "odoo-key-PLAIN-1234567890";
const PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg\n-----END PRIVATE KEY-----";

struct World {
    app: Router,
    pool: PgPool,
    owner: String,
    ws: String,
    fake: Fake,
}

/// Ada owns `acme`; providers answer from the in-process fake.
async fn world(pool: PgPool) -> World {
    let fake = Fake::default();
    let mut st = state(pool.clone(), Mailer::capture().0);
    st.integrations = Runtime::fake(fake.clone()).unwrap();
    let app = build_app(st);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    World {
        app,
        pool,
        owner,
        ws,
        fake,
    }
}

impl World {
    fn uri(&self, path: &str) -> String {
        format!("/v1/workspace/{}/integrations{path}", self.ws)
    }
    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        send(&self.app, method, &self.uri(path), body, Some(&self.owner)).await
    }
    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, path, Some(body)).await
    }
    async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.call(Method::GET, path, None).await
    }
    async fn odoo(&self, label: &str) -> String {
        let (s, b) = self.post("/connections", odoo(label)).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["data"]["connection_id"].as_str().unwrap().to_owned()
    }
    async fn netsuite(&self, label: &str) -> String {
        let (s, b) = self.post("/connections", netsuite(label)).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["data"]["connection_id"].as_str().unwrap().to_owned()
    }
    async fn activate_conn(&self, id: &str) {
        let (s, b) = self
            .post(&format!("/connections/{id}/test"), json!({}))
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b["data"]["status"], "active", "{b}");
    }
    async fn pair(&self) -> (String, String, String) {
        let o = self.odoo("O").await;
        let n = self.netsuite("N").await;
        let (s, b) = self
            .post(
                "/pairs",
                json!({"source_connection_id": o, "target_connection_id": n}),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        (b["data"]["pair_id"].as_str().unwrap().to_owned(), o, n)
    }
}

fn odoo(label: &str) -> Value {
    json!({
        "provider": "odoo", "label": label,
        "config": {"base_url": "https://acme.odoo.com", "database": null},
        "secrets": {"api_key": SECRET},
        "credential_expires_at": "2099-01-01T00:00:00Z",
    })
}

fn netsuite(label: &str) -> Value {
    json!({
        "provider": "netsuite", "label": label,
        "config": {"rest_domain": "1234567.suitetalk.api.netsuite.com", "environment": "production",
                   "client_id": "abc", "certificate_id": "kid1", "alg": "ES256"},
        "secrets": {"private_key": PEM},
    })
}

// ---------- connections ----------

#[sqlx::test]
async fn create_get_list_mask_the_secret(pool: PgPool) {
    let w = world(pool).await;
    let id = w.odoo("Odoo main").await;

    let (s, b) = w.get(&format!("/connections/{id}")).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let c = &b["data"];
    assert_eq!(c["provider"], "odoo");
    assert_eq!(c["status"], "draft");
    assert_eq!(c["status_reason"], Value::Null);
    assert_eq!(c["secrets"], json!({"api_key": "********"}));
    assert_eq!(c["config"]["base_url"], "https://acme.odoo.com");
    assert_eq!(c["last_probe"], Value::Null);
    assert_eq!(c["credential_expires_at"], "2099-01-01T00:00:00Z");

    let ns = w.netsuite("NS").await;
    let (s, b) = w.get("/connections").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["meta"]["total"], 2);
    let listed = b["data"].as_array().unwrap();
    assert!(
        listed
            .iter()
            .any(|c| c["id"] == ns.as_str() && c["secrets"] == json!({"private_key": "********"}))
    );
    for body in [
        b.to_string(),
        w.get(&format!("/connections/{ns}")).await.1.to_string(),
    ] {
        assert!(
            !body.contains(SECRET) && !body.contains("BEGIN PRIVATE"),
            "{body}"
        );
    }
}

#[sqlx::test]
async fn secret_is_sealed_never_plain_in_the_database(pool: PgPool) {
    let w = world(pool).await;
    let id = w.odoo("O").await;
    let sealed: Vec<u8> = sqlx::query_scalar("SELECT sealed FROM integration_secrets")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert!(!sealed.windows(SECRET.len()).any(|x| x == SECRET.as_bytes()));
    // No other column of any integration table holds it.
    let dump: String = sqlx::query_scalar(
        "SELECT concat_ws('|', (SELECT string_agg(c::text, '|') FROM integration_connections c),
                              (SELECT string_agg(s.name || s.key_id, '|') FROM integration_secrets s))",
    )
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert!(!dump.contains(SECRET), "{dump}");
    assert!(dump.contains(&id));
}

#[sqlx::test]
async fn create_validates_fields(pool: PgPool) {
    let w = world(pool).await;
    let (s, b) = w
        .post(
            "/connections",
            json!({"provider": "odoo", "config": {"base_url": "ftp://x", "extra": 1}}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let e = &b["errors"];
    assert!(e["label"].is_array(), "{b}");
    assert!(e["config.base_url"].is_array(), "{b}");
    assert!(e["config.extra"].is_array(), "{b}");

    let (s, b) = w
        .post(
            "/connections",
            json!({"provider": "sap", "label": "x", "config": {}}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["provider"].is_array(), "{b}");

    // Secret name of the other provider.
    let mut body = odoo("x");
    body["secrets"] = json!({"private_key": PEM});
    let (s, b) = w.post("/connections", body).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["secrets.private_key"].is_array(), "{b}");

    // The mask on create: nothing is stored to keep.
    let mut body = odoo("x");
    body["secrets"] = json!({"api_key": "********"});
    let (s, b) = w.post("/connections", body).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        b["errors"]["secrets.api_key"][0],
        "Enter this credential again."
    );

    let mut body = netsuite("x");
    body["config"]["alg"] = json!("RS256");
    body["config"]["environment"] = json!("dev");
    let (s, b) = w.post("/connections", body).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        b["errors"]["config.alg"].is_array() && b["errors"]["config.environment"].is_array(),
        "{b}"
    );
}

#[sqlx::test]
async fn label_unique_per_provider_any_case(pool: PgPool) {
    let w = world(pool).await;
    w.odoo("Main").await;
    let (s, b) = w.post("/connections", odoo("MAIN")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert!(b["errors"]["label"].is_array(), "{b}");
    w.netsuite("Main").await;
}

#[sqlx::test]
async fn outbound_guard_refuses_inward_hosts(pool: PgPool) {
    let w = world(pool).await;
    for url in [
        "https://127.0.0.1",
        "https://10.0.0.5",
        "https://192.168.1.1",
        "https://169.254.169.254",
        "https://[::1]",
        "https://localhost",
        "https://odoo.localhost",
        "https://intranet",
        "http://acme.odoo.com",
        "https://acme.odoo.com/odoo",
        "https://user:pw@acme.odoo.com",
        "https://acme.odoo.com?x=1",
    ] {
        let mut body = odoo("x");
        body["config"]["base_url"] = json!(url);
        let (s, b) = w.post("/connections", body).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{url}: {b}");
        assert!(b["errors"]["config.base_url"].is_array(), "{url}: {b}");
    }
    for host in ["10.1.2.3", "localhost", "a.b/c", "x.com:8443", "metadata"] {
        let mut body = netsuite("x");
        body["config"]["rest_domain"] = json!(host);
        let (s, b) = w.post("/connections", body).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{host}: {b}");
        assert!(b["errors"]["config.rest_domain"].is_array(), "{host}: {b}");
    }
    // A trailing slash is the same origin.
    let mut body = odoo("ok");
    body["config"]["base_url"] = json!("https://acme.odoo.com/");
    assert_eq!(w.post("/connections", body).await.0, StatusCode::CREATED);
}

#[sqlx::test]
async fn update_keeps_masked_secret_unless_host_moves(pool: PgPool) {
    let w = world(pool).await;
    let id = w.odoo("O").await;
    let path = format!("/connections/{id}");

    let (s, b) = w
        .post(
            &path,
            json!({"label": "Renamed", "secrets": {"api_key": "********"}}),
        )
        .await;
    assert_eq!((s, &b), (StatusCode::OK, &json!({"message": "ok"})));
    let (_, b) = w.get(&path).await;
    assert_eq!(b["data"]["label"], "Renamed");
    assert_eq!(b["data"]["secrets"]["api_key"], "********");

    // Mask + moved base_url: the kept key would go to the new host.
    let (s, b) = w
        .post(
            &path,
            json!({"config": {"base_url": "https://evil.example.com"}, "secrets": {"api_key": "********"}}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        b,
        json!({"message": "Enter this credential again.",
               "errors": {"secrets.api_key": ["Enter this credential again."]}})
    );
    // Moved without sending the secret at all: same refusal.
    let (s, _) = w
        .post(
            &path,
            json!({"config": {"base_url": "https://evil.example.com"}}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    // Moved with a new secret: fine.
    let (s, b) = w
        .post(
            &path,
            json!({"config": {"base_url": "https://new.odoo.com"}, "secrets": {"api_key": "new-key-123"}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, b) = w.get(&path).await;
    assert_eq!(b["data"]["config"]["base_url"], "https://new.odoo.com");

    // Provider is create-only.
    let (s, b) = w.post(&path, json!({"provider": "netsuite"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["provider"].is_array(), "{b}");
}

#[sqlx::test]
async fn no_credentials_key_refuses_a_secret(pool: PgPool) {
    let mut st = state(pool, Mailer::capture().0);
    st.credentials = None;
    let app = build_app(st);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let (s, b) = send(
        &app,
        Method::POST,
        &format!("/v1/workspace/{ws}/integrations/connections"),
        Some(odoo("O")),
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        b["errors"]["secrets.api_key"][0],
        "Credential storage isn't configured."
    );
}

#[sqlx::test]
async fn test_probe_sets_status(pool: PgPool) {
    let w = world(pool).await;
    let id = w.odoo("O").await;
    let test = format!("/connections/{id}/test");

    let (s, b) = w.post(&test, json!({})).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "active");
    assert_eq!(b["data"]["last_probe"].as_array().unwrap().len(), 3);
    assert!(b["data"]["last_probe_at"].is_string());

    let fail = |key: &str, class| CapabilityCheck {
        key: key.into(),
        ok: false,
        error_class: Some(class),
        detail: Some("refused".into()),
    };
    use verzo_360_api::integrations::capability::ErrorClass;
    w.fake
        .script(&id, vec![fail("purchase_request.read", ErrorClass::Auth)]);
    let (_, b) = w.post(&test, json!({})).await;
    assert_eq!(
        (
            b["data"]["status"].as_str(),
            b["data"]["status_reason"].as_str()
        ),
        (Some("unauthorized"), Some("auth_failed"))
    );

    w.fake.script(
        &id,
        vec![fail("purchase_request.read", ErrorClass::Permission)],
    );
    let (_, b) = w.post(&test, json!({})).await;
    assert_eq!(
        (
            b["data"]["status"].as_str(),
            b["data"]["status_reason"].as_str()
        ),
        (Some("degraded"), Some("permission_denied"))
    );

    w.fake.script(
        &id,
        vec![fail("source_controls.read", ErrorClass::NotFound)],
    );
    let (_, b) = w.post(&test, json!({})).await;
    assert_eq!(
        b["data"]["status_reason"],
        "probe_failed:source_controls.read"
    );

    // Within 14 days of expiry: degraded even when every check passes.
    w.fake.clear(&id);
    let soon = (chrono::Utc::now() + chrono::Duration::days(10)).to_rfc3339();
    w.post(
        &format!("/connections/{id}"),
        json!({"credential_expires_at": soon}),
    )
    .await;
    let (_, b) = w.post(&test, json!({})).await;
    assert_eq!(
        (
            b["data"]["status"].as_str(),
            b["data"]["status_reason"].as_str()
        ),
        (Some("degraded"), Some("credential_expiring"))
    );

    // Past expiry: expired, whatever the probe says.
    let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    w.post(
        &format!("/connections/{id}"),
        json!({"credential_expires_at": past}),
    )
    .await;
    let (_, b) = w.post(&test, json!({})).await;
    assert_eq!(
        (
            b["data"]["status"].as_str(),
            b["data"]["status_reason"].as_str()
        ),
        (Some("expired"), Some("credential_expired"))
    );

    // No expiry date: active again.
    w.post(
        &format!("/connections/{id}"),
        json!({"credential_expires_at": null}),
    )
    .await;
    w.activate_conn(&id).await;
}

#[sqlx::test]
async fn live_netsuite_probe_with_unusable_key_is_unauthorized(pool: PgPool) {
    let (app, _) = app_with_mail(pool);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let base = format!("/v1/workspace/{ws}/integrations/connections");
    let (_, b) = send(&app, Method::POST, &base, Some(netsuite("N")), Some(&owner)).await;
    let id = b["data"]["connection_id"].as_str().unwrap().to_owned();
    let (s, b) = send(
        &app,
        Method::POST,
        &format!("{base}/{id}/test"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let checks = b["data"]["last_probe"].as_array().unwrap();
    assert_eq!(checks.len(), 4);
    // The C3b adapter refuses the truncated test PEM before any request leaves.
    assert!(
        checks
            .iter()
            .all(|c| c["ok"] == false && c["error_class"] == "auth")
    );
    assert_eq!(b["data"]["status"], "unauthorized");
    assert_eq!(b["data"]["status_reason"], "auth_failed");
}

/// Fake Odoo 19: every JSON-2 call answers `[]` for the stored key, else the documented 401
/// (facts odoo-19.md L27-28, L37).
async fn fake_odoo() -> String {
    let app = Router::new().route(
        "/json/2/{model}/{method}",
        post(|headers: HeaderMap| async move {
            let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
            if auth == Some(format!("bearer {SECRET}").as_str()) {
                (StatusCode::OK, "[]".to_owned())
            } else {
                let body = json!({"name": "werkzeug.exceptions.Unauthorized",
                    "message": "Invalid apikey", "arguments": [], "context": {}, "debug": ""});
                (StatusCode::UNAUTHORIZED, body.to_string())
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[sqlx::test]
async fn live_odoo_connection_probes_with_its_stored_key(pool: PgPool) {
    let st = state(pool.clone(), Mailer::capture().0);
    let app = build_app(st.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let base = format!("/v1/workspace/{ws}/integrations/connections");
    let (_, b) = send(&app, Method::POST, &base, Some(odoo("O")), Some(&owner)).await;
    let id: uuid::Uuid = b["data"]["connection_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    // The route only takes public https hosts; point the row at the loopback fake directly.
    let url = fake_odoo().await;
    sqlx::query("UPDATE integration_connections SET config = $2 WHERE id = $1")
        .bind(id)
        .bind(json!({"base_url": url, "database": "acme"}))
        .execute(&pool)
        .await
        .unwrap();
    let ws_id: uuid::Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM integration_connections WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut ctx = connections::ctx(&st);
    ctx.outbound = Outbound::new(&["127.0.0.1".to_owned()]).unwrap();
    ctx.mode = Mode::Live;

    let row = connections::load(&pool, ws_id, id).await.unwrap();
    let checks = probe::run(&ctx, &row).await;
    let keys: Vec<_> = checks.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "purchase_request.list",
            "purchase_request.read",
            "source_controls.read"
        ]
    );
    assert!(checks.iter().all(|c| c.ok), "{checks:?}");

    // No stored key: every check fails as auth, without calling Odoo.
    sqlx::query("DELETE FROM integration_secrets WHERE connection_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let checks = probe::run(&ctx, &row).await;
    assert!(
        checks
            .iter()
            .all(|c| !c.ok && c.error_class == Some(ErrorClass::Auth)),
        "{checks:?}"
    );
}

#[sqlx::test]
async fn disconnect_drops_secret_and_new_secret_returns_to_draft(pool: PgPool) {
    let w = world(pool).await;
    let id = w.odoo("O").await;
    w.activate_conn(&id).await;
    let (s, b) = w
        .post(&format!("/connections/{id}/disconnect"), json!({}))
        .await;
    assert_eq!((s, b), (StatusCode::OK, json!({"message": "ok"})));
    let (_, b) = w.get(&format!("/connections/{id}")).await;
    assert_eq!(b["data"]["status"], "disconnected");
    assert_eq!(b["data"]["secrets"]["api_key"], Value::Null);
    // A probe never revives a disconnected connection.
    let (_, b) = w.post(&format!("/connections/{id}/test"), json!({})).await;
    assert_eq!(b["data"]["status"], "disconnected");

    w.post(
        &format!("/connections/{id}"),
        json!({"secrets": {"api_key": "fresh-key"}}),
    )
    .await;
    let (_, b) = w.get(&format!("/connections/{id}")).await;
    assert_eq!(b["data"]["status"], "draft");
    assert_eq!(b["data"]["secrets"]["api_key"], "********");
}

#[sqlx::test]
async fn other_workspace_and_bad_ids_are_404(pool: PgPool) {
    let w = world(pool).await;
    let id = w.odoo("Lone").await;
    let (pair, _, _) = w.pair().await;
    let (bob, bob_ws) = signup(&w.app, "bob@example.com", "bobco").await;
    let foreign = |p: String| format!("/v1/workspace/{bob_ws}/integrations{p}");
    for (method, path) in [
        (Method::GET, format!("/connections/{id}")),
        (Method::POST, format!("/connections/{id}")),
        (Method::POST, format!("/connections/{id}/test")),
        (Method::POST, format!("/connections/{id}/disconnect")),
        (Method::GET, format!("/pairs/{pair}")),
        (Method::POST, format!("/pairs/{pair}/activate")),
        (Method::POST, format!("/pairs/{pair}/pause")),
        (Method::GET, format!("/pairs/{pair}/mappings")),
        (Method::GET, "/connections/not-a-uuid".to_owned()),
        (Method::GET, "/pairs/not-a-uuid".to_owned()),
    ] {
        let (s, b) = send(
            &w.app,
            method.clone(),
            &foreign(path.clone()),
            Some(json!({})),
            Some(&bob),
        )
        .await;
        assert_eq!(
            (s, b["message"].as_str()),
            (StatusCode::NOT_FOUND, Some(NOT_FOUND)),
            "{method} {path}"
        );
    }
    // Bob's own lists don't show Ada's rows.
    let (_, b) = send(
        &w.app,
        Method::GET,
        &foreign("/connections".into()),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(b["meta"]["total"], 0);
    // Bob can't pair Ada's connections.
    let o = w.odoo("O2").await;
    let n = w.netsuite("N2").await;
    let (s, b) = send(
        &w.app,
        Method::POST,
        &foreign("/pairs".into()),
        Some(json!({"source_connection_id": o, "target_connection_id": n})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
}

#[sqlx::test]
async fn permissions_view_and_manage(pool: PgPool) {
    let w = world(pool).await;
    let id = w.odoo("O").await;
    let viewer = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "v@example.com",
        &["integration.view"],
    )
    .await;
    let nobody = member_with(&w.app, &w.pool, &w.ws, "n@example.com", &[]).await;
    let manager = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "m@example.com",
        &["integration.manage"],
    )
    .await;

    let get = w.uri(&format!("/connections/{id}"));
    assert_eq!(
        send(&w.app, Method::GET, &get, None, Some(&viewer)).await.0,
        StatusCode::OK
    );
    let (s, b) = send(&w.app, Method::GET, &get, None, Some(&nobody)).await;
    assert_eq!(
        (s, b["message"].as_str()),
        (StatusCode::FORBIDDEN, Some(UNAUTHORIZED))
    );
    for (path, body) in [
        ("/connections".to_owned(), odoo("V")),
        (format!("/connections/{id}"), json!({"label": "x"})),
        (format!("/connections/{id}/test"), json!({})),
        (format!("/connections/{id}/disconnect"), json!({})),
        ("/pairs".to_owned(), json!({})),
    ] {
        let (s, _) = send(
            &w.app,
            Method::POST,
            &w.uri(&path),
            Some(body),
            Some(&viewer),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}");
    }
    let (s, _) = send(
        &w.app,
        Method::POST,
        &w.uri("/connections"),
        Some(odoo("M")),
        Some(&manager),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(
        send(&w.app, Method::GET, &w.uri("/connections"), None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

// ---------- pairs ----------

#[sqlx::test]
async fn pair_create_defaults_patch_activate_pause(pool: PgPool) {
    let w = world(pool).await;
    let (pair, o, n) = w.pair().await;
    let path = format!("/pairs/{pair}");
    let (s, b) = w.get(&path).await;
    assert_eq!(s, StatusCode::OK);
    let p = &b["data"];
    assert_eq!(p["workflow_key"], "wf01_purchase");
    assert_eq!(p["status"], "draft");
    assert_eq!(
        p["config"],
        json!({"poll_minutes": 5, "backfill_days": 0, "full_scan_minutes": 60, "succeeded_watch_days": 30,
               "tax_tolerance_minor": 0, "default_locations": {}})
    );

    let (s, b) = w
        .post(&path, json!({"config": {"poll_minutes": 1441}}))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["config.poll_minutes"].is_array(), "{b}");
    let (s, b) = w.post(&path, json!({"config": {"nope": 1}})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (s, _) = w
        .post(
            &path,
            json!({"config": {"poll_minutes": 10, "default_locations": {"1": "7"}}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, b) = w.get(&path).await;
    assert_eq!(b["data"]["config"]["poll_minutes"], 10);
    assert_eq!(b["data"]["config"]["full_scan_minutes"], 60);
    assert_eq!(b["data"]["config"]["default_locations"], json!({"1": "7"}));

    // Connections still draft.
    let (s, b) = w.post(&format!("{path}/activate"), json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b["code"], "connection_not_active");

    w.activate_conn(&o).await;
    w.activate_conn(&n).await;
    // WF-01 governs mapped companies only: none mapped → refused (R3-§2).
    let (s, b) = w.post(&format!("{path}/activate"), json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["code"], "source_controls_unmet");
    let (s, b) = w
        .post(
            &format!("{path}/mappings"),
            json!({"entity": "company", "source_id": "1", "target_id": "3"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let (s, b) = w.post(&format!("{path}/activate"), json!({})).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "active");
    assert!(b["data"]["activated_at"].is_string());

    let (s, b) = w.post(&format!("{path}/pause"), json!({})).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (
            b["data"]["status"].as_str(),
            b["data"]["paused_reason"].as_str()
        ),
        (Some("paused"), Some("manual"))
    );

    let (_, b) = w.get("/pairs").await;
    assert_eq!(b["meta"]["total"], 1);
}

#[sqlx::test]
async fn pair_refuses_wrong_providers_and_duplicates(pool: PgPool) {
    let w = world(pool).await;
    let o = w.odoo("O").await;
    let n = w.netsuite("N").await;
    let (s, b) = w
        .post(
            "/pairs",
            json!({"source_connection_id": n, "target_connection_id": o}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        b["errors"]["source_connection_id"].is_array()
            && b["errors"]["target_connection_id"].is_array(),
        "{b}"
    );
    let (s, b) = w.post("/pairs", json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["source_connection_id"].is_array(), "{b}");
    let ok = json!({"source_connection_id": o, "target_connection_id": n});
    assert_eq!(w.post("/pairs", ok.clone()).await.0, StatusCode::CREATED);
    let (s, b) = w.post("/pairs", ok).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
}

// ---------- mappings ----------

#[sqlx::test]
async fn mappings_upsert_list_delete(pool: PgPool) {
    let w = world(pool).await;
    let (pair, _, _) = w.pair().await;
    let base = format!("/pairs/{pair}/mappings");
    let vendor = json!({"entity": "vendor", "source_id": "42", "target_id": "1187",
                        "source_label": "Azure Interior", "target_label": "Azure Ltd"});
    let (s, b) = w.post(&base, vendor.clone()).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let id = b["data"]["mapping_id"].as_str().unwrap().to_owned();

    // Same (entity, source_id): one row, target replaced, same id.
    let mut again = vendor.clone();
    again["target_id"] = json!("1188");
    let (s, b) = w.post(&base, again).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(b["data"]["mapping_id"], id.as_str());
    w.post(
        &base,
        json!({"entity": "product", "source_id": "42", "target_id": "9"}),
    )
    .await;

    let (_, b) = w.get(&format!("{base}?entity=vendor")).await;
    assert_eq!(b["meta"]["total"], 1);
    assert_eq!(b["data"][0]["target_id"], "1188");
    assert_eq!(b["data"][0]["pair_id"], pair.as_str());
    let (_, b) = w.get(&base).await;
    assert_eq!(b["meta"]["total"], 2);
    let (s, b) = w.get(&format!("{base}?entity=tax")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(b["errors"]["entity"].is_array());

    let (s, b) = w
        .post(
            &base,
            json!({"entity": "tax", "source_id": "", "target_id": "x".repeat(101)}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    for f in ["entity", "source_id", "target_id"] {
        assert!(b["errors"][f].is_array(), "{f}: {b}");
    }

    let (s, b) = w.call(Method::DELETE, &format!("{base}/{id}"), None).await;
    assert_eq!((s, b), (StatusCode::OK, json!({"message": "ok"})));
    let (s, _) = w.call(Method::DELETE, &format!("{base}/{id}"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn user_mapping_needs_an_active_member(pool: PgPool) {
    let w = world(pool).await;
    let (pair, _, _) = w.pair().await;
    let base = format!("/pairs/{pair}/mappings");
    let ada: String =
        sqlx::query_scalar("SELECT id::text FROM users WHERE email = 'ada@example.com'")
            .fetch_one(&w.pool)
            .await
            .unwrap();
    let (s, b) = w
        .post(
            &base,
            json!({"entity": "user", "source_id": "2", "target_id": ada}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");

    signup(&w.app, "out@example.com", "elsewhere").await;
    let outsider: String =
        sqlx::query_scalar("SELECT id::text FROM users WHERE email = 'out@example.com'")
            .fetch_one(&w.pool)
            .await
            .unwrap();
    for target in [outsider.as_str(), "not-a-uuid"] {
        let (s, b) = w
            .post(
                &base,
                json!({"entity": "user", "source_id": "3", "target_id": target}),
            )
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            b["errors"]["target_id"][0],
            "The selected target id is invalid."
        );
    }
}

#[sqlx::test]
async fn mapping_routes_refuse_other_workspace_pair(pool: PgPool) {
    let w = world(pool).await;
    let (pair, _, _) = w.pair().await;
    let (s, b) = w
        .post(
            &format!("/pairs/{pair}/mappings"),
            json!({"entity": "vendor", "source_id": "1", "target_id": "2"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let mapping = b["data"]["mapping_id"].as_str().unwrap().to_owned();
    let (bob, bob_ws) = signup(&w.app, "bob@example.com", "bobco").await;
    let uri = format!("/v1/workspace/{bob_ws}/integrations/pairs/{pair}/mappings");
    let (s, _) = send(
        &w.app,
        Method::POST,
        &uri,
        Some(json!({"entity": "vendor", "source_id": "1", "target_id": "3"})),
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = send(
        &w.app,
        Method::DELETE,
        &format!("{uri}/{mapping}"),
        None,
        Some(&bob),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
