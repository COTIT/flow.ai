//! Account identity and credential rotation through the settings trust boundary.
use crate::common::{self, Api};
use axum::http::{Method, StatusCode};
use genesy_360_api::{CredentialKey, gateway::accounts, mail::Mailer};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn config(environment: &str, merchant: &str) -> Value {
    json!({"integrations":{"payment":{
        "active_provider":"globadyme","enabled":true,"environment":environment,
        "payment_providers":{"globadyme":{environment:{
            "merchant_id":merchant,"username":"login-original","password":"password-original",
            "callback_secret":"callback-original"
        }}}
    }}})
}

async fn post(api: &Api, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, "", Some(body), &api.owner).await
}

async fn saved(api: &Api, body: Value) {
    let (status, body) = post(api, body).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn current(api: &Api) -> Value {
    let (status, body) = api.call(Method::GET, "", None, &api.owner).await;
    assert_eq!(status, StatusCode::OK);
    json!({"integrations":body["data"]["integrations"]})
}

fn slot(body: &mut Value) -> &mut Value {
    &mut body["integrations"]["payment"]["payment_providers"]["globadyme"]["sandbox"]
}

async fn secret(api: &Api, field: &str) -> String {
    let path = accounts::path("sandbox", field);
    let (id, sealed): (String, Vec<u8>) = sqlx::query_as(
        "SELECT key_id,sealed FROM workspace_credentials WHERE workspace_id=$1::uuid AND path=$2",
    )
    .bind(&api.ws)
    .bind(&path)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    let key = CredentialKey::from_hex(common::CREDENTIALS_KEY).unwrap();
    assert_eq!(id, key.id());
    key.open(api.ws.parse().unwrap(), &path, &sealed)
        .unwrap()
        .expose()
        .to_owned()
}

async fn use_account(api: &Api) {
    let state = common::state(api.pool.clone(), Mailer::capture().0);
    let account = accounts::select(&state, api.ws.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let mut tx = api.pool.begin().await.unwrap();
    accounts::bind(&mut tx, api.ws.parse().unwrap(), &account)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test]
async fn used_account_identity_and_destructive_section_replacement_are_refused(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, config("sandbox", "merchant-original")).await;
    use_account(&api).await;
    let baseline = current(&api).await;
    for field in ["merchant_id", "username"] {
        let mut body = baseline.clone();
        slot(&mut body)[field] = json!("replacement");
        let (status, response) = post(&api, body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    }
    let (status, response) = post(&api, json!({"integrations":{"mail":{"driver":"smtp"}}})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(
        current(&api).await,
        baseline,
        "every rejected save rolls back its section merge"
    );
    assert_eq!(secret(&api, "username").await, "login-original");
    assert_eq!(secret(&api, "password").await, "password-original");
    let mut disabled = baseline.clone();
    disabled["integrations"]["payment"]["enabled"] = json!(false);
    saved(&api, disabled).await;
    let state = common::state(api.pool.clone(), Mailer::capture().0);
    assert!(
        accounts::select(&state, api.ws.parse().unwrap())
            .await
            .is_err()
    );
    assert!(
        accounts::bound(
            &state,
            api.ws.parse().unwrap(),
            "sandbox",
            Some("merchant-original")
        )
        .await
        .is_ok(),
        "historical work survives disabling new collection"
    );
}

#[sqlx::test]
async fn merchant_binding_is_unique_per_environment_across_workspaces(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, config("sandbox", "shared-merchant")).await;
    let (owner, ws) = common::signup(&api.app, "other-gateway@example.com", "other-gateway").await;
    let path = format!("/v1/workspace/{ws}/settings");
    let (status, body) = common::send(
        &api.app,
        Method::POST,
        &path,
        Some(config("sandbox", "shared-merchant")),
        Some(&owner),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = common::send(
        &api.app,
        Method::POST,
        &path,
        Some(config("production", "shared-merchant")),
        Some(&owner),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "same merchant may exist in another environment: {body}"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM payment_accounts WHERE merchant_id='shared-merchant'",
    )
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
}

#[sqlx::test]
async fn concurrent_callback_rotations_keep_exactly_one_previous_secret(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, config("sandbox", "merchant-rotate")).await;
    use_account(&api).await;
    let mut first = current(&api).await;
    let mut second = first.clone();
    slot(&mut first)["callback_secret"] = json!("callback-next-a");
    slot(&mut second)["callback_secret"] = json!("callback-next-b");
    let (first, second) = tokio::join!(post(&api, first), post(&api, second));
    let mut statuses = [first.0.as_u16(), second.0.as_u16()];
    statuses.sort_unstable();
    assert_eq!(
        statuses,
        [200, 422],
        "only one unresolved predecessor allowed"
    );
    assert_eq!(
        secret(&api, "previous_callback_secret").await,
        "callback-original"
    );
    assert!(
        ["callback-next-a", "callback-next-b"]
            .contains(&secret(&api, "callback_secret").await.as_str())
    );
    let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM payment_audit WHERE workspace_id=$1::uuid AND action='callback_rotated'").bind(&api.ws).fetch_one(&api.pool).await.unwrap();
    assert_eq!(audit, 1);
    let masked = current(&api).await;
    let json = masked.to_string();
    assert!(!json.contains("callback-next") && !json.contains("callback-original"));
    let mut clear = masked;
    slot(&mut clear)["previous_callback_secret"] = Value::Null;
    assert_eq!(post(&api, clear).await.0, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn password_rotation_invalidates_stale_collection_selection(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, config("sandbox", "merchant-revision")).await;
    let state = common::state(api.pool.clone(), Mailer::capture().0);
    let ws: Uuid = api.ws.parse().unwrap();
    let stale = accounts::select(&state, ws).await.unwrap().unwrap();
    let mut first = current(&api).await;
    let mut second = first.clone();
    slot(&mut first)["password"] = json!("password-next-a");
    slot(&mut second)["password"] = json!("password-next-b");
    let (first, second) = tokio::join!(post(&api, first), post(&api, second));
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(second.0, StatusCode::OK, "{}", second.1);
    let mut tx = api.pool.begin().await.unwrap();
    assert!(matches!(
        accounts::bind(&mut tx, ws, &stale).await,
        Err(genesy_360_api::AppError::Conflict(_))
    ));
    tx.rollback().await.unwrap();
    let fresh = accounts::select(&state, ws).await.unwrap().unwrap();
    assert_eq!(fresh.revision, stale.revision + 2);
    assert!(
        ["password-next-a", "password-next-b"].contains(&secret(&api, "password").await.as_str())
    );
    let mut tx = api.pool.begin().await.unwrap();
    accounts::bind(&mut tx, ws, &fresh).await.unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test]
async fn disabling_new_collection_invalidates_a_previously_selected_account(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, config("sandbox", "merchant-disable")).await;
    let state = common::state(api.pool.clone(), Mailer::capture().0);
    let ws = api.ws.parse().unwrap();
    let selected = accounts::select(&state, ws).await.unwrap().unwrap();
    let mut body = current(&api).await;
    body["integrations"]["payment"]["enabled"] = json!(false);
    saved(&api, body).await;
    let mut tx = api.pool.begin().await.unwrap();
    assert!(
        accounts::bind(&mut tx, ws, &selected).await.is_err(),
        "disable must fence new admission even when credentials did not rotate"
    );
    tx.rollback().await.unwrap();
    assert!(
        accounts::bound(&state, ws, "sandbox", Some("merchant-disable"))
            .await
            .is_ok(),
        "historical reconciliation remains available"
    );
}

#[sqlx::test]
async fn clearing_unused_credentials_invalidates_a_previously_selected_account(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, config("sandbox", "merchant-clear")).await;
    let state = common::state(api.pool.clone(), Mailer::capture().0);
    let ws = api.ws.parse().unwrap();
    let selected = accounts::select(&state, ws).await.unwrap().unwrap();
    let mut body = current(&api).await;
    slot(&mut body)["password"] = Value::Null;
    saved(&api, body).await;
    let mut tx = api.pool.begin().await.unwrap();
    assert!(
        accounts::bind(&mut tx, ws, &selected).await.is_err(),
        "cleared credentials must invalidate a preselected client"
    );
    tx.rollback().await.unwrap();
    assert!(accounts::select(&state, ws).await.is_err());
}

#[sqlx::test]
async fn callback_rotation_preserves_exact_secret_bytes(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    let mut body = config("sandbox", "merchant-exact-secrets");
    slot(&mut body)["password"] = json!(" password-original ");
    slot(&mut body)["callback_secret"] = json!(" callback-original ");
    saved(&api, body).await;
    assert_eq!(secret(&api, "password").await, " password-original ");
    assert_eq!(secret(&api, "callback_secret").await, " callback-original ");
    let mut body = current(&api).await;
    slot(&mut body)["callback_secret"] = json!(" callback-next ");
    saved(&api, body).await;
    assert_eq!(secret(&api, "callback_secret").await, " callback-next ");
    assert_eq!(
        secret(&api, "previous_callback_secret").await,
        " callback-original "
    );
}

#[sqlx::test]
async fn selected_slot_without_merchant_is_incomplete_even_if_metadata_remains(pool: PgPool) {
    let api = Api::new(pool, "/settings").await;
    saved(&api, config("sandbox", "merchant-unused-clear")).await;
    let mut body = current(&api).await;
    slot(&mut body)["merchant_id"] = Value::Null;
    saved(&api, body).await;
    let state = common::state(api.pool.clone(), Mailer::capture().0);
    assert!(
        accounts::select(&state, api.ws.parse().unwrap())
            .await
            .is_err(),
        "new collection must not select stale account metadata"
    );
}
