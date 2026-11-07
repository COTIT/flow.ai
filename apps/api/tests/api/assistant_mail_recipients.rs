//! Direct assistant mail freezes approved contact fallback recipients before transport.

use crate::common::{self, Api, fake_smtp, send, send_raw};
use axum::http::{Method, StatusCode};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const BRIDGE: &str = "assistant-mail-recipient-test-secret-at-least-thirty-two-bytes";

async fn setup(pool: PgPool) -> (Api, String, String, fake_smtp::Inbox) {
    let (port, inbox) = fake_smtp::start().await;
    let mut api = Api::new(pool.clone(), "/send-message").await;
    let mailer = genesy_360_api::mail::Mailer::capture()
        .0
        .with_trusted_smtp_hosts(&["localhost".to_owned()])
        .with_timeout(std::time::Duration::from_secs(2));
    let mut state = common::state(pool, mailer);
    state.assistant_bridge_secret = Some(std::sync::Arc::new(
        genesy_360_api::AssistantSecret::new(BRIDGE.to_owned()).unwrap(),
    ));
    api.app = genesy_360_api::build_app(state);
    let relation = api
        .with_base("/relations")
        .create(
            json!({"company_name":"Reviewed recipient","type":"business"}),
            "relation_id",
        )
        .await;
    let (status, response) = api
        .with_base("/relations")
        .call(
            Method::POST,
            &format!("/{relation}/contacts"),
            Some(json!({"first_name":"Alice","last_name":"Contact","email":"alice@example.test"})),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
    let contact = response["data"]["contact_id"].as_str().unwrap().to_owned();
    let profile = api.with_base("/email-profiles").create(json!({"name":"Local SMTP","default":true,"sender_name":"Office","email":"office@example.test","smtp":{"host":"localhost","port":port}}), "email_profile_id").await;
    (api, contact, profile, inbox)
}

fn message(contact: &str, profile: &str) -> Value {
    json!({"addressed_contact_id":contact,"email_profile_id":profile,"address":{"address":"Main","house_number":"1","zipcode":"1000AA","city":"Amsterdam","country":"NL"},"to":[],"subject":"Approved message","body":"<p>Approved content</p>","send_method":"email"})
}

async fn review(api: &Api, token: &str, body: Value) -> (String, String, Value) {
    let base = format!("/v1/workspace/{}/assistant", api.ws);
    let (status, response) = send(
        &api.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
    let session = response["data"]["id"].as_str().unwrap().to_owned();
    let (status, proposal) = send(&api.app, Method::POST, &format!("{base}/actions"), Some(json!({"session_id":session,"operation_id":"messages_send","method":"POST","path":format!("/v1/workspace/{}/send-message",api.ws),"body":body})), Some(token)).await;
    assert_eq!(status, StatusCode::CREATED, "{proposal}");
    let action = proposal["data"]["id"].as_str().unwrap().to_owned();
    let uri = format!("{base}/actions/{action}/approve");
    let approval = json!({"session_id":session});
    let stamp = chrono::Utc::now().timestamp().to_string();
    let canonical = format!(
        "{stamp}\nPOST\n{uri}\n{}",
        hex::encode(Sha256::digest(approval.to_string().as_bytes()))
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(BRIDGE.as_bytes()).unwrap();
    mac.update(canonical.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let (status, response) = send_raw(
        &api.app,
        Method::POST,
        &uri,
        Some(approval),
        Some(token),
        &[
            ("Assistant-Bridge-Timestamp", &stamp),
            ("Assistant-Bridge-Signature", &signature),
        ],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&response)
    );
    (session, action, proposal)
}

async fn dispatch(api: &Api, token: &str, body: Value, session: &str, action: &str) -> StatusCode {
    send_raw(
        &api.app,
        Method::POST,
        &format!("/v1/workspace/{}/send-message", api.ws),
        Some(body),
        Some(token),
        &[
            ("Assistant-Session-Id", session),
            ("Assistant-Action-Id", action),
        ],
    )
    .await
    .0
}

#[sqlx::test]
async fn changed_contact_refuses_smtp_and_fresh_review_sends_to_new_address(pool: PgPool) {
    let (api, contact, profile, inbox) = setup(pool.clone()).await;
    let body = message(&contact, &profile);
    let (session, action, _) = review(&api, &api.owner, body.clone()).await;
    sqlx::query(
        "UPDATE contacts SET email='bob@example.test' WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&contact)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        dispatch(&api, &api.owner, body.clone(), &session, &action).await,
        StatusCode::CONFLICT
    );
    assert!(
        inbox.lock().unwrap().is_empty(),
        "stale approval must not reach SMTP"
    );
    let logs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM email_send_logs WHERE workspace_id=$1::uuid")
            .bind(&api.ws)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(logs, 0, "stale approval must fail before mail admission");
    let (session, action, proposal) = review(&api, &api.owner, body.clone()).await;
    assert_eq!(
        proposal["data"]["review_facts"]["facts"]["locked"]["to"],
        json!(["bob@example.test"])
    );
    assert_eq!(
        dispatch(&api, &api.owner, body, &session, &action).await,
        StatusCode::OK
    );
    let mail = inbox.lock().unwrap();
    assert_eq!(mail.len(), 1);
    assert_eq!(mail[0].rcpt, vec!["<bob@example.test>"]);
}

#[sqlx::test]
async fn native_mail_keeps_live_fallback_and_explicit_assistant_to_stays_frozen(pool: PgPool) {
    let (api, contact, profile, inbox) = setup(pool.clone()).await;
    let mut explicit = message(&contact, &profile);
    explicit["to"] = json!(["explicit@example.test"]);
    let (session, action, _) = review(&api, &api.owner, explicit.clone()).await;
    sqlx::query(
        "UPDATE contacts SET email='bob@example.test' WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&contact)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        dispatch(&api, &api.owner, explicit, &session, &action).await,
        StatusCode::OK
    );
    let (status, response) = api
        .call(
            Method::POST,
            "",
            Some(message(&contact, &profile)),
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let mail = inbox.lock().unwrap();
    assert_eq!(mail.len(), 2);
    assert_eq!(mail[0].rcpt, vec!["<explicit@example.test>"]);
    assert_eq!(mail[1].rcpt, vec!["<bob@example.test>"]);
}

#[sqlx::test]
async fn sender_without_contact_view_gets_hidden_facts_but_private_snapshot_still_blocks_change(
    pool: PgPool,
) {
    let (api, contact, profile, inbox) = setup(pool.clone()).await;
    let sender = common::member_with(
        &api.app,
        &pool,
        &api.ws,
        "sender@example.test",
        &["workspace.send-message"],
    )
    .await;
    let body = message(&contact, &profile);
    let (session, action, proposal) = review(&api, &sender, body.clone()).await;
    assert_eq!(
        proposal["data"]["review_facts"],
        json!({"factsHiddenByPermission":true})
    );
    assert!(!proposal.to_string().contains("alice@example.test"));
    sqlx::query(
        "UPDATE contacts SET email='bob@example.test' WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&api.ws)
    .bind(&contact)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        dispatch(&api, &sender, body, &session, &action).await,
        StatusCode::CONFLICT
    );
    assert!(inbox.lock().unwrap().is_empty());
}

#[sqlx::test]
async fn write_permission_is_checked_before_resolving_any_contact(pool: PgPool) {
    let (api, contact, profile, inbox) = setup(pool.clone()).await;
    let denied = common::member_with(&api.app, &pool, &api.ws, "denied@example.test", &[]).await;
    let base = format!("/v1/workspace/{}/assistant", api.ws);
    let (status, session) = send(
        &api.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(&denied),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    for target in [contact, uuid::Uuid::now_v7().to_string()] {
        let (status,response)=send(&api.app,Method::POST,&format!("{base}/actions"),Some(json!({"session_id":session["data"]["id"],"operation_id":"messages_send","method":"POST","path":format!("/v1/workspace/{}/send-message",api.ws),"body":message(&target,&profile)})),Some(&denied)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{response}");
        assert!(!response.to_string().contains("alice@example.test"));
    }
    assert!(inbox.lock().unwrap().is_empty());
}
