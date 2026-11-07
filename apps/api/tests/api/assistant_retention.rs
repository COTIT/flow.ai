//! Assistant retention preserves unresolved actions and isolates session-owned context.

use crate::common::Api;
use genesy_360_api::cleanup;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn context(api: &Api, days: i32, state: &str) -> Uuid {
    let session = api
        .with_base("/assistant/sessions")
        .create(json!({"language":"en"}), "id")
        .await;
    let id = Uuid::parse_str(&session).unwrap();
    let workspace = Uuid::parse_str(&api.ws).unwrap();
    sqlx::query("UPDATE assistant_sessions SET state=$2,cleaned_at=now()-make_interval(days=>$3) WHERE id=$1 AND workspace_id=$4")
        .bind(id).bind(state).bind(days).bind(workspace).execute(&api.pool).await.unwrap();
    sqlx::query("INSERT INTO assistant_uploads(workspace_id,session_id,media_type,size_bytes,state) VALUES($1,$2,'image/png',1,'cleaned')")
        .bind(workspace).bind(id).execute(&api.pool).await.unwrap();
    sqlx::query("INSERT INTO assistant_checkpoint.checkpoints(thread_id,checkpoint_id,checkpoint) VALUES($1,'checkpoint','{}')")
        .bind(&session).execute(&api.pool).await.unwrap();
    sqlx::query("INSERT INTO assistant_checkpoint.checkpoint_blobs(thread_id,channel,version,type,blob) VALUES($1,'status','1','json',$2)")
        .bind(&session).bind(b"context".as_slice()).execute(&api.pool).await.unwrap();
    sqlx::query("INSERT INTO assistant_checkpoint.checkpoint_writes(thread_id,checkpoint_id,task_id,idx,channel,type,blob) VALUES($1,'checkpoint','task',0,'status','json',$2)")
        .bind(&session).bind(b"context".as_slice()).execute(&api.pool).await.unwrap();
    id
}

async fn counts(api: &Api, id: Uuid) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM assistant_sessions WHERE workspace_id=$1 AND id=$2),(SELECT count(*) FROM assistant_uploads WHERE workspace_id=$1 AND session_id=$2),(SELECT count(*) FROM assistant_checkpoint.checkpoint_writes WHERE thread_id=$3),(SELECT count(*) FROM assistant_checkpoint.checkpoint_blobs WHERE thread_id=$3),(SELECT count(*) FROM assistant_checkpoint.checkpoints WHERE thread_id=$3)")
        .bind(Uuid::parse_str(&api.ws).unwrap()).bind(id).bind(id.to_string()).fetch_one(&api.pool).await.unwrap()
}

#[sqlx::test]
async fn old_cleaned_context_is_purged_with_its_checkpoints(pool: PgPool) {
    let api = Api::new(pool.clone(), "/assistant").await;
    let id = context(&api, 91, "cleaned").await;
    cleanup::prune(&pool).await.unwrap();
    assert_eq!(counts(&api, id).await, (0, 0, 0, 0, 0));
    cleanup::prune(&pool).await.unwrap();
}

#[sqlx::test]
async fn retention_preserves_recent_and_not_yet_cleaned_sessions(pool: PgPool) {
    let api = Api::new(pool.clone(), "/assistant").await;
    let recent = context(&api, 89, "cleaned").await;
    let active = context(&api, 91, "active").await;
    let closed = context(&api, 91, "closed").await;
    cleanup::prune(&pool).await.unwrap();
    for id in [recent, active, closed] {
        assert_eq!(counts(&api, id).await, (1, 1, 1, 1, 1));
    }
}

#[sqlx::test]
async fn any_remaining_action_preserves_all_session_context(pool: PgPool) {
    let api = Api::new(pool.clone(), "/assistant").await;
    for state in [
        "unknown",
        "dispatching",
        "proposed",
        "approved",
        "completed",
        "queued",
        "partial",
    ] {
        let id = context(&api, 91, "cleaned").await;
        sqlx::query("INSERT INTO assistant_actions(workspace_id,session_id,user_id,login_id,operation_id,step_index,request_hash,state) SELECT workspace_id,id,user_id,login_id,'products_create',1,$2,$3 FROM assistant_sessions WHERE id=$1")
            .bind(id).bind([0_u8;32].as_slice()).bind(state).execute(&pool).await.unwrap();
        cleanup::prune(&pool).await.unwrap();
        assert_eq!(counts(&api, id).await, (1, 1, 1, 1, 1));
    }
}

#[sqlx::test]
async fn removing_one_workspace_context_keeps_another_workspaces_unresolved_context(pool: PgPool) {
    let first = Api::new(pool.clone(), "/assistant").await;
    let other = first.as_new_owner("other@example.com", "other").await;
    let gone = context(&first, 91, "cleaned").await;
    let retained = context(&other, 91, "cleaned").await;
    sqlx::query("INSERT INTO assistant_actions(workspace_id,session_id,user_id,login_id,operation_id,step_index,request_hash,state) SELECT workspace_id,id,user_id,login_id,'products_create',1,$2,'unknown' FROM assistant_sessions WHERE id=$1")
        .bind(retained).bind([0_u8;32].as_slice()).execute(&pool).await.unwrap();
    cleanup::prune(&pool).await.unwrap();
    assert_eq!(counts(&first, gone).await, (0, 0, 0, 0, 0));
    assert_eq!(counts(&other, retained).await, (1, 1, 1, 1, 1));
}

#[sqlx::test]
async fn cleaned_session_with_remote_upload_reference_remains_recoverable(pool: PgPool) {
    let api = Api::new(pool.clone(), "/assistant").await;
    let id = context(&api, 91, "cleaned").await;
    sqlx::query("UPDATE assistant_uploads SET state='cleanup_pending',file_id='provider_reference' WHERE workspace_id=$1 AND session_id=$2")
        .bind(Uuid::parse_str(&api.ws).unwrap()).bind(id).execute(&pool).await.unwrap();
    cleanup::prune(&pool).await.unwrap();
    assert_eq!(counts(&api, id).await, (1, 1, 1, 1, 1));
}

#[sqlx::test]
async fn retention_respects_both_sides_of_ninety_day_cutoff(pool: PgPool) {
    let api = Api::new(pool.clone(), "/assistant").await;
    let old = context(&api, 91, "cleaned").await;
    let recent = context(&api, 89, "cleaned").await;
    sqlx::query("UPDATE assistant_sessions SET cleaned_at=now()-interval '90 days'-interval '1 minute' WHERE workspace_id=$1 AND id=$2")
        .bind(Uuid::parse_str(&api.ws).unwrap()).bind(old).execute(&pool).await.unwrap();
    sqlx::query("UPDATE assistant_sessions SET cleaned_at=now()-interval '90 days'+interval '1 minute' WHERE workspace_id=$1 AND id=$2")
        .bind(Uuid::parse_str(&api.ws).unwrap()).bind(recent).execute(&pool).await.unwrap();
    cleanup::prune(&pool).await.unwrap();
    assert_eq!(counts(&api, old).await, (0, 0, 0, 0, 0));
    assert_eq!(counts(&api, recent).await, (1, 1, 1, 1, 1));
}
