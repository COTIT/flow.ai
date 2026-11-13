//! Durable dialogue ownership, canonical replacement and cleanup isolation.

use super::assistant::{signed, signed_app};
use crate::common::{self, send, send_raw, signup};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

async fn create(app: &Router, ws: &str, token: &str) -> String {
    let (status, body) = send(
        app,
        Method::POST,
        &format!("/v1/workspace/{ws}/assistant/conversations"),
        Some(json!({"title":"Harvest"})),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"]["id"].as_str().unwrap().to_owned()
}

async fn source(
    app: &Router,
    ws: &str,
    token: &str,
    cid: &str,
    provider: &str,
) -> (String, String) {
    let base = format!("/v1/workspace/{ws}/assistant");
    let (_, session) = send(
        app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(token),
    )
    .await;
    let sid = session["data"]["id"].as_str().unwrap();
    assert_eq!(
        signed(
            app,
            &format!("{base}/sessions/{sid}/attach"),
            json!({"conversation_id":provider}),
            token
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status,body)=signed(app,&format!("{base}/conversations/{cid}/sources"),json!({"session_id":sid,"provider_conversation_id":provider,"agent_id":"agent_test","text_only":false}),token).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    (
        sid.to_owned(),
        body["data"]["id"].as_str().unwrap().to_owned(),
    )
}

async fn events(
    app: &Router,
    ws: &str,
    token: &str,
    cid: &str,
    sid: &str,
    source: &str,
    entries: Value,
) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        &format!("/v1/workspace/{ws}/assistant/conversations/{cid}/events"),
        Some(json!({"session_id":sid,"source_id":source,"events":entries})),
        Some(token),
    )
    .await
}

#[sqlx::test]
async fn history_survives_new_login_but_stays_private(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let uri = format!("/v1/workspace/{ws}/assistant/conversations/{cid}");
    let login = send(
        &app,
        Method::POST,
        "/v1/login",
        Some(json!({"email":"ada@example.com","password":common::PASSWORD})),
        None,
    )
    .await;
    let second = login.1["data"]["access_token"].as_str().unwrap();
    assert_eq!(
        send(&app, Method::GET, &uri, None, Some(second)).await.0,
        StatusCode::OK
    );
    let bob = common::member_with(&app, &pool, &ws, "bob@example.com", &[]).await;
    assert_eq!(
        send(&app, Method::GET, &uri, None, Some(&bob)).await.0,
        StatusCode::NOT_FOUND
    );
    let (eve, other) = signup(&app, "eve@example.com", "other").await;
    assert_eq!(
        send(
            &app,
            Method::GET,
            &uri.replace(&ws, &other),
            None,
            Some(&eve)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::GET, &uri, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn final_replaces_complete_provisional_preserves_pending_and_rejects_stale_cursor(
    pool: PgPool,
) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, src) = source(&app, &ws, &token, &cid, "conv_test").await;
    let input = json!([{"key":"p1","ordinal":0,"role":"user","text":"Harvest maize","modality":"voice","status":"complete"},{"key":"queued","ordinal":1,"role":"user","text":"Never guess delivery","modality":"text","status":"queued"}]);
    assert_eq!(
        events(&app, &ws, &token, &cid, &sid, &src, input.clone())
            .await
            .0,
        StatusCode::OK
    );
    let uri = format!("/v1/workspace/{ws}/assistant/conversations/{cid}/messages");
    let (_, page) = send(&app, Method::GET, &uri, None, Some(&token)).await;
    let rev = page["data"]["revision"].as_i64().unwrap();
    assert_eq!(
        events(&app, &ws, &token, &cid, &sid, &src, input).await.0,
        StatusCode::OK
    );
    let (_, same) = send(&app, Method::GET, &uri, None, Some(&token)).await;
    assert_eq!(same["data"]["revision"], rev);
    let final_body = json!({"provider_conversation_id":"conv_test","agent_id":"agent_test","turns":[{"key":"f1","role":"user","text":"Harvest maize corrected","modality":"voice","interrupted":false}],"summary":"Harvest"});
    assert_eq!(
        signed(
            &app,
            "/v1/assistant/conversations/ingest",
            final_body.clone(),
            ""
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, page) = send(&app, Method::GET, &uri, None, Some(&token)).await;
    assert_eq!(page["data"]["messages"].as_array().unwrap().len(), 2);
    assert!(
        page["data"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["text"] == "Harvest maize corrected")
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("{uri}?revision={rev}"),
            None,
            Some(&token)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let final_rev = page["data"]["revision"].clone();
    assert_eq!(
        signed(&app, "/v1/assistant/conversations/ingest", final_body, "")
            .await
            .0,
        StatusCode::OK
    );
    let (_, page) = send(&app, Method::GET, &uri, None, Some(&token)).await;
    assert_eq!(page["data"]["revision"], final_rev);
}

#[sqlx::test]
async fn ended_allows_original_flush_deleted_never_resurrects(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, src) = source(&app, &ws, &token, &cid, "conv_delete").await;
    let uri = format!("/v1/workspace/{ws}/assistant/conversations/{cid}");
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("{uri}/end"),
            Some(json!({})),
            Some(&token)
        )
        .await
        .0,
        StatusCode::OK
    );
    let entry = json!([{"key":"p1","ordinal":0,"role":"user","text":"Saved flush","modality":"text","status":"complete"}]);
    assert_eq!(
        events(&app, &ws, &token, &cid, &sid, &src, entry.clone())
            .await
            .0,
        StatusCode::OK
    );
    let login = send(
        &app,
        Method::POST,
        "/v1/login",
        Some(json!({"email":"ada@example.com","password":common::PASSWORD})),
        None,
    )
    .await;
    assert_eq!(
        events(
            &app,
            &ws,
            login.1["data"]["access_token"].as_str().unwrap(),
            &cid,
            &sid,
            &src,
            entry
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::DELETE, &uri, None, Some(&token)).await.0,
        StatusCode::OK
    );
    let body = json!({"provider_conversation_id":"conv_delete","agent_id":"agent_test","turns":[{"key":"f1","role":"user","text":"Cannot revive","modality":"text","interrupted":false}]});
    assert_eq!(
        signed(&app, "/v1/assistant/conversations/ingest", body, "")
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(&app, Method::GET, &uri, None, Some(&token)).await.0,
        StatusCode::NOT_FOUND
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM assistant_conversation_turns WHERE conversation_id=$1",
    )
    .bind(uuid::Uuid::parse_str(&cid).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn source_binding_signed_immutable_and_matching_execution_only(pool: PgPool) {
    let app = signed_app(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, src) = source(&app, &ws, &token, &cid, "conv_binding").await;
    let path = format!("/v1/workspace/{ws}/assistant/conversations/{cid}/sources");
    let body = json!({"session_id":sid,"provider_conversation_id":"conv_forged","agent_id":"agent_test","text_only":false});
    assert_eq!(
        send(&app, Method::POST, &path, Some(body.clone()), Some(&token))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        signed(&app, &path, body, &token).await.0,
        StatusCode::CONFLICT
    );
    let retry = json!({"session_id":sid,"provider_conversation_id":"conv_binding","agent_id":"agent_test","text_only":false});
    let (status, same) = signed(&app, &path, retry, &token).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(same["data"]["id"], src);
    assert_eq!(send(&app,Method::POST,"/v1/assistant/conversations/ingest",Some(json!({"provider_conversation_id":"conv_binding","agent_id":"agent_test","turns":[]})),None).await.0,StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn pages_bounded_search_literal_tail_excludes_pending(pool: PgPool) {
    let app = signed_app(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, src) = source(&app, &ws, &token, &cid, "conv_pages").await;
    let entries:Vec<_>=(0..10).map(|i|json!({"key":format!("k{i}"),"ordinal":i,"role":"user","text":format!("message {i} 100%"),"modality":"text","status":if i==9{"queued"}else{"complete"}})).collect();
    assert_eq!(
        events(&app, &ws, &token, &cid, &sid, &src, json!(entries))
            .await
            .0,
        StatusCode::OK
    );
    let uri = format!("/v1/workspace/{ws}/assistant/conversations/{cid}/messages");
    let (_, first) = send(
        &app,
        Method::GET,
        &format!("{uri}?limit=2"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(first["data"]["messages"].as_array().unwrap().len(), 2);
    assert_eq!(first["data"]["has_more"], true);
    let cursor = first["data"]["cursor"].as_str().unwrap();
    let revision = first["data"]["revision"].as_i64().unwrap();
    let (_, second) = send(
        &app,
        Method::GET,
        &format!("{uri}?limit=2&revision={revision}&cursor={cursor}"),
        None,
        Some(&token),
    )
    .await;
    assert_ne!(
        first["data"]["messages"][0]["key"],
        second["data"]["messages"][0]["key"]
    );
    let (_, tail) = send(
        &app,
        Method::GET,
        &format!("{uri}?tail=true&limit=2"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(tail["data"]["messages"][0]["key"], "k7");
    assert_eq!(tail["data"]["messages"][1]["key"], "k8");
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("{uri}?limit=101"),
            None,
            Some(&token)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("{uri}?cursor=garbage"),
            None,
            Some(&token)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let huge = json!([{"key":"huge","ordinal":11,"role":"user","text":"x".repeat(16385),"modality":"text","status":"complete"}]);
    assert_eq!(
        events(&app, &ws, &token, &cid, &sid, &src, huge).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test]
async fn cleanup_grace_preserves_sources_and_legacy_sessions_stay_unchanged(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, _) = source(&app, &ws, &token, &cid, "conv_cleanup").await;
    let base = format!("/v1/workspace/{ws}/assistant");
    send(
        &app,
        Method::DELETE,
        &format!("{base}/sessions/{sid}"),
        None,
        Some(&token),
    )
    .await;
    let due = cleanup_due(&app).await;
    assert!(!due.iter().any(|v| v["session_id"] == sid));
    sqlx::query("UPDATE assistant_sessions SET closed_at=now()-interval '61 minutes',last_seen_at=now()-interval '61 minutes' WHERE workspace_id=$1 AND id=$2").bind(uuid::Uuid::parse_str(&ws).unwrap()).bind(uuid::Uuid::parse_str(&sid).unwrap()).execute(&pool).await.unwrap();
    let due = cleanup_due(&app).await;
    assert!(due.iter().any(|v| v["session_id"] == sid));
    let (_, legacy) = send(
        &app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(&token),
    )
    .await;
    let legacy = legacy["data"]["id"].as_str().unwrap();
    send(
        &app,
        Method::DELETE,
        &format!("{base}/sessions/{legacy}"),
        None,
        Some(&token),
    )
    .await;
    let due = cleanup_due(&app).await;
    assert!(due.iter().any(|v| v["session_id"] == legacy));
}

async fn cleanup_due(app: &Router) -> Vec<Value> {
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let path = "/v1/assistant/cleanup";
    let message = format!(
        "{timestamp}\nGET\n{path}\n{}",
        hex::encode(Sha256::digest(b""))
    );
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(super::assistant::BRIDGE.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let (status, body) = send_raw(
        app,
        Method::GET,
        path,
        None,
        None,
        &[
            ("Assistant-Bridge-Timestamp", &timestamp),
            ("Assistant-Bridge-Signature", &signature),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    serde_json::from_slice::<Value>(&body).unwrap()["data"]
        .as_array()
        .unwrap()
        .clone()
}

#[sqlx::test]
async fn authoritative_redaction_and_unknown_payload_fields_never_archive_controls(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, src) = source(&app, &ws, &token, &cid, "conv_redact").await;
    let entry = json!([{"key":"r1","ordinal":0,"role":"user","text":"Confirm action 123456 Bearer ABC123 sk-abcdefghijklmnop","modality":"text","status":"complete"}]);
    assert_eq!(
        events(&app, &ws, &token, &cid, &sid, &src, entry).await.0,
        StatusCode::OK
    );
    let uri = format!("/v1/workspace/{ws}/assistant/conversations/{cid}/messages");
    let (_, page) = send(&app, Method::GET, &uri, None, Some(&token)).await;
    let text = page["data"]["messages"][0]["text"].as_str().unwrap();
    assert_eq!(
        text,
        "[approval redacted] [credential redacted] [credential redacted]"
    );
    let body = json!({"session_id":sid,"source_id":src,"events":[{"key":"r2","ordinal":1,"role":"agent","text":"safe","modality":"text","status":"complete","tool_calls":[{"token":"never-store"}]}]});
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/v1/workspace/{ws}/assistant/conversations/{cid}/events"),
            Some(body),
            Some(&token)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let snapshot = json!({"provider_conversation_id":"conv_redact","agent_id":"agent_test","turns":[{"key":"final","role":"agent","text":"Confirm batch 654321","modality":"text"}],"summary":"Bearer xyz"});
    assert_eq!(
        signed(&app, "/v1/assistant/conversations/ingest", snapshot, "")
            .await
            .0,
        StatusCode::OK
    );
    let (_, page) = send(&app, Method::GET, &uri, None, Some(&token)).await;
    assert_eq!(page["data"]["messages"][0]["role"], "agent");
    assert!(!page.to_string().contains("654321"));
}

#[sqlx::test]
async fn inactive_retention_tombstones_content_and_preserves_provider_cleanup_identity(
    pool: PgPool,
) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, src) = source(&app, &ws, &token, &cid, "conv_expire").await;
    assert_eq!(events(&app,&ws,&token,&cid,&sid,&src,json!([{"key":"k1","ordinal":0,"role":"user","text":"Expire body","modality":"text","status":"complete"}])).await.0,StatusCode::OK);
    sqlx::query("UPDATE assistant_conversations SET expires_at=now()-interval '1 second',last_activity_at=now()-interval '90 days' WHERE workspace_id=$1 AND id=$2").bind(uuid::Uuid::parse_str(&ws).unwrap()).bind(uuid::Uuid::parse_str(&cid).unwrap()).execute(&pool).await.unwrap();
    genesy_360_api::cleanup::prune(&pool).await.unwrap();
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM assistant_conversation_turns WHERE workspace_id=$1 AND conversation_id=$2").bind(uuid::Uuid::parse_str(&ws).unwrap()).bind(uuid::Uuid::parse_str(&cid).unwrap()).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
    let provider:String=sqlx::query_scalar("SELECT provider_conversation_id FROM assistant_conversation_sources WHERE workspace_id=$1 AND conversation_id=$2").bind(uuid::Uuid::parse_str(&ws).unwrap()).bind(uuid::Uuid::parse_str(&cid).unwrap()).fetch_one(&pool).await.unwrap();
    assert_eq!(provider, "conv_expire");
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/v1/workspace/{ws}/assistant/conversations/{cid}"),
            None,
            Some(&token)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn source_directory_and_current_login_lookup_are_paged_and_private(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, _) = source(&app, &ws, &token, &cid, "conv_first").await;
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/v1/workspace/{ws}/assistant/sessions/{sid}"),
            None,
            Some(&token)
        )
        .await
        .0,
        StatusCode::OK
    );
    source(&app, &ws, &token, &cid, "conv_second").await;
    let uri = format!("/v1/workspace/{ws}/assistant/conversations/{cid}");
    let (_, page) = send(
        &app,
        Method::GET,
        &format!("{uri}?source_limit=1"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(page["data"]["conversation"]["id"], cid);
    assert_eq!(page["data"]["sources"].as_array().unwrap().len(), 1);
    assert_eq!(page["data"]["sources_has_more"], true);
    let cursor = page["data"]["next_source_cursor"].as_i64().unwrap();
    let (_, older) = send(
        &app,
        Method::GET,
        &format!("{uri}?source_before={cursor}&source_limit=1"),
        None,
        Some(&token),
    )
    .await;
    assert_eq!(older["data"]["sources"][0]["ordinal"], 1);
    assert!(!page.to_string().contains("provider_conversation_id"));
    assert!(!page.to_string().contains("session_id"));
    let list = format!("/v1/workspace/{ws}/assistant/conversations?session_id={sid}");
    let (_, mine) = send(&app, Method::GET, &list, None, Some(&token)).await;
    assert_eq!(mine["data"]["conversations"].as_array().unwrap().len(), 1);
    let login = send(
        &app,
        Method::POST,
        "/v1/login",
        Some(json!({"email":"ada@example.com","password":common::PASSWORD})),
        None,
    )
    .await;
    let (_, other) = send(
        &app,
        Method::GET,
        &list,
        None,
        login.1["data"]["access_token"].as_str(),
    )
    .await;
    assert_eq!(other["data"]["conversations"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn final_ready_cleanup_can_finish_early_but_unfinalized_ack_waits_grace(pool: PgPool) {
    let app = signed_app(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, _) = source(&app, &ws, &token, &cid, "conv_early").await;
    send(
        &app,
        Method::POST,
        &format!("/v1/workspace/{ws}/assistant/conversations/{cid}/end"),
        None,
        Some(&token),
    )
    .await;
    let ack = format!("/v1/assistant/cleanup/{sid}/ack");
    assert_eq!(
        signed(&app, &ack, json!({}), "").await.0,
        StatusCode::FORBIDDEN
    );
    // Bodyless signed controls sign empty bytes, unlike JSON ingestion.
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let message = format!(
        "{timestamp}\nPOST\n{ack}\n{}",
        hex::encode(Sha256::digest(b""))
    );
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(super::assistant::BRIDGE.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let headers = [
        ("Assistant-Bridge-Timestamp", timestamp.as_str()),
        ("Assistant-Bridge-Signature", signature.as_str()),
    ];
    assert_eq!(
        send_raw(&app, Method::POST, &ack, None, None, &headers)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        signed(
            &app,
            "/v1/assistant/conversations/ingest",
            json!({"provider_conversation_id":"conv_early","agent_id":"agent_test","turns":[]}),
            ""
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(
        cleanup_due(&app)
            .await
            .iter()
            .any(|v| v["session_id"] == sid)
    );
    assert_eq!(
        send_raw(&app, Method::POST, &ack, None, None, &headers)
            .await
            .0,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn message_pages_bound_serialized_bytes_without_losing_remaining_turns(pool: PgPool) {
    let app = signed_app(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, src) = source(&app, &ws, &token, &cid, "conv_bytes").await;
    for start in [0, 3, 6] {
        let entries:Vec<_>=(start..start+3).map(|i|json!({"key":format!("k{i}"),"ordinal":i,"role":"user","text":"\\".repeat(16000),"modality":"text","status":"complete"})).collect();
        assert_eq!(
            events(&app, &ws, &token, &cid, &sid, &src, json!(entries))
                .await
                .0,
            StatusCode::OK
        );
    }
    let uri = format!("/v1/workspace/{ws}/assistant/conversations/{cid}/messages");
    let mut path = uri.clone();
    let mut keys = std::collections::HashSet::new();
    loop {
        let (status, page) = send(&app, Method::GET, &path, None, Some(&token)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(serde_json::to_vec(&page).unwrap().len() <= 65536);
        for row in page["data"]["messages"].as_array().unwrap() {
            assert!(keys.insert(row["key"].as_str().unwrap().to_owned()));
        }
        if page["data"]["has_more"] == false {
            break;
        }
        path = format!("{uri}?cursor={}", page["data"]["cursor"].as_str().unwrap());
    }
    assert_eq!(keys.len(), 9);
}

#[sqlx::test]
async fn source_directory_bounds_encoded_bytes_and_pages_every_full_summary(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    source(&app, &ws, &token, &cid, "conv_summaries").await;
    let workspace = uuid::Uuid::parse_str(&ws).unwrap();
    let conversation = uuid::Uuid::parse_str(&cid).unwrap();
    sqlx::query("UPDATE assistant_conversation_sources SET state='final',summary=repeat('x',16384) WHERE workspace_id=$1 AND conversation_id=$2").bind(workspace).bind(conversation).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO assistant_conversation_sources(workspace_id,conversation_id,session_id,login_id,provider_conversation_id,agent_id,text_only,ordinal,state,summary) SELECT s.workspace_id,s.conversation_id,uuidv7(),s.login_id,'conv_summaries_'||n,s.agent_id,s.text_only,n,'final',repeat('x',16384) FROM assistant_conversation_sources s CROSS JOIN generate_series(2,13) n WHERE s.workspace_id=$1 AND s.conversation_id=$2 AND s.ordinal=1").bind(workspace).bind(conversation).execute(&pool).await.unwrap();
    let uri = format!("/v1/workspace/{ws}/assistant/conversations/{cid}");
    let mut path = uri.clone();
    let mut seen = std::collections::HashSet::new();
    loop {
        let (status, page) = send(&app, Method::GET, &path, None, Some(&token)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(serde_json::to_vec(&page).unwrap().len() <= 65536);
        for source in page["data"]["sources"].as_array().unwrap() {
            assert_eq!(source["summary"].as_str().unwrap().len(), 16384);
            assert!(seen.insert(source["id"].as_str().unwrap().to_owned()));
        }
        if page["data"]["sources_has_more"] == false {
            break;
        }
        path = format!(
            "{uri}?source_before={}",
            page["data"]["next_source_cursor"].as_i64().unwrap()
        );
    }
    assert_eq!(seen.len(), 13);
}

#[sqlx::test]
async fn namespaced_provider_keys_allow_two_hundred_byte_native_ids(pool: PgPool) {
    let app = signed_app(pool);
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (sid, src) = source(&app, &ws, &token, &cid, "conv_keys").await;
    let key = format!("{src}:agent:{}", "a".repeat(200));
    assert_eq!(key.len(), 243);
    assert_eq!(events(&app,&ws,&token,&cid,&sid,&src,json!([{"key":key,"ordinal":0,"role":"agent","text":"Native id stays exact","modality":"voice","status":"complete"}])).await.0,StatusCode::OK);
    assert_eq!(events(&app,&ws,&token,&cid,&sid,&src,json!([{"key":"b".repeat(257),"ordinal":1,"role":"agent","text":"Refused bound","modality":"voice","status":"complete"}])).await.0,StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn competing_history_sources_across_logins_are_atomic_and_closed_source_allows_return(
    pool: PgPool,
) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let login = send(
        &app,
        Method::POST,
        "/v1/login",
        Some(json!({"email":"ada@example.com","password":common::PASSWORD})),
        None,
    )
    .await;
    let second = login.1["data"]["access_token"].as_str().unwrap();
    let first_cid = create(&app, &ws, &token).await;
    let second_cid = create(&app, &ws, second).await;
    let first_session = ready_session(&app, &ws, &token, "conv_race_first").await;
    let second_session = ready_session(&app, &ws, second, "conv_race_second").await;
    let first_path = format!("/v1/workspace/{ws}/assistant/conversations/{first_cid}/sources");
    let second_path = format!("/v1/workspace/{ws}/assistant/conversations/{second_cid}/sources");
    let first_body = json!({"session_id":first_session,"provider_conversation_id":"conv_race_first","agent_id":"agent_test","text_only":false});
    let second_body = json!({"session_id":second_session,"provider_conversation_id":"conv_race_second","agent_id":"agent_test","text_only":true});
    let (first_result, second_result) = tokio::join!(
        signed(&app, &first_path, first_body.clone(), &token),
        signed(&app, &second_path, second_body.clone(), second)
    );
    let statuses = [first_result.0, second_result.0];
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CREATED)
            .count(),
        1,
        "Exactly one concurrent source may become active: {statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1
    );
    let (
        winner_path,
        winner_body,
        winner_token,
        winner_session,
        winner_id,
        loser_path,
        loser_body,
        loser_token,
    ) = if first_result.0 == StatusCode::CREATED {
        (
            &first_path,
            &first_body,
            token.as_str(),
            first_session.as_str(),
            &first_result.1["data"]["id"],
            &second_path,
            &second_body,
            second,
        )
    } else {
        (
            &second_path,
            &second_body,
            second,
            second_session.as_str(),
            &second_result.1["data"]["id"],
            &first_path,
            &first_body,
            token.as_str(),
        )
    };
    let (status, retry) = signed(&app, winner_path, winner_body.clone(), winner_token).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(&retry["data"]["id"], winner_id);
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/v1/workspace/{ws}/assistant/sessions/{winner_session}"),
            None,
            Some(winner_token)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        signed(&app, loser_path, loser_body.clone(), loser_token)
            .await
            .0,
        StatusCode::CREATED
    );
    let bob = common::member_with(&app, &pool, &ws, "bob@example.com", &[]).await;
    let bob_cid = create(&app, &ws, &bob).await;
    source(&app, &ws, &bob, &bob_cid, "conv_race_bob").await;
}

async fn ready_session(app: &Router, ws: &str, token: &str, provider: &str) -> String {
    let base = format!("/v1/workspace/{ws}/assistant");
    let (status, body) = send(
        app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let sid = body["data"]["id"].as_str().unwrap();
    assert_eq!(
        signed(
            app,
            &format!("{base}/sessions/{sid}/attach"),
            json!({"conversation_id":provider}),
            token
        )
        .await
        .0,
        StatusCode::OK
    );
    sid.to_owned()
}

#[sqlx::test]
async fn expired_revoked_and_unlinked_legacy_sessions_do_not_block_fresh_history_source(
    pool: PgPool,
) {
    let app = signed_app(pool.clone());
    let (token, ws) = signup(&app, "ada@example.com", "acme").await;
    let cid = create(&app, &ws, &token).await;
    let (expired, _) = source(&app, &ws, &token, &cid, "conv_expired_guard").await;
    sqlx::query("UPDATE assistant_sessions SET expires_at=now()-interval '1 second' WHERE workspace_id=$1 AND id=$2").bind(uuid::Uuid::parse_str(&ws).unwrap()).bind(uuid::Uuid::parse_str(&expired).unwrap()).execute(&pool).await.unwrap();
    let cid2 = create(&app, &ws, &token).await;
    let (active, _) = source(&app, &ws, &token, &cid2, "conv_revoked_guard").await;
    let login = send(
        &app,
        Method::POST,
        "/v1/login",
        Some(json!({"email":"ada@example.com","password":common::PASSWORD})),
        None,
    )
    .await;
    let fresh = login.1["data"]["access_token"].as_str().unwrap();
    let cid3 = create(&app, &ws, fresh).await;
    let sid3 = ready_session(&app, &ws, fresh, "conv_fresh_guard").await;
    let path = format!("/v1/workspace/{ws}/assistant/conversations/{cid3}/sources");
    let body = json!({"session_id":sid3,"provider_conversation_id":"conv_fresh_guard","agent_id":"agent_test","text_only":true});
    assert_eq!(
        signed(&app, &path, body.clone(), fresh).await.0,
        StatusCode::CONFLICT
    );
    sqlx::query("DELETE FROM access_tokens t USING assistant_sessions a WHERE a.workspace_id=$1 AND a.id=$2 AND t.id=a.login_id").bind(uuid::Uuid::parse_str(&ws).unwrap()).bind(uuid::Uuid::parse_str(&active).unwrap()).execute(&pool).await.unwrap();
    assert_eq!(
        signed(&app, &path, body, fresh).await.0,
        StatusCode::CREATED
    );
    ready_session(&app, &ws, fresh, "conv_legacy_unlinked").await;
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/v1/workspace/{ws}/assistant/sessions/{sid3}"),
            None,
            Some(fresh)
        )
        .await
        .0,
        StatusCode::OK
    );
    let cid4 = create(&app, &ws, fresh).await;
    source(&app, &ws, fresh, &cid4, "conv_after_legacy").await;
}
