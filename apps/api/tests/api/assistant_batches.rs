//! Fixed batch approval binds ordered immutable requests and stops unadmitted work.

use crate::common::{self, send, send_raw, signup};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
const KEY: &str = "assistant-batch-bridge-secret-for-isolated-tests";
async fn world(pool: PgPool) -> (Router, String, String, String) {
    let mut state = common::state(pool, genesy_360_api::mail::Mailer::capture().0);
    state.assistant_bridge_secret = Some(std::sync::Arc::new(
        genesy_360_api::AssistantSecret::new(KEY.into()).unwrap(),
    ));
    let app = genesy_360_api::build_app(state);
    let (owner, workspace) = signup(&app, "batch@example.test", "batch").await;
    let (status, session) = send(
        &app,
        Method::POST,
        &format!("/v1/workspace/{workspace}/assistant/sessions"),
        Some(json!({"language":"en"})),
        Some(&owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let session = session["data"]["id"].as_str().unwrap().to_owned();
    (app, owner, workspace, session)
}
async fn signed(app: &Router, path: &str, value: Value, owner: &str) -> (StatusCode, Value) {
    let stamp = chrono::Utc::now().timestamp().to_string();
    let message = format!(
        "{stamp}\nPOST\n{path}\n{}",
        hex::encode(Sha256::digest(value.to_string().as_bytes()))
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(KEY.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let (status, body) = send_raw(
        app,
        Method::POST,
        path,
        Some(value),
        Some(owner),
        &[
            ("Assistant-Bridge-Timestamp", &stamp),
            ("Assistant-Bridge-Signature", &signature),
        ],
    )
    .await;
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}
fn body(workspace: &str, name: &str) -> Value {
    json!({"operation_id":"warehouses_create","method":"POST","path":format!("/v1/workspace/{workspace}/warehouses"),"body":{"name":name}})
}
async fn create(app: &Router, owner: &str, workspace: &str, session: &str) -> Value {
    let path = format!("/v1/workspace/{workspace}/assistant/batches");
    let(status,result)=signed(app,&path,json!({"session_id":session,"actions":[body(workspace,"Batch warehouse one"),body(workspace,"Batch warehouse two")]}),owner).await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    result["data"].clone()
}
#[allow(clippy::too_many_arguments)]
async fn dispatch(
    app: &Router,
    owner: &str,
    workspace: &str,
    session: &str,
    action: &str,
    batch: &str,
    execution: &str,
    name: &str,
) -> (StatusCode, Value) {
    let path = format!("/v1/workspace/{workspace}/warehouses");
    let (status, body) = send_raw(
        app,
        Method::POST,
        &path,
        Some(json!({"name":name})),
        Some(owner),
        &[
            ("Assistant-Action-Id", action),
            ("Assistant-Session-Id", session),
            ("Assistant-Batch-Id", batch),
            ("Assistant-Batch-Token", execution),
        ],
    )
    .await;
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}
#[sqlx::test]
async fn exact_ordered_batch_needs_one_independent_approval_and_verified_step_advance(
    pool: PgPool,
) {
    let (app, owner, workspace, session) = world(pool.clone()).await;
    let batch = create(&app, &owner, &workspace, &session).await;
    let id = batch["id"].as_str().unwrap();
    let steps = batch["steps"].as_array().unwrap();
    let first = steps[0]["id"].as_str().unwrap();
    let second = steps[1]["id"].as_str().unwrap();
    assert_eq!(batch["state"], "proposed");
    assert_eq!(batch["fingerprint"].as_str().unwrap().len(), 64);
    let base = format!("/v1/workspace/{workspace}/assistant");
    assert_eq!(
        signed(
            &app,
            &format!("{base}/actions/{first}/approve"),
            json!({"session_id":session}),
            &owner
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("{base}/batches/{id}/approve"),
            Some(json!({"session_id":session})),
            Some(&owner)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        signed(
            &app,
            &format!("{base}/batches/{id}/approve"),
            json!({"session_id":session}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, start) = signed(
        &app,
        &format!("{base}/batches/{id}/start"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{start}");
    let execution = start["data"]["execution_token"].as_str().unwrap();
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            second,
            id,
            execution,
            "Batch warehouse two"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            first,
            id,
            execution,
            "Batch warehouse one"
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            second,
            id,
            execution,
            "Batch warehouse two"
        )
        .await
        .0,
        StatusCode::CONFLICT,
        "HTTPsuccessalone cannot admit nextstep"
    );
    assert_eq!(
        signed(
            &app,
            &format!("{base}/batches/{id}/advance"),
            json!({"session_id":session,"execution_token":execution,"action_id":first}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            second,
            id,
            execution,
            "Batch warehouse two"
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (status, end) = signed(
        &app,
        &format!("{base}/batches/{id}/advance"),
        json!({"session_id":session,"execution_token":execution,"action_id":second}),
        &owner,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{end}");
    assert_eq!(end["data"]["state"], "completed");
    assert_eq!(end["data"]["next_index"], 2);
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            first,
            id,
            execution,
            "Batch warehouse one"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM warehouses WHERE workspace_id=$1::uuid AND name LIKE 'Batch warehouse%'").bind(&workspace).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 2);
}
#[sqlx::test]
async fn paused_batch_never_dispatches_remainder_or_restarts_consumed_controller(pool: PgPool) {
    let (app, owner, workspace, session) = world(pool.clone()).await;
    let batch = create(&app, &owner, &workspace, &session).await;
    let id = batch["id"].as_str().unwrap();
    let first = batch["steps"][0]["id"].as_str().unwrap();
    let second = batch["steps"][1]["id"].as_str().unwrap();
    let base = format!("/v1/workspace/{workspace}/assistant/batches/{id}");
    signed(
        &app,
        &format!("{base}/approve"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let (_, start) = signed(
        &app,
        &format!("{base}/start"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let execution = start["data"]["execution_token"].as_str().unwrap();
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            first,
            id,
            execution,
            "Batch warehouse one"
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("{base}/pause"),
            Some(json!({"session_id":session,"reason":"disconnected"})),
            Some(&owner)
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, current) = send(&app, Method::GET, &base, None, Some(&owner)).await;
    assert_eq!(current["data"]["state"], "paused");
    assert!(
        current["data"]["steps"][1]["proposal"].is_null(),
        "Paused work cannot resume, so its unclaimed draft must be purged."
    );
    assert_eq!(
        current["data"]["steps"][0]["state"], "unknown",
        "unverified HTTPcompletion staysuncertain"
    );
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            second,
            id,
            execution,
            "Batch warehouse two"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        signed(
            &app,
            &format!("{base}/start"),
            json!({"session_id":session}),
            &owner
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[sqlx::test]
async fn one_final_user_event_cannot_authorize_single_and_batch_scopes(pool: PgPool) {
    let (app, owner, workspace, session) = world(pool).await;
    let base = format!("/v1/workspace/{workspace}/assistant");
    let event = "d".repeat(64);
    let mut proposal = body(&workspace, "Standalone proof");
    proposal["session_id"] = json!(session);
    let (status, one) = send(
        &app,
        Method::POST,
        &format!("{base}/actions"),
        Some(proposal.clone()),
        Some(&owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{one}");
    let one = one["data"]["id"].as_str().unwrap();
    assert_eq!(
        signed(
            &app,
            &format!("{base}/actions/{one}/approve"),
            json!({"session_id":session,"approval_event_sha256":event}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    send(
        &app,
        Method::POST,
        &format!("{base}/actions/{one}/cancel"),
        Some(json!({"session_id":session})),
        Some(&owner),
    )
    .await;
    let batch = create(&app, &owner, &workspace, &session).await;
    let id = batch["id"].as_str().unwrap();
    assert_eq!(
        signed(
            &app,
            &format!("{base}/batches/{id}/approve"),
            json!({"session_id":session,"approval_event_sha256":event}),
            &owner
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let batch_event = "e".repeat(64);
    assert_eq!(
        signed(
            &app,
            &format!("{base}/batches/{id}/approve"),
            json!({"session_id":session,"approval_event_sha256":batch_event}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    send(
        &app,
        Method::POST,
        &format!("{base}/batches/{id}/cancel"),
        Some(json!({"session_id":session,"reason":"cancelled"})),
        Some(&owner),
    )
    .await;
    let (status, two) = send(
        &app,
        Method::POST,
        &format!("{base}/actions"),
        Some(proposal),
        Some(&owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{two}");
    let two = two["data"]["id"].as_str().unwrap();
    assert_eq!(
        signed(
            &app,
            &format!("{base}/actions/{two}/approve"),
            json!({"session_id":session,"approval_event_sha256":batch_event}),
            &owner
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[sqlx::test]
async fn disconnect_pause_after_admission_allows_current_commit_but_no_next_dispatch(pool: PgPool) {
    let (app, owner, workspace, session) = world(pool.clone()).await;
    let batch = create(&app, &owner, &workspace, &session).await;
    let id = batch["id"].as_str().unwrap().to_owned();
    let first = batch["steps"][0]["id"].as_str().unwrap().to_owned();
    let second = batch["steps"][1]["id"].as_str().unwrap().to_owned();
    let base = format!("/v1/workspace/{workspace}/assistant/batches/{id}");
    signed(
        &app,
        &format!("{base}/approve"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let (_, start) = signed(
        &app,
        &format!("{base}/start"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let execution = start["data"]["execution_token"]
        .as_str()
        .unwrap()
        .to_owned();
    sqlx::raw_sql("CREATE FUNCTION slow_batch_insert() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.4);RETURN NEW;END $$;CREATE TRIGGER slow_batch BEFORE INSERT ON warehouses FOR EACH ROW EXECUTE FUNCTION slow_batch_insert();").execute(&pool).await.unwrap();
    let running = {
        let (app, owner, workspace, session, first, id, execution) = (
            app.clone(),
            owner.clone(),
            workspace.clone(),
            session.clone(),
            first.clone(),
            id.clone(),
            execution.clone(),
        );
        tokio::spawn(async move {
            dispatch(
                &app,
                &owner,
                &workspace,
                &session,
                &first,
                &id,
                &execution,
                "Batch warehouse one",
            )
            .await
        })
    };
    let mut admitted = false;
    for _ in 0..100 {
        let state: String = sqlx::query_scalar(
            "SELECT state FROM assistant_actions WHERE workspace_id=$1::uuid AND id=$2::uuid",
        )
        .bind(&workspace)
        .bind(&first)
        .fetch_one(&pool)
        .await
        .unwrap();
        if state == "dispatching" {
            admitted = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(admitted);
    let (status, pause) = send(
        &app,
        Method::POST,
        &format!("{base}/pause"),
        Some(json!({"session_id":session,"reason":"disconnected"})),
        Some(&owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pause}");
    assert_eq!(running.await.unwrap().0, StatusCode::CREATED);
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            &second,
            &id,
            &execution,
            "Batch warehouse two"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM warehouses WHERE workspace_id=$1::uuid AND name LIKE 'Batch warehouse%'").bind(&workspace).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn postcommit_ledger_loss_does_not_advance_or_replay_batch(pool: PgPool) {
    let (app, owner, workspace, session) = world(pool.clone()).await;
    let batch = create(&app, &owner, &workspace, &session).await;
    let id = batch["id"].as_str().unwrap();
    let first = batch["steps"][0]["id"].as_str().unwrap();
    let second = batch["steps"][1]["id"].as_str().unwrap();
    let base = format!("/v1/workspace/{workspace}/assistant/batches/{id}");
    signed(
        &app,
        &format!("{base}/approve"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let (_, start) = signed(
        &app,
        &format!("{base}/start"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let execution = start["data"]["execution_token"].as_str().unwrap();
    sqlx::raw_sql("CREATE FUNCTION fail_batch_finalize() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.batch_id IS NOT NULL AND NEW.state='completed' THEN RAISE EXCEPTION 'injected response finalization loss';END IF;RETURN NEW;END $$;CREATE TRIGGER fail_batch_finalize BEFORE UPDATE ON assistant_actions FOR EACH ROW EXECUTE FUNCTION fail_batch_finalize();").execute(&pool).await.unwrap();
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            first,
            id,
            execution,
            "Batch warehouse one"
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        signed(
            &app,
            &format!("{base}/advance"),
            json!({"session_id":session,"execution_token":execution,"action_id":first}),
            &owner
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    send(
        &app,
        Method::POST,
        &format!("{base}/pause"),
        Some(json!({"session_id":session,"reason":"unknown"})),
        Some(&owner),
    )
    .await;
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            second,
            id,
            execution,
            "Batch warehouse two"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            first,
            id,
            execution,
            "Batch warehouse one"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM warehouses WHERE workspace_id=$1::uuid AND name LIKE 'Batch warehouse%'").bind(&workspace).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn first_step_changes_material_facts_second_step_requires_new_review(pool: PgPool) {
    let (app, owner, workspace, session) = world(pool.clone()).await;
    let (_, wh) = send(
        &app,
        Method::POST,
        &format!("/v1/workspace/{workspace}/warehouses"),
        Some(json!({"name":"Stock main","default_warehouse":true})),
        Some(&owner),
    )
    .await;
    let wh = wh["data"]["warehouse_id"].as_str().unwrap();
    let (_, product) = send(
        &app,
        Method::POST,
        &format!("/v1/workspace/{workspace}/products"),
        Some(json!({"name":"Batch stock","price":"1","has_stock":true})),
        Some(&owner),
    )
    .await;
    let product = product["data"]["product_id"].as_str().unwrap();
    let path = format!("/v1/workspace/{workspace}/products/{product}/set-stock-levels");
    let firstbody =
        json!({"warehouses":[{"warehouse_id":wh,"stock_lvl":"10","reason":"First count"}]});
    let secondbody =
        json!({"warehouses":[{"warehouse_id":wh,"stock_lvl":"20","reason":"Second count"}]});
    let base = format!("/v1/workspace/{workspace}/assistant");
    let(status,result)=signed(&app,&format!("{base}/batches"),json!({"session_id":session,"actions":[{"operation_id":"stock_set_levels","method":"POST","path":path,"body":firstbody},{"operation_id":"stock_set_levels","method":"POST","path":path,"body":secondbody}]}),&owner).await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    let id = result["data"]["id"].as_str().unwrap();
    let first = result["data"]["steps"][0]["id"].as_str().unwrap();
    let second = result["data"]["steps"][1]["id"].as_str().unwrap();
    signed(
        &app,
        &format!("{base}/batches/{id}/approve"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let (_, start) = signed(
        &app,
        &format!("{base}/batches/{id}/start"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let execution = start["data"]["execution_token"].as_str().unwrap();
    let headers = |action| {
        [
            ("Assistant-Action-Id", action),
            ("Assistant-Session-Id", session.as_str()),
            ("Assistant-Batch-Id", id),
            ("Assistant-Batch-Token", execution),
        ]
    };
    assert_eq!(
        send_raw(
            &app,
            Method::POST,
            &path,
            Some(firstbody),
            Some(&owner),
            &headers(first)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        signed(
            &app,
            &format!("{base}/batches/{id}/advance"),
            json!({"session_id":session,"execution_token":execution,"action_id":first}),
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
            &path,
            Some(secondbody),
            Some(&owner),
            &headers(second)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let level:String=sqlx::query_scalar("SELECT stock_lvl::text FROM stocks WHERE workspace_id=$1::uuid AND product_id=$2::uuid AND warehouse_id=$3::uuid AND bin_id IS NULL AND lot_id IS NULL").bind(&workspace).bind(product).bind(wh).fetch_one(&pool).await.unwrap();
    assert_eq!(level, "10.00");
}

#[sqlx::test]
async fn batch_expiry_and_private_controller_are_enforced(pool: PgPool) {
    let (app, owner, workspace, session) = world(pool.clone()).await;
    let batch = create(&app, &owner, &workspace, &session).await;
    let id = batch["id"].as_str().unwrap();
    let first = batch["steps"][0]["id"].as_str().unwrap();
    let base = format!("/v1/workspace/{workspace}/assistant/batches/{id}");
    let (other, _) = signup(&app, "other-batch@example.test", "other-batch").await;
    assert!(matches!(
        send(&app, Method::GET, &base, None, Some(&other)).await.0,
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    assert_eq!(
        signed(
            &app,
            &format!("{base}/approve"),
            json!({"session_id":session}),
            &owner
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, start) = signed(
        &app,
        &format!("{base}/start"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let proof = start["data"]["execution_token"].as_str().unwrap();
    assert_eq!(
        signed(
            &app,
            &format!("{base}/start"),
            json!({"session_id":session}),
            &owner
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let hash: Vec<u8> = sqlx::query_scalar(
        "SELECT run_hash FROM assistant_batches WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&workspace)
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(hash, Sha256::digest(proof.as_bytes()).to_vec());
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            first,
            id,
            &"a".repeat(64),
            "Batch warehouse one"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let status = send(&app, Method::GET, &base, None, Some(&owner)).await.1;
    assert!(!status.to_string().contains(proof));
    assert!(status["data"].get("run_hash").is_none());
    sqlx::query("UPDATE assistant_batches SET expires_at=clock_timestamp()-interval '1 second' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&workspace).bind(id).execute(&pool).await.unwrap();
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            first,
            id,
            proof,
            "Batch warehouse one"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(&app, Method::GET, &base, None, Some(&owner)).await.1["data"]["state"],
        "expired"
    );
}

#[sqlx::test]
async fn cancelled_batch_and_ended_session_block_remaining_claims(pool: PgPool) {
    let (app, owner, workspace, session) = world(pool.clone()).await;
    let batch = create(&app, &owner, &workspace, &session).await;
    let id = batch["id"].as_str().unwrap();
    let first = batch["steps"][0]["id"].as_str().unwrap();
    let base = format!("/v1/workspace/{workspace}/assistant/batches/{id}");
    signed(
        &app,
        &format!("{base}/approve"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let (_, start) = signed(
        &app,
        &format!("{base}/start"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let proof = start["data"]["execution_token"].as_str().unwrap();
    let (status, cancel) = send(
        &app,
        Method::POST,
        &format!("{base}/cancel"),
        Some(json!({"session_id":session,"reason":"changed_intent"})),
        Some(&owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancel["data"]["state"], "cancelled");
    assert!(
        cancel["data"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["proposal"].is_null() && s["state"] == "cancelled")
    );
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            first,
            id,
            proof,
            "Batch warehouse one"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let next = create(&app, &owner, &workspace, &session).await;
    let id = next["id"].as_str().unwrap();
    let first = next["steps"][0]["id"].as_str().unwrap();
    let base = format!("/v1/workspace/{workspace}/assistant/batches/{id}");
    signed(
        &app,
        &format!("{base}/approve"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let (_, start) = signed(
        &app,
        &format!("{base}/start"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    let proof = start["data"]["execution_token"].as_str().unwrap();
    sqlx::query(
        "UPDATE assistant_sessions SET state='closed' WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&workspace)
    .bind(&session)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        dispatch(
            &app,
            &owner,
            &workspace,
            &session,
            first,
            id,
            proof,
            "Batch warehouse one"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM warehouses WHERE workspace_id=$1::uuid AND name LIKE 'Batch warehouse%'").bind(&workspace).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn terminal_delivery_evidence_resolves_unknown_batch_step_without_advancing(pool: PgPool) {
    let (app, owner, workspace, session) = world(pool.clone()).await;
    let batch = create(&app, &owner, &workspace, &session).await;
    let id = batch["id"].as_str().unwrap();
    let first = batch["steps"][0]["id"].as_str().unwrap();
    let second = batch["steps"][1]["id"].as_str().unwrap();
    send(
        &app,
        Method::POST,
        &format!("/v1/workspace/{workspace}/assistant/batches/{id}/pause"),
        Some(json!({"session_id":session,"reason":"unknown"})),
        Some(&owner),
    )
    .await;
    sqlx::query("UPDATE assistant_actions SET operation_id='messages_send',state='queued',batch_verdict='unknown',proposal=NULL,review_facts='{}'::jsonb,finished_at=now(),evidence='{\"delivery\":{\"status\":\"accepted\"}}'::jsonb WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&workspace).bind(first).execute(&pool).await.unwrap();
    sqlx::query("UPDATE assistant_actions SET state='completed',batch_verdict='unknown',proposal=NULL,review_facts='{}'::jsonb,finished_at=now(),evidence='{\"meaning\":\"endpoint_completed\"}'::jsonb WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&workspace).bind(second).execute(&pool).await.unwrap();
    genesy_360_api::cleanup::prune(&pool).await.unwrap();
    let (_, status) = send(
        &app,
        Method::GET,
        &format!("/v1/workspace/{workspace}/assistant/batches/{id}"),
        None,
        Some(&owner),
    )
    .await;
    assert_eq!(status["data"]["steps"][0]["state"], "queued");
    assert_eq!(status["data"]["steps"][1]["state"], "unknown");
    assert_eq!(status["data"]["state"], "paused");
    assert_eq!(status["data"]["next_index"], 0);
}

#[sqlx::test]
async fn expired_controller_lease_purges_unstarted_drafts_without_admission(pool: PgPool) {
    let (app, owner, workspace, session) = world(pool.clone()).await;
    let batch = create(&app, &owner, &workspace, &session).await;
    let id = batch["id"].as_str().unwrap();
    let base = format!("/v1/workspace/{workspace}/assistant/batches/{id}");
    signed(
        &app,
        &format!("{base}/approve"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    signed(
        &app,
        &format!("{base}/start"),
        json!({"session_id":session}),
        &owner,
    )
    .await;
    sqlx::query("UPDATE assistant_batches SET run_expires_at=clock_timestamp()-interval '1 second' WHERE workspace_id=$1::uuid AND id=$2::uuid").bind(&workspace).bind(id).execute(&pool).await.unwrap();
    genesy_360_api::cleanup::prune(&pool).await.unwrap();
    let state: String = sqlx::query_scalar(
        "SELECT state FROM assistant_batches WHERE workspace_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&workspace)
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "paused");
    let drafts:i64=sqlx::query_scalar("SELECT count(*) FROM assistant_actions WHERE workspace_id=$1::uuid AND batch_id=$2::uuid AND (proposal IS NOT NULL OR review_facts<>'{}'::jsonb)").bind(&workspace).bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(drafts, 0);
    let effects:i64=sqlx::query_scalar("SELECT count(*) FROM warehouses WHERE workspace_id=$1::uuid AND name LIKE 'Batch warehouse%'").bind(&workspace).fetch_one(&pool).await.unwrap();
    assert_eq!(effects, 0);
}
