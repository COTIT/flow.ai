//! Assistant ownership, exact approvals, once-only dispatch and upload quotas.

use crate::common::{self, send, send_raw, signup};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

async fn session(app: &Router, ws: &str, token: &str) -> String {
    let (status, body) = send(
        app,
        Method::POST,
        &format!("/v1/workspace/{ws}/assistant/sessions"),
        Some(json!({"language":"en"})),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"]["id"].as_str().unwrap().to_owned()
}

#[sqlx::test]
async fn session_is_bound_to_workspace_actor_and_login(pool: PgPool) {
    let app = common::app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let id = session(&app, &ws, &owner).await;
    let uri = format!("/v1/workspace/{ws}/assistant/sessions/{id}");
    let (_, body) = send(&app, Method::GET, &uri, None, Some(&owner)).await;
    assert_eq!(body["data"]["state"], "active");
    let bob = common::member_with(&app, &pool, &ws, "bob@example.com", &[]).await;
    assert_eq!(
        send(&app, Method::GET, &uri, None, Some(&bob)).await.0,
        StatusCode::NOT_FOUND
    );
    let (eve, ews) = signup(&app, "eve@example.com", "eve").await;
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/v1/workspace/{ews}/assistant/sessions/{id}"),
            None,
            Some(&eve)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let login = send(
        &app,
        Method::POST,
        "/v1/login",
        Some(json!({"email":"ada@example.com","password":common::PASSWORD})),
        None,
    )
    .await;
    let other_login = login.1["data"]["access_token"].as_str().unwrap();
    assert_eq!(
        send(&app, Method::GET, &uri, None, Some(other_login))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn unsigned_approval_and_conversation_attachment_are_refused(pool: PgPool) {
    let app = common::app(pool);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let id = session(&app, &ws, &owner).await;
    let (status, _) = send_raw(
        &app,
        Method::POST,
        &format!("/v1/workspace/{ws}/assistant/sessions/{id}/attach"),
        Some(json!({"conversation_id":"conv_other"})),
        Some(&owner),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

pub(crate) const BRIDGE: &str = "assistant-test-bridge-secret-at-least-thirty-two-bytes";

pub(crate) fn signed_app(pool: PgPool) -> Router {
    let mut state = common::state(pool, genesy_360_api::mail::Mailer::capture().0);
    state.assistant_bridge_secret = Some(std::sync::Arc::new(
        genesy_360_api::AssistantSecret::new(BRIDGE.to_owned()).unwrap(),
    ));
    genesy_360_api::build_app(state)
}

pub(crate) async fn signed(
    app: &Router,
    uri: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let message = format!(
        "{timestamp}\nPOST\n{uri}\n{}",
        hex::encode(Sha256::digest(body.to_string().as_bytes()))
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(BRIDGE.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let (status, bytes) = send_raw(
        app,
        Method::POST,
        uri,
        Some(body),
        Some(token),
        &[
            ("Assistant-Bridge-Timestamp", &timestamp),
            ("Assistant-Bridge-Signature", &signature),
        ],
    )
    .await;
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[sqlx::test]
async fn approved_action_dispatches_once_and_binds_exact_request(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let base = format!("/v1/workspace/{ws}/assistant/actions");
    let target = format!("/v1/workspace/{ws}/users/teams");
    let fields = json!({"name":"Assistant team","description":"Assistant test"});
    let (status, proposed) = send(&app, Method::POST, &base, Some(json!({"session_id":sid,"operation_id":"teams_create","method":"POST","path":target,"body":fields})), Some(&owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{proposed}");
    let action = proposed["data"]["id"].as_str().unwrap();
    let headers = [
        ("Assistant-Action-Id", action),
        ("Assistant-Session-Id", sid.as_str()),
    ];
    assert_eq!(
        send_raw(
            &app,
            Method::POST,
            &target,
            Some(fields.clone()),
            Some(&owner),
            &headers
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        signed(
            &app,
            &format!("{base}/{action}/approve"),
            json!({"session_id":sid}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send_raw(
            &app,
            Method::POST,
            &target,
            Some(json!({"name":"Changed"})),
            Some(&owner),
            &headers
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (a, b) = tokio::join!(
        send_raw(
            &app,
            Method::POST,
            &target,
            Some(fields.clone()),
            Some(&owner),
            &headers
        ),
        send_raw(
            &app,
            Method::POST,
            &target,
            Some(fields.clone()),
            Some(&owner),
            &headers
        )
    );
    assert_eq!(
        [a.0, b.0]
            .iter()
            .filter(|s| **s == StatusCode::CREATED)
            .count(),
        1
    );
    assert_eq!(
        [a.0, b.0]
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM teams WHERE workspace_id=$1::uuid AND name='Assistant team'",
    )
    .bind(&ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let (_, outcome) = send(
        &app,
        Method::GET,
        &format!("{base}/{action}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(outcome["data"]["state"], "completed");
    assert!(outcome["data"].get("body").is_none());
}

async fn approved_team(
    app: &Router,
    ws: &str,
    owner: &str,
    sid: &str,
    name: &str,
) -> (String, String, Value) {
    let target = format!("/v1/workspace/{ws}/users/teams");
    let fields = json!({"name":name,"description":"Assistant test"});
    let base = format!("/v1/workspace/{ws}/assistant/actions");
    let (status,body)=send(app,Method::POST,&base,Some(json!({"session_id":sid,"operation_id":"teams_create","method":"POST","path":target,"body":fields})),Some(owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["data"]["id"].as_str().unwrap().to_owned();
    let (status, body) = signed(
        app,
        &format!("{base}/{id}/approve"),
        json!({"session_id":sid}),
        owner,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (id, target, fields)
}

#[sqlx::test]
async fn failure_after_business_commit_keeps_claim_consumed(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let (id, target, fields) = approved_team(&app, &ws, &owner, &sid, "Committed once").await;
    sqlx::raw_sql("CREATE FUNCTION fail_assistant_finalize() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.state='completed' THEN RAISE EXCEPTION 'injected ledger finalization failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_finalize BEFORE UPDATE ON assistant_actions FOR EACH ROW EXECUTE FUNCTION fail_assistant_finalize();").execute(&pool).await.unwrap();
    let headers = [
        ("Assistant-Action-Id", id.as_str()),
        ("Assistant-Session-Id", sid.as_str()),
    ];
    assert_eq!(
        send_raw(
            &app,
            Method::POST,
            &target,
            Some(fields.clone()),
            Some(&owner),
            &headers
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        send_raw(
            &app,
            Method::POST,
            &target,
            Some(fields),
            Some(&owner),
            &headers
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM teams WHERE workspace_id=$1::uuid AND name='Committed once'",
    )
    .bind(&ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    sqlx::query("UPDATE assistant_actions SET dispatched_at=now()-interval '11 minutes' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&ws).bind(&id).execute(&pool).await.unwrap();
    genesy_360_api::cleanup::prune(&pool).await.unwrap();
    let (_, body) = send(
        &app,
        Method::GET,
        &format!("/v1/workspace/{ws}/assistant/actions/{id}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(body["data"]["state"], "unknown");
    assert!(body["data"]["proposal"].is_null());
}

#[sqlx::test]
async fn drafts_restore_then_expire_without_retaining_business_arguments(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let (id, _, _) = approved_team(&app, &ws, &owner, &sid, "Private draft").await;
    let history = format!("/v1/workspace/{ws}/assistant/sessions/{sid}/actions");
    let (_, body) = send(&app, Method::GET, &history, None, Some(&owner)).await;
    assert_eq!(body["data"][0]["proposal"]["body"]["name"], "Private draft");
    sqlx::query("UPDATE assistant_actions SET expires_at=now()-interval '1 second' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&ws).bind(&id).execute(&pool).await.unwrap();
    let (_, body) = send(&app, Method::GET, &history, None, Some(&owner)).await;
    assert!(body["data"][0]["proposal"].is_null());
    assert_eq!(body["data"][0]["state"], "expired");
    genesy_360_api::cleanup::prune(&pool).await.unwrap();
    let remains:bool=sqlx::query_scalar("SELECT proposal IS NOT NULL FROM assistant_actions WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&ws).bind(&id).fetch_one(&pool).await.unwrap();
    assert!(!remains);
}

#[sqlx::test]
async fn signed_conversation_is_fixed_and_cancelled_upload_remains_cleanable(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let base = format!("/v1/workspace/{ws}/assistant/sessions/{sid}");
    for _ in 0..2 {
        assert_eq!(
            signed(
                &app,
                &format!("{base}/attach"),
                json!({"conversation_id":"conv_one"}),
                &owner
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert_eq!(
        signed(
            &app,
            &format!("{base}/attach"),
            json!({"conversation_id":"conv_other"}),
            &owner
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, body) = send(
        &app,
        Method::POST,
        &format!("{base}/uploads"),
        Some(json!({"media_type":"image/png","size_bytes":100})),
        Some(&owner),
    )
    .await;
    let upload = body["data"]["id"].as_str().unwrap();
    let path = format!("{base}/uploads/{upload}");
    send(&app, Method::DELETE, &path, None, Some(&owner)).await;
    let (status, body) = signed(
        &app,
        &format!("{path}/complete"),
        json!({"file_id":"file_one"}),
        &owner,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["state"], "cleanup_pending");
    send(&app, Method::DELETE, &base, None, Some(&owner)).await;
    let refs: Option<String> = sqlx::query_scalar(
        "SELECT file_id FROM assistant_uploads WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&ws)
    .bind(upload)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(refs.as_deref(), Some("file_one"));
}

#[sqlx::test]
async fn cancelled_action_cannot_dispatch_and_legacy_request_still_works(pool: PgPool) {
    let app = signed_app(pool);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let (id, target, fields) = approved_team(&app, &ws, &owner, &sid, "Cancelled").await;
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/v1/workspace/{ws}/assistant/actions/{id}/cancel"),
            Some(json!({"session_id":sid})),
            Some(&owner)
        )
        .await
        .0,
        StatusCode::OK
    );
    let headers = [
        ("Assistant-Action-Id", id.as_str()),
        ("Assistant-Session-Id", sid.as_str()),
    ];
    assert_eq!(
        send_raw(
            &app,
            Method::POST,
            &target,
            Some(fields.clone()),
            Some(&owner),
            &headers
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(&app, Method::POST, &target, Some(fields), Some(&owner))
            .await
            .0,
        StatusCode::CREATED
    );
}

#[sqlx::test]
async fn upload_reservations_enforce_two_inflight_and_ten_total(pool: PgPool) {
    let app = common::app(pool);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let id = session(&app, &ws, &owner).await;
    let uri = format!("/v1/workspace/{ws}/assistant/sessions/{id}/uploads");
    let input = json!({"media_type":"application/pdf","size_bytes":128});
    let (status, body) = send(&app, Method::POST, &uri, Some(input.clone()), Some(&owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let first = body["data"]["id"].as_str().unwrap();
    assert_eq!(
        send(&app, Method::POST, &uri, Some(input.clone()), Some(&owner))
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(
        send(&app, Method::POST, &uri, Some(input.clone()), Some(&owner))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("{uri}/{first}"),
            None,
            Some(&owner)
        )
        .await
        .0,
        StatusCode::OK
    );
    for _ in 0..8 {
        let (_, body) = send(&app, Method::POST, &uri, Some(input.clone()), Some(&owner)).await;
        let upload = body["data"]["id"].as_str().unwrap();
        send(
            &app,
            Method::DELETE,
            &format!("{uri}/{upload}"),
            None,
            Some(&owner),
        )
        .await;
    }
    assert_eq!(
        send(&app, Method::POST, &uri, Some(input), Some(&owner))
            .await
            .0,
        StatusCode::CONFLICT
    );
}

#[sqlx::test]
async fn multipart_proposal_requires_bridge_and_binds_raw_digest(pool: PgPool) {
    let app = signed_app(pool);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let proposal = format!("/v1/workspace/{ws}/assistant/actions");
    let input = json!({"session_id":sid,"operation_id":"files_create","method":"POST","path":format!("/v1/workspace/{ws}/files"),"body_sha256":hex::encode(Sha256::digest(b"raw-file-body")),"content_type":"multipart/form-data; boundary=assistant-test","body":{"files":[{"handle":"owned-file"}]}});
    let (status, body) = signed(&app, &proposal, input.clone(), &owner).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, _) = send_raw(
        &app,
        Method::POST,
        &proposal,
        Some(input),
        Some(&owner),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn owned_upload_metadata_restores_and_refuses_other_login(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let base = format!("/v1/workspace/{ws}/assistant/sessions/{sid}");
    signed(
        &app,
        &format!("{base}/attach"),
        json!({"conversation_id":"conv_metadata"}),
        &owner,
    )
    .await;
    let (_, body) = send(
        &app,
        Method::POST,
        &format!("{base}/uploads"),
        Some(json!({"media_type":"application/pdf","size_bytes":10485760})),
        Some(&owner),
    )
    .await;
    let id = body["data"]["id"].as_str().unwrap();
    let(status,body)=signed(&app,&format!("{base}/uploads/{id}/complete"),json!({"file_id":"file_metadata","size_bytes":128,"media_type":"application/pdf","sha256":"a".repeat(64)}),&owner).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = send(
        &app,
        Method::GET,
        &format!("{base}/uploads/{id}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["size_bytes"], 128);
    assert_eq!(body["data"]["sha256"], "a".repeat(64));
    let (_, body) = send(
        &app,
        Method::GET,
        &format!("{base}/uploads"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    let bob = common::member_with(&app, &pool, &ws, "bob@example.com", &[]).await;
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("{base}/uploads/{id}"),
            None,
            Some(&bob)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn effectful_get_preview_requires_and_consumes_approval(pool: PgPool) {
    let app = signed_app(pool);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let target =
        format!("/v1/workspace/{ws}/invoices/01900000-0000-7000-8000-000000000001/preview");
    let base = format!("/v1/workspace/{ws}/assistant/actions");
    let(status,body)=send(&app,Method::POST,&base,Some(json!({"session_id":sid,"operation_id":"invoices_preview","method":"GET","path":target})),Some(&owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["data"]["id"].as_str().unwrap();
    let headers = [
        ("Assistant-Action-Id", id),
        ("Assistant-Session-Id", sid.as_str()),
    ];
    assert_eq!(
        send_raw(&app, Method::GET, &target, None, Some(&owner), &headers)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        signed(
            &app,
            &format!("{base}/{id}/approve"),
            json!({"session_id":sid}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, _) = send_raw(&app, Method::GET, &target, None, Some(&owner), &headers).await;
    assert_ne!(status, StatusCode::CONFLICT);
    assert_eq!(
        send_raw(&app, Method::GET, &target, None, Some(&owner), &headers)
            .await
            .0,
        StatusCode::CONFLICT
    );
}

async fn dispatch_multipart(
    app: &Router,
    target: &str,
    owner: &str,
    sid: &str,
    id: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> StatusCode {
    use axum::{body::Body, extract::ConnectInfo, http::Request};
    use tower::ServiceExt;
    let mut req = Request::post(target)
        .header("Authorization", format!("Bearer {owner}"))
        .header("Assistant-Action-Id", id)
        .header("Assistant-Session-Id", sid)
        .header("Content-Type", content_type)
        .body(Body::from(bytes))
        .unwrap();
    req.extensions_mut().insert(ConnectInfo(
        common::PEER.parse::<std::net::SocketAddr>().unwrap(),
    ));
    app.clone().oneshot(req).await.unwrap().status()
}

#[sqlx::test]
async fn raw_multipart_dispatch_binds_file_bytes_and_boundary(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let content_type = "multipart/form-data; boundary=assistant-native-file";
    let bytes=b"--assistant-native-file\r\nContent-Disposition: form-data; name=\"public\"\r\n\r\n1\r\n--assistant-native-file\r\nContent-Disposition: form-data; name=\"upload\"; filename=\"invoice.pdf\"\r\nContent-Type: application/pdf\r\n\r\n%PDF-1.4\ncontent\r\n--assistant-native-file--\r\n".to_vec();
    let target = format!("/v1/workspace/{ws}/files");
    let base = format!("/v1/workspace/{ws}/assistant/actions");
    let(status,body)=signed(&app,&base,json!({"session_id":sid,"operation_id":"files_create","method":"POST","path":target,"body_sha256":hex::encode(Sha256::digest(&bytes)),"content_type":content_type}),&owner).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["data"]["id"].as_str().unwrap();
    assert_eq!(
        signed(
            &app,
            &format!("{base}/{id}/approve"),
            json!({"session_id":sid}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut changed = bytes.clone();
    changed.push(b'x');
    assert_eq!(
        dispatch_multipart(&app, &target, &owner, &sid, id, content_type, changed).await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        dispatch_multipart(
            &app,
            &target,
            &owner,
            &sid,
            id,
            "multipart/form-data; boundary=different",
            bytes.clone()
        )
        .await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        dispatch_multipart(&app, &target, &owner, &sid, id, content_type, bytes.clone()).await,
        StatusCode::CREATED
    );
    assert_eq!(
        dispatch_multipart(&app, &target, &owner, &sid, id, content_type, bytes).await,
        StatusCode::CONFLICT
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM files WHERE workspace_id=$1::uuid AND status='stored'",
    )
    .bind(&ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn handler_timeout_keeps_durable_claim_and_never_redispatches(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let target = format!("/v1/workspace/{ws}/relations");
    let base = format!("/v1/workspace/{ws}/assistant/actions");
    let fields = json!({"company_name":"Timeout relation","type":"business"});
    let(status,body)=send(&app,Method::POST,&base,Some(json!({"session_id":sid,"operation_id":"relations_create","method":"POST","path":target,"body":fields})),Some(&owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["data"]["id"].as_str().unwrap();
    assert_eq!(
        signed(
            &app,
            &format!("{base}/{id}/approve"),
            json!({"session_id":sid}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut state = common::state(pool.clone(), genesy_360_api::mail::Mailer::capture().0);
    state.request_timeout = std::time::Duration::from_millis(100);
    let short = genesy_360_api::build_app(state);
    let mut holder = pool.begin().await.unwrap();
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('relations.relation_number|' || $1,0))",
    )
    .bind(&ws)
    .execute(&mut *holder)
    .await
    .unwrap();
    let headers = [
        ("Assistant-Action-Id", id),
        ("Assistant-Session-Id", sid.as_str()),
    ];
    assert_eq!(
        send_raw(
            &short,
            Method::POST,
            &target,
            Some(fields.clone()),
            Some(&owner),
            &headers
        )
        .await
        .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    holder.rollback().await.unwrap();
    let (_, body) = send(
        &app,
        Method::GET,
        &format!("{base}/{id}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(body["data"]["state"], "unknown");
    assert_eq!(
        send_raw(
            &app,
            Method::POST,
            &target,
            Some(fields),
            Some(&owner),
            &headers
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[sqlx::test]
async fn spoken_approval_event_cannot_approve_two_actions_across_replicas(pool: PgPool) {
    let app = signed_app(pool.clone());
    let other = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let base = format!("/v1/workspace/{ws}/assistant/actions");
    let mut ids = Vec::new();
    for name in ["First spoken action", "Second spoken action"] {
        let(status,body)=send(&app,Method::POST,&base,Some(json!({"session_id":sid,"operation_id":"teams_create","method":"POST","path":format!("/v1/workspace/{ws}/users/teams"),"body":{"name":name,"description":"Spoken test"}})),Some(&owner)).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        ids.push(body["data"]["id"].as_str().unwrap().to_owned());
    }
    let proof = json!({"session_id":sid,"approval_event_sha256":"b".repeat(64)});
    let a = format!("{base}/{}/approve", ids[0]);
    let b = format!("{base}/{}/approve", ids[1]);
    let (one, two) = tokio::join!(
        signed(&app, &a, proof.clone(), &owner),
        signed(&other, &b, proof.clone(), &owner)
    );
    assert_eq!(
        [one.0, two.0]
            .iter()
            .filter(|s| **s == StatusCode::OK)
            .count(),
        1,
        "{one:?} {two:?}"
    );
    assert_eq!(
        [one.0, two.0]
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1,
        "{one:?} {two:?}"
    );
    let winner = if one.0 == StatusCode::OK { &a } else { &b };
    assert_eq!(
        signed(&other, winner, proof, &owner).await.0,
        StatusCode::OK
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM assistant_actions WHERE workspace_id=$1::uuid AND session_id=$2::uuid AND state='approved'").bind(&ws).bind(&sid).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn resolved_audit_expires_after_ninety_days_but_unknown_is_retained(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let (done, target, fields) = approved_team(&app, &ws, &owner, &sid, "Old completed").await;
    let headers = [
        ("Assistant-Action-Id", done.as_str()),
        ("Assistant-Session-Id", sid.as_str()),
    ];
    assert_eq!(
        send_raw(
            &app,
            Method::POST,
            &target,
            Some(fields),
            Some(&owner),
            &headers
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (unknown, _, _) = approved_team(&app, &ws, &owner, &sid, "Old unknown").await;
    sqlx::query("UPDATE assistant_actions SET finished_at=now()-interval '91 days' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&ws).bind(&done).execute(&pool).await.unwrap();
    sqlx::query("UPDATE assistant_actions SET state='unknown',proposal=NULL,dispatched_at=now()-interval '100 days' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&ws).bind(&unknown).execute(&pool).await.unwrap();
    genesy_360_api::cleanup::prune(&pool).await.unwrap();
    let ids:Vec<String>=sqlx::query_scalar("SELECT id::text FROM assistant_actions WHERE workspace_id=$1::uuid AND session_id=$2::uuid").bind(&ws).bind(&sid).fetch_all(&pool).await.unwrap();
    assert_eq!(ids, vec![unknown]);
}

async fn bridge_control(app: &Router, method: Method, uri: &str) -> (StatusCode, Value) {
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let message = format!(
        "{timestamp}\n{method}\n{uri}\n{}",
        hex::encode(Sha256::digest(b""))
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(BRIDGE.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let (status, bytes) = send_raw(
        app,
        method,
        uri,
        None,
        None,
        &[
            ("Assistant-Bridge-Timestamp", &timestamp),
            ("Assistant-Bridge-Signature", &signature),
        ],
    )
    .await;
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[sqlx::test]
async fn cleanup_waits_for_inflight_upload_and_never_revives_cleaned_session(pool: PgPool) {
    let app = signed_app(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let base = format!("/v1/workspace/{ws}/assistant/sessions/{sid}");
    signed(
        &app,
        &format!("{base}/attach"),
        json!({"conversation_id":"conv_cleanup"}),
        &owner,
    )
    .await;
    let (_, body) = send(
        &app,
        Method::POST,
        &format!("{base}/uploads"),
        Some(json!({"media_type":"application/pdf","size_bytes":128})),
        Some(&owner),
    )
    .await;
    let upload = body["data"]["id"].as_str().unwrap();
    send(&app, Method::DELETE, &base, None, Some(&owner)).await;
    let (status, body) = bridge_control(&app, Method::GET, "/v1/assistant/cleanup").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"], json!([]));
    let ack = format!("/v1/assistant/cleanup/{sid}/ack");
    assert_eq!(
        bridge_control(&app, Method::POST, &ack).await.0,
        StatusCode::CONFLICT
    );
    sqlx::query("UPDATE assistant_uploads SET created_at=now()-interval '61 seconds' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&ws).bind(upload).execute(&pool).await.unwrap();
    let (_, body) = bridge_control(&app, Method::GET, "/v1/assistant/cleanup").await;
    assert_eq!(body["data"][0]["conversation_id"], "conv_cleanup");
    assert_eq!(
        bridge_control(&app, Method::POST, &ack).await.0,
        StatusCode::OK
    );
    assert_eq!(
        bridge_control(&app, Method::POST, &ack).await.0,
        StatusCode::OK
    );
    let (_, body) = send(&app, Method::GET, &base, None, Some(&owner)).await;
    assert_eq!(body["data"]["state"], "cleaned");
    assert!(body["data"]["conversation_id"].is_null());
    assert_eq!(
        signed(
            &app,
            &format!("{base}/uploads/{upload}/complete"),
            json!({"file_id":"late_file"}),
            &owner
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[sqlx::test]
async fn receipt_profile_get_requires_approval_and_dispatches_one_email(pool: PgPool) {
    let (port, inbox) = common::fake_smtp::start().await;
    let (mailer, _) = genesy_360_api::mail::Mailer::capture();
    let mailer = mailer.with_trusted_smtp_hosts(&["localhost".to_owned()]);
    let mut state = common::state(pool, mailer);
    state.assistant_bridge_secret = Some(std::sync::Arc::new(
        genesy_360_api::AssistantSecret::new(BRIDGE.into()).unwrap(),
    ));
    let app = genesy_360_api::build_app(state);
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    let sid = session(&app, &ws, &owner).await;
    let(status,body)=send(&app,Method::POST,&format!("/v1/workspace/{ws}/email-profiles"),Some(json!({"name":"Guarded profile","sender_name":"Test","email":"test@example.com","default":true,"smtp":{"host":"localhost","port":port}})),Some(&owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let profile = body["data"]["email_profile_id"].as_str().unwrap();
    let target = format!("/v1/workspace/{ws}/email-profiles/{profile}/test-connection");
    let base = format!("/v1/workspace/{ws}/assistant/actions");
    let(status,body)=send(&app,Method::POST,&base,Some(json!({"session_id":sid,"operation_id":"messages_profile_test","method":"GET","path":target})),Some(&owner)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["data"]["id"].as_str().unwrap();
    let headers = [
        ("Assistant-Action-Id", id),
        ("Assistant-Session-Id", sid.as_str()),
    ];
    assert_eq!(
        send_raw(&app, Method::GET, &target, None, Some(&owner), &headers)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert!(inbox.lock().unwrap().is_empty());
    assert_eq!(
        signed(
            &app,
            &format!("{base}/{id}/approve"),
            json!({"session_id":sid}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send_raw(&app, Method::GET, &target, None, Some(&owner), &headers)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        send_raw(&app, Method::GET, &target, None, Some(&owner), &headers)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(inbox.lock().unwrap().len(), 1);
    let (_, status) = send(
        &app,
        Method::GET,
        &format!("{base}/{id}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(status["data"]["state"], "queued");
    assert!(status["data"]["finished_at"].is_string());
    assert_eq!(status["data"]["evidence"]["delivery"]["status"], "accepted");
    assert!(status["data"]["evidence"]["delivery"]["send_log_id"].is_string());
}

#[sqlx::test]
async fn pin_credentials_are_refused_before_a_business_draft_is_stored(pool: PgPool) {
    let app = common::app(pool.clone());
    let (owner, ws) = signup(&app, "pin-review@example.test", "pin-review").await;
    let sid = session(&app, &ws, &owner).await;
    let (status,body)=send(&app,Method::POST,&format!("/v1/workspace/{ws}/assistant/actions"),Some(json!({"session_id":sid,"operation_id":"till_users_create","method":"POST","path":format!("/v1/workspace/{ws}/till/users"),"body":{"name":"Cashier","pin":"4821"}})),Some(&owner)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["errors"]["body"][0],
        "Credentials require the secure Office form."
    );
    let stored: i64 =
        sqlx::query_scalar("SELECT count(*) FROM assistant_actions WHERE workspace_id=$1::uuid")
            .bind(&ws)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, 0);
}
