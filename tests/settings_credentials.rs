//! Integration credentials in POST/GET /settings (F2b): the kept providers' keys are stored
//! sealed in `workspace_credentials`, GET prints a mask, the mask sent back keeps the stored
//! one, and nothing prints or stores the plaintext.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, signup};
use genesy_360_api::{CredentialKey, build_app, mail::Mailer};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MASK: &str = "********";
const RE_ENTER: &str = "Enter this credential again.";
const MOLLIE: &str = "integrations.payment.payment_providers.mollie.api_key";

/// The kept providers' credential paths.
const KEPT: &[&str] = &[
    "smtp.password",
    MOLLIE,
    "integrations.payment.payment_providers.pay.api_code",
    "integrations.payment.payment_providers.pay.api_token",
    "integrations.payment.payment_providers.pay.service_id",
    "integrations.mail.graph.client_secret",
];

fn key() -> CredentialKey {
    CredentialKey::from_hex(common::CREDENTIALS_KEY).unwrap()
}

/// `{a: {b: {c: v}}}` for "a.b.c".
fn nested(path: &str, v: Value) -> Value {
    path.rsplit('.').fold(v, |acc, k| json!({ k: acc }))
}

fn at<'a>(v: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(v, |acc, k| &acc[k])
}

/// Body that sets `path` to `v`; Graph needs its other fields when the driver is graph.
fn body(path: &str, v: Value) -> Value {
    let mut b = nested(path, v);
    if path.starts_with("integrations.mail.graph") {
        b["integrations"]["mail"]["graph"]["tenant_id"] = json!("t");
    }
    b
}

async fn post(api: &Api, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, "", Some(body), &api.owner).await
}

async fn saved(api: &Api, body: Value) {
    let (status, res) = post(api, body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

async fn get(api: &Api, query: &str) -> Value {
    let (status, res) = api.call(Method::GET, query, None, &api.owner).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].clone()
}

/// (key_id, sealed) of a stored credential.
async fn row(api: &Api, path: &str) -> Option<(String, Vec<u8>)> {
    sqlx::query_as(
        "SELECT key_id, sealed FROM workspace_credentials
         WHERE workspace_id = $1::uuid AND path = $2",
    )
    .bind(&api.ws)
    .bind(path)
    .fetch_optional(&api.pool)
    .await
    .unwrap()
}

async fn opened(api: &Api, path: &str) -> String {
    let (key_id, sealed) = row(api, path).await.expect("stored");
    assert_eq!(key_id, key().id());
    let ws: Uuid = api.ws.parse().unwrap();
    key().open(ws, path, &sealed).unwrap().expose().to_owned()
}

// --- happy ---------------------------------------------------------------------------------

#[sqlx::test]
async fn every_kept_credential_is_stored_sealed_and_printed_masked(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    for path in KEPT {
        let plain = format!("s3cret-{path}");
        saved(&api, body(path, json!(plain))).await;
        assert_eq!(opened(&api, path).await, plain, "{path}");
        let (_, sealed) = row(&api, path).await.unwrap();
        assert!(
            !sealed.windows(6).any(|w| w == b"s3cret"),
            "{path} stored in plaintext"
        );
        assert_eq!(at(&get(&api, "").await, path), MASK, "{path}");
        assert_eq!(get(&api, &format!("?type={path}")).await, json!([MASK]));
    }
    // The settings row itself never holds one.
    let data: Value = sqlx::query_scalar("SELECT data FROM settings WHERE workspace_id = $1::uuid")
        .bind(&api.ws)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert!(!data.to_string().contains("s3cret"), "{data}");
    let all = get(&api, "").await;
    assert!(!all.to_string().contains("s3cret"));
    // Each integrations save above replaced the section, so only the last one's credential
    // is left there; smtp is its own section.
    assert_eq!(at(&all, "smtp.password"), MASK);
    assert_eq!(at(&all, "integrations.mail.graph.client_secret"), MASK);
    assert_eq!(at(&all, MOLLIE), &Value::Null);
    assert!(row(&api, MOLLIE).await.is_none());
}

/// The web app posts the whole section back as GET printed it: the mask keeps the stored
/// value, a new value replaces it, and null, blank or a missing key clears it.
#[sqlx::test]
async fn mask_keeps_new_value_replaces_and_null_clears(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, body(MOLLIE, json!("test_one"))).await;
    let first = row(&api, MOLLIE).await.unwrap();

    let integrations = get(&api, "?type=integrations").await;
    saved(&api, json!({ "integrations": integrations })).await;
    assert_eq!(
        row(&api, MOLLIE).await.unwrap(),
        first,
        "mask kept it untouched"
    );
    saved(&api, json!({ "smtp": { "host": "mail.example.com" } })).await;
    assert_eq!(
        row(&api, MOLLIE).await.unwrap(),
        first,
        "other section left it"
    );

    saved(&api, body(MOLLIE, json!("test_two"))).await;
    assert_eq!(opened(&api, MOLLIE).await, "test_two");

    for clear in [Value::Null, json!(""), json!("   ")] {
        saved(&api, body(MOLLIE, json!("test_three"))).await;
        saved(&api, body(MOLLIE, clear.clone())).await;
        assert!(row(&api, MOLLIE).await.is_none(), "{clear} clears");
        assert_eq!(at(&get(&api, "").await, MOLLIE), &Value::Null);
    }
    // The section sent without the key (or with its provider object null) clears it too, as
    // the rest of the section resets (Laravel replaced the whole section).
    for sent in [
        json!({ "integrations": { "payment": { "active_provider": "mollie" } } }),
        json!({ "integrations": { "payment": null } }),
        json!({ "integrations": null }),
    ] {
        saved(&api, body(MOLLIE, json!("test_four"))).await;
        saved(&api, sent.clone()).await;
        assert!(row(&api, MOLLIE).await.is_none(), "{sent}");
    }
}

/// `?type=` with a stored credential: sections and providers show the mask, nothing deeper
/// or misspelled shows more.
#[sqlx::test]
async fn type_paths_show_only_the_mask(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, json!({ "smtp": { "host": "h", "password": "p4ss" } })).await;
    saved(&api, body(MOLLIE, json!("test_abc"))).await;
    assert_eq!(get(&api, "?type=smtp").await["password"], MASK);
    assert_eq!(
        get(&api, "?type=integrations.payment.payment_providers").await["mollie"],
        json!({ "api_key": MASK })
    );
    for q in [
        "smtp.password.x",
        "SMTP.password",
        "smtp.Password",
        "smtp..password",
    ] {
        assert_eq!(get(&api, &format!("?type={q}")).await, json!([]), "{q}");
    }
    for q in [
        "",
        "?type=smtp",
        "?type=integrations",
        "?type=smtp.password",
    ] {
        let got = get(&api, q).await.to_string();
        assert!(
            !got.contains("p4ss") && !got.contains("test_abc"),
            "{q}: {got}"
        );
    }
}

/// The mask keeps a credential only when one is stored, and a kept SMTP password only while
/// the server it is sent to stays the same: otherwise a member could point the stored
/// password at their own server.
#[sqlx::test]
async fn the_mask_keeps_only_a_stored_credential_on_its_own_server(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    for path in KEPT {
        let (status, res) = post(&api, body(path, json!(MASK))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {res}");
        assert_eq!(res["errors"][path][0], RE_ENTER, "{path}");
    }
    let smtp = json!({ "host": "mail.example.com", "port": 587, "username": "ada",
                       "encryption": "tls", "from_name": "Ada" });
    let mut first = smtp.clone();
    first["password"] = json!("p4ss");
    saved(&api, json!({ "smtp": first })).await;
    let stored = row(&api, "smtp.password").await.unwrap();
    for (field, other) in [
        ("host", json!("evil.example.com")),
        ("port", json!(2525)),
        ("username", json!("eve")),
        ("encryption", json!("ssl")),
        ("host", Value::Null),
    ] {
        let mut sent = smtp.clone();
        sent[field] = other.clone();
        sent["password"] = json!(MASK);
        let (status, res) = post(&api, json!({ "smtp": sent })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {res}");
        assert_eq!(res["errors"]["smtp.password"][0], RE_ENTER);
        assert_eq!(row(&api, "smtp.password").await.unwrap(), stored, "{field}");
        assert_eq!(
            get(&api, "?type=smtp.host").await,
            json!(["mail.example.com"])
        );
    }
    // Same server: kept, also with the port sent as a string and another sender name.
    let mut same = smtp.clone();
    same["port"] = json!("587");
    same["from_name"] = json!("Ada L");
    same["password"] = json!(MASK);
    saved(&api, json!({ "smtp": same })).await;
    assert_eq!(row(&api, "smtp.password").await.unwrap(), stored);
    // A new server with a new password is fine.
    let mut moved = smtp.clone();
    moved["host"] = json!("smtp.new.example.com");
    moved["password"] = json!("n3w");
    saved(&api, json!({ "smtp": moved })).await;
    assert_eq!(opened(&api, "smtp.password").await, "n3w");
    // Graph switched on with a mask but no stored secret is refused, not saved without one.
    let graph = json!({ "integrations": { "mail": { "driver": "graph", "graph": {
        "tenant_id": "t", "client_id": "c", "client_secret": MASK, "mailbox": "m@example.com" } } } });
    let (status, res) = post(&api, graph).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["integrations.mail.graph.client_secret"][0],
        RE_ENTER
    );
}

#[sqlx::test]
async fn smtp_password_keeps_its_spaces(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, json!({ "smtp": { "password": " pa ss " } })).await;
    assert_eq!(opened(&api, "smtp.password").await, " pa ss ");
    saved(&api, json!({ "smtp": null })).await;
    assert!(row(&api, "smtp.password").await.is_none());
}

// --- errors --------------------------------------------------------------------------------

#[sqlx::test]
async fn dropped_providers_stay_refused(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    for path in [
        "integrations.fulfillment.fulfillment_providers.sendcloud.public",
        "integrations.fulfillment.fulfillment_providers.sendcloud.secret",
        "integrations.accounting.accounting_providers.jortt.client_secret",
        "integrations.export.export_providers.modjo.api_key",
        "integrations.syncProviders.picqer.api_key",
    ] {
        let (status, res) = post(&api, nested(path, json!("s3cret"))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert_eq!(res["errors"][path][0], "This integration isn't supported.");
        // The mask is a value too; null and blank are fine.
        let (status, _) = post(&api, nested(path, json!(MASK))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        saved(&api, nested(path, Value::Null)).await;
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_credentials")
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn bad_values_are_refused(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    let cases = [
        (
            MOLLIE,
            json!("x".repeat(256)),
            "The integrations.payment.payment providers.mollie.api key must not be greater than 255 characters.",
        ),
        (
            MOLLIE,
            json!(12),
            "The integrations.payment.payment providers.mollie.api key must be a string.",
        ),
        (
            "smtp.password",
            json!("x".repeat(256)),
            "The smtp.password must not be greater than 255 characters.",
        ),
        (
            "integrations.mail.graph.client_secret",
            json!("x".repeat(1001)),
            "The integrations.mail.graph.client secret must not be greater than 1000 characters.",
        ),
    ];
    for (path, v, msg) in cases {
        let (status, res) = post(&api, body(path, v)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert_eq!(res["errors"][path][0], msg, "{res}");
        assert!(row(&api, path).await.is_none());
    }
    saved(&api, body(MOLLIE, json!("x".repeat(255)))).await;
    saved(
        &api,
        body(
            "integrations.mail.graph.client_secret",
            json!("é".repeat(1000)),
        ),
    )
    .await;
    assert_eq!(
        opened(&api, "integrations.mail.graph.client_secret").await,
        "é".repeat(1000)
    );
}

/// No `CREDENTIALS_KEY`: storing one is a 422 on its key and nothing is written; clearing
/// still works, and a workspace without credentials is unaffected.
#[sqlx::test]
async fn without_a_key_credentials_cant_be_set(pool: PgPool) {
    let mut state = common::state(pool.clone(), Mailer::capture().0);
    state.credentials = None;
    let app = build_app(state);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let uri = format!("/v1/workspace/{ws}/settings");
    let call = |body: Value| {
        let (app, uri, owner) = (app.clone(), uri.clone(), owner.clone());
        async move { common::send(&app, Method::POST, &uri, Some(body), Some(&owner)).await }
    };
    let (status, res) = call(json!({ "smtp": { "host": "h", "password": "p" } })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res,
        json!({ "message": "Credential storage isn't configured.",
                "errors": { "smtp.password": ["Credential storage isn't configured."] } })
    );
    let host: Option<Value> =
        sqlx::query_scalar("SELECT data -> 'smtp' FROM settings WHERE workspace_id = $1::uuid")
            .bind(&ws)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(host, None, "nothing of the refused save was written");
    assert_eq!(
        call(json!({ "smtp": { "host": "h", "password": null } }))
            .await
            .0,
        StatusCode::OK
    );
}

// --- tenancy -------------------------------------------------------------------------------

#[sqlx::test]
async fn credentials_stay_in_their_workspace(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, body(MOLLIE, json!("test_ada"))).await;
    let (bob, bob_ws) = signup(&api.app, "bob@example.com", "bobco").await;
    let bob_get = format!("/v1/workspace/{bob_ws}/settings");
    let (_, res) = common::send(&api.app, Method::GET, &bob_get, None, Some(&bob)).await;
    assert_eq!(at(&res["data"], MOLLIE), &Value::Null, "Bob has none");
    // Bob can't reach Ada's workspace.
    let ada_uri = format!("/v1/workspace/{}/settings", api.ws);
    for (method, body) in [
        (Method::GET, None),
        (Method::POST, Some(body(MOLLIE, json!("x")))),
    ] {
        let (status, _) = common::send(&api.app, method, &ada_uri, body, Some(&bob)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    assert_eq!(opened(&api, MOLLIE).await, "test_ada");
    // A sealed value copied into Bob's workspace doesn't open there.
    let (_, sealed) = row(&api, MOLLIE).await.unwrap();
    let bob_ws: Uuid = bob_ws.parse().unwrap();
    assert!(key().open(bob_ws, MOLLIE, &sealed).is_err());
}

/// A workspace delete takes its credentials with it.
#[sqlx::test]
async fn rows_go_with_their_workspace(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, body(MOLLIE, json!("test_ada"))).await;
    sqlx::query("DELETE FROM workspaces WHERE id = $1::uuid")
        .bind(&api.ws)
        .execute(&api.pool)
        .await
        .unwrap();
    assert!(row(&api, MOLLIE).await.is_none());
}
