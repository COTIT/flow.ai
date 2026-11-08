//! Workspace-filtered assistant SQL and atomic claim/quota transitions.

use super::dto::{CompleteInput, ProposalInput, UploadInput};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

/// Authenticated actor and original login; no bearer retained.
#[derive(Clone, Copy)]
pub struct Owner {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub login_id: Uuid,
}

/// Owned session row; `usable` includes hard expiry and orphan timeout.
pub struct Session {
    pub id: Uuid,
    pub language: String,
    pub state: String,
    pub conversation_id: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub usable: bool,
}
impl Session {
    /// Safe session control resource.
    pub fn resource(&self) -> Value {
        json!({"id":self.id,"task_id":self.id,"language":self.language,"state":if self.state=="active"&&!self.usable{"expired"}else{&self.state},"conversation_id":self.conversation_id,"expires_at":self.expires_at,"last_seen_at":self.last_seen_at})
    }
}

/// New login-bound session.
pub async fn create_session(
    db: impl PgExecutor<'_>,
    o: Owner,
    language: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!("INSERT INTO assistant_sessions(workspace_id,user_id,login_id,language) VALUES($1,$2,$3,$4) RETURNING id",o.workspace_id,o.user_id,o.login_id,language).fetch_one(db).await
}

/// Session lookup before route workspace is known; login and actor remain required.
pub async fn session_workspace(
    db: impl PgExecutor<'_>,
    user: Uuid,
    login: Uuid,
    id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT workspace_id FROM assistant_sessions WHERE user_id=$1 AND login_id=$2 AND id=$3",
        user,
        login,
        id
    )
    .fetch_optional(db)
    .await
}

/// Owned session, including closed sessions for history.
pub async fn session(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
) -> Result<Option<Session>, sqlx::Error> {
    sqlx::query_as!(Session,r#"SELECT id,language,state,conversation_id,expires_at,last_seen_at,(state='active' AND expires_at>now() AND last_seen_at>now()-interval '15 minutes') AS "usable!" FROM assistant_sessions WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4"#,o.workspace_id,o.user_id,o.login_id,id).fetch_optional(db).await
}

/// Owned session plus `/me/workspaces` ids and `/farm/me` roles here, one statement.
pub async fn session_identity(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
) -> Result<Option<(Session, Vec<Uuid>, Vec<String>)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT s.id, s.language, s.state, s.conversation_id, s.expires_at, s.last_seen_at,
                  (s.state='active' AND s.expires_at>now() AND s.last_seen_at>now()-interval '15 minutes') AS "usable!",
                  ARRAY(SELECT m.workspace_id FROM workspace_members m
                        JOIN workspaces w ON w.id = m.workspace_id AND w.deleted_at IS NULL
                        WHERE m.user_id = s.user_id AND m.active
                        ORDER BY m.created_at, m.workspace_id) AS "workspaces!",
                  ARRAY(SELECT f.role FROM farm_memberships f
                        JOIN workspaces w ON w.id = f.workspace_id AND w.deleted_at IS NULL
                        WHERE f.user_id = s.user_id AND f.workspace_id = s.workspace_id
                          AND f.revoked_at IS NULL
                          AND (f.role = 'farmer' OR EXISTS (
                                SELECT 1 FROM workspace_members wm
                                WHERE wm.workspace_id = f.workspace_id AND wm.user_id = f.user_id AND wm.active))
                        GROUP BY f.role
                        ORDER BY array_position(ARRAY['admin','ops','field_officer','farmer'], f.role)) AS "farm_roles!"
           FROM assistant_sessions s
           WHERE s.workspace_id=$1 AND s.user_id=$2 AND s.login_id=$3 AND s.id=$4"#,
        o.workspace_id,
        o.user_id,
        o.login_id,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| {
        (
            Session {
                id: r.id,
                language: r.language,
                state: r.state,
                conversation_id: r.conversation_id,
                expires_at: r.expires_at,
                last_seen_at: r.last_seen_at,
                usable: r.usable,
            },
            r.workspaces,
            r.farm_roles,
        )
    }))
}

/// Serialize quota and task-step allocation on one owned session row.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn lock_session(
    db: &mut PgConnection,
    o: Owner,
    id: Uuid,
) -> Result<Option<Session>, sqlx::Error> {
    sqlx::query_as!(Session,r#"SELECT id,language,state,conversation_id,expires_at,last_seen_at,(state='active' AND expires_at>now() AND last_seen_at>now()-interval '15 minutes') AS "usable!" FROM assistant_sessions WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 FOR UPDATE"#,o.workspace_id,o.user_id,o.login_id,id).fetch_optional(db).await
}

/// Close without deleting cleanup references.
pub async fn close_session(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_sessions SET state=CASE WHEN state='cleaned' THEN state ELSE 'closed' END,closed_at=coalesce(closed_at,now()) WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4",o.workspace_id,o.user_id,o.login_id,id).execute(db).await?.rows_affected())
}

/// Heartbeat cannot revive expired or abandoned sessions.
pub async fn heartbeat(db: impl PgExecutor<'_>, o: Owner, id: Uuid) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_sessions SET last_seen_at=now() WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND state='active' AND expires_at>now() AND last_seen_at>now()-interval '15 minutes'",o.workspace_id,o.user_id,o.login_id,id).execute(db).await?.rows_affected())
}

/// Attach once; same provider reference is idempotent.
pub async fn attach(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    conversation: &str,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_sessions SET conversation_id=$5 WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND state='active' AND expires_at>now() AND last_seen_at>now()-interval '15 minutes' AND (conversation_id IS NULL OR conversation_id=$5)",o.workspace_id,o.user_id,o.login_id,id,conversation).execute(db).await?.rows_affected())
}

/// Store bounded business draft; session lock makes step ordering stable.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn propose(
    db: &mut PgConnection,
    o: Owner,
    input: &ProposalInput,
    hash: &[u8],
    proposal: &Value,
    review_facts: &Value,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!("INSERT INTO assistant_actions(workspace_id,session_id,user_id,login_id,operation_id,step_index,request_hash,proposal,review_facts) VALUES($1,$2,$3,$4,$5,(SELECT coalesce(max(step_index),0)+1 FROM assistant_actions WHERE workspace_id=$1 AND session_id=$2),$6,$7,$8) RETURNING id",o.workspace_id,input.session_id,o.user_id,o.login_id,input.operation_id,hash,proposal,review_facts).fetch_one(db).await
}

/// Bounded action resource; expired drafts are never returned.
pub async fn action(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',id,'session_id',session_id,'operation_id',operation_id,'step_index',step_index,'batch_id',batch_id,'batch_index',batch_index,'batch_verdict',batch_verdict,'state',CASE WHEN state IN ('proposed','approved','batch_pending') AND expires_at<=now() THEN 'expired' WHEN batch_verdict IS NOT NULL THEN batch_verdict WHEN batch_id IS NOT NULL AND state='completed' THEN 'unknown' WHEN state='batch_pending' THEN 'proposed' ELSE state END,'expires_at',expires_at,'request_hash',encode(request_hash,'hex'),'proposal',CASE WHEN state IN ('proposed','approved','batch_pending') AND expires_at>now() THEN proposal ELSE NULL END,'http_status',http_status,'finished_at',finished_at,'review_facts',CASE WHEN state IN ('proposed','approved','batch_pending') AND expires_at>now() THEN review_facts ELSE NULL END,'evidence',evidence) AS "data!" FROM assistant_actions WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4"#,o.workspace_id,o.user_id,o.login_id,id).fetch_optional(db).await
}

/// Newest-first, indexed session history; no unbounded response.
pub async fn history(
    db: impl PgExecutor<'_>,
    o: Owner,
    session: Uuid,
    before: i64,
) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',id,'session_id',session_id,'operation_id',operation_id,'step_index',step_index,'batch_id',batch_id,'batch_index',batch_index,'batch_verdict',batch_verdict,'state',CASE WHEN state IN ('proposed','approved','batch_pending') AND expires_at<=now() THEN 'expired' WHEN batch_verdict IS NOT NULL THEN batch_verdict WHEN batch_id IS NOT NULL AND state='completed' THEN 'unknown' WHEN state='batch_pending' THEN 'proposed' ELSE state END,'expires_at',expires_at,'proposal',CASE WHEN state IN ('proposed','approved','batch_pending') AND expires_at>now() THEN proposal ELSE NULL END,'http_status',http_status,'finished_at',finished_at,'review_facts',CASE WHEN state IN ('proposed','approved','batch_pending') AND expires_at>now() THEN review_facts ELSE NULL END,'evidence',evidence) AS "data!" FROM assistant_actions WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND session_id=$4 AND step_index<$5 ORDER BY step_index DESC LIMIT 100"#,o.workspace_id,o.user_id,o.login_id,session,before).fetch_all(db).await
}

/// Signed approval consumes event once; same-event retries report existing state.
pub async fn approve(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    session: Uuid,
    event: Option<&str>,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_actions SET state=CASE WHEN state IN ('proposed','approved','batch_pending') THEN 'approved' ELSE state END,approved_at=coalesce(approved_at,now()),approval_event_sha256=coalesce(approval_event_sha256,$6::text) WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND session_id=$5 AND batch_id IS NULL AND ((state IN ('proposed','approved','batch_pending') AND expires_at>now() AND (approval_event_sha256 IS NULL OR $6::text IS NULL OR approval_event_sha256=$6::text)) OR ($6::text IS NOT NULL AND approval_event_sha256=$6::text))",o.workspace_id,o.user_id,o.login_id,id,session,event).execute(db).await?.rows_affected())
}

/// Cancellation wins only before dispatch; erase business draft immediately.
pub async fn cancel(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    session: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_actions SET state='cancelled',proposal=NULL,review_facts='{}'::jsonb,finished_at=now() WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND session_id=$5 AND batch_id IS NULL AND state IN ('proposed','approved','cancelled')",o.workspace_id,o.user_id,o.login_id,id,session).execute(db).await?.rows_affected())
}

/// Commit this CAS before invoking business handler; never reclaim or replay dispatch.
pub async fn claim(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    session: Uuid,
    operation: &str,
    hash: &[u8],
    needs_facts: bool,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"UPDATE assistant_actions SET state='dispatching',proposal=NULL,dispatched_at=now() WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND session_id=$5 AND operation_id=$6 AND request_hash=$7 AND batch_id IS NULL AND state='approved' AND expires_at>clock_timestamp() AND (NOT $8 OR (review_facts->>'version'='1' AND review_facts->>'operation_id'=$6 AND review_facts->'facts'->>'kind' IS NOT NULL AND (coalesce(review_facts->'facts'->'locked','null'::jsonb)<>'null'::jsonb OR coalesce(review_facts->'facts'->'priced','null'::jsonb)<>'null'::jsonb))) AND EXISTS(SELECT 1 FROM assistant_sessions s WHERE s.workspace_id=$1 AND s.id=$5 AND s.user_id=$2 AND s.login_id=$3 AND s.state='active' AND s.expires_at>now() AND s.last_seen_at>now()-interval '15 minutes') AND EXISTS(SELECT 1 FROM access_tokens t WHERE t.id=$3 AND t.user_id=$2 AND t.expires_at>clock_timestamp()) RETURNING review_facts AS "facts!""#,o.workspace_id,o.user_id,o.login_id,id,session,operation,hash,needs_facts).fetch_optional(db).await
}

/// Record only bounded projected evidence; failure leaves consumed claim unresolved.
pub async fn finish(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    status: i32,
    evidence: &Value,
) -> Result<(), sqlx::Error> {
    let (next, resolved) = projected_outcome(status, evidence);
    sqlx::query!("UPDATE assistant_actions a SET state=CASE WHEN (EXISTS(SELECT 1 FROM invoice_deliveries d WHERE d.workspace_id=$1 AND d.action_id=a.id) OR (a.operation_id='orders_refund' AND a.evidence->'refund'->>'refund_id' IS NOT NULL) OR (a.finished_at IS NOT NULL AND a.state IN ('queued','partial','failed'))) THEN a.state ELSE $7 END,http_status=$5,evidence=CASE WHEN (EXISTS(SELECT 1 FROM invoice_deliveries d WHERE d.workspace_id=$1 AND d.action_id=a.id) OR (a.operation_id='orders_refund' AND a.evidence->'refund'->>'refund_id' IS NOT NULL) OR (a.finished_at IS NOT NULL AND a.state IN ('queued','partial','failed'))) THEN $6::jsonb || a.evidence ELSE a.evidence || $6::jsonb END,review_facts='{}'::jsonb,finished_at=CASE WHEN (EXISTS(SELECT 1 FROM invoice_deliveries d WHERE d.workspace_id=$1 AND d.action_id=a.id) OR (a.operation_id='orders_refund' AND a.evidence->'refund'->>'refund_id' IS NOT NULL) OR (a.finished_at IS NOT NULL AND a.state IN ('queued','partial','failed'))) THEN a.finished_at WHEN $8 THEN now() ELSE NULL END WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND state IN ('dispatching','unknown','queued','partial','failed')",o.workspace_id,o.user_id,o.login_id,id,status,evidence,next,resolved).execute(db).await?;
    Ok(())
}

/// Link admitted refund and action in the business transaction before transport.
pub async fn queue_refund(
    db: &mut PgConnection,
    workspace: Uuid,
    user: Uuid,
    action: Uuid,
    refund: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_actions a SET state='queued',evidence=evidence || jsonb_build_object('refund',jsonb_build_object('refund_id',$4::uuid,'status','creating')),proposal=NULL,review_facts='{}'::jsonb,finished_at=NULL WHERE a.workspace_id=$1 AND a.user_id=$2 AND a.id=$3 AND a.operation_id='orders_refund' AND a.state='dispatching' AND EXISTS(SELECT 1 FROM order_payments p WHERE p.workspace_id=$1 AND p.id=$4 AND p.kind='refund' AND p.created_by=$2)",workspace,user,action,refund).execute(db).await?.rows_affected())
}

/// Resolve only correlated native refund evidence; paused batches never advance.
async fn reconcile_refunds(db: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query!(r#"WITH proven AS (
        SELECT a.workspace_id,a.id,p.status FROM assistant_actions a
        JOIN order_payments p ON p.workspace_id=a.workspace_id AND p.id=CASE WHEN a.evidence->'refund'->>'refund_id' ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN (a.evidence->'refund'->>'refund_id')::uuid END AND p.created_by=a.user_id AND p.kind='refund'
        WHERE a.operation_id='orders_refund' AND a.state IN ('queued','dispatching','unknown','failed','completed') AND p.status IN ('refunded','failed','canceled') AND a.evidence->'refund'->>'status' IS DISTINCT FROM p.status
        ORDER BY p.updated_at,a.id LIMIT 100 FOR UPDATE OF a SKIP LOCKED
    ) UPDATE assistant_actions a SET state=CASE WHEN p.status='refunded' THEN 'completed' ELSE 'failed' END,
        batch_verdict=CASE WHEN a.batch_id IS NULL THEN a.batch_verdict WHEN p.status='refunded' THEN 'completed' ELSE 'failed' END,
        evidence=jsonb_set(a.evidence,'{refund,status}',to_jsonb(p.status)),proposal=NULL,review_facts='{}'::jsonb,finished_at=now()
        FROM proven p WHERE a.workspace_id=p.workspace_id AND a.id=p.id"#).execute(db).await?;
    Ok(())
}

/// Quota counts include cancelled reservations in lifetime total.
pub async fn upload_counts(
    db: &mut PgConnection,
    workspace: Uuid,
    session: Uuid,
) -> Result<(i64, i64), sqlx::Error> {
    let r=sqlx::query!(r#"SELECT count(*) AS "total!",count(*) FILTER(WHERE state='reserved' AND created_at>now()-interval '60 seconds') AS "reserved!" FROM assistant_uploads WHERE workspace_id=$1 AND session_id=$2"#,workspace,session).fetch_one(db).await?;
    Ok((r.total, r.reserved))
}

/// Reserve after caller locks session and checks quotas.
pub async fn reserve(
    db: &mut PgConnection,
    workspace: Uuid,
    session: Uuid,
    input: &UploadInput,
) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar!(r#"INSERT INTO assistant_uploads(workspace_id,session_id,media_type,size_bytes) VALUES($1,$2,$3,$4) RETURNING jsonb_build_object('id',id,'state',state,'media_type',media_type,'size_bytes',size_bytes) AS "data!""#,workspace,session,input.media_type,input.size_bytes).fetch_one(db).await
}

/// Complete or repeat same provider binding; late completion remains cleanup-visible.
pub async fn complete_upload(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    session: Uuid,
    id: Uuid,
    input: &CompleteInput,
    cleanup_only: bool,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"UPDATE assistant_uploads SET file_id=$4,size_bytes=coalesce($5,size_bytes),media_type=coalesce($6,media_type),sha256=coalesce($7,sha256),state=CASE WHEN $8 OR state IN ('cancelled','cleanup_pending') OR created_at<=now()-interval '60 seconds' THEN 'cleanup_pending' ELSE 'uploaded' END WHERE workspace_id=$1 AND session_id=$2 AND id=$3 AND state IN ('reserved','uploaded','cancelled','cleanup_pending') AND (file_id IS NULL OR file_id=$4) RETURNING jsonb_build_object('id',id,'file_id',file_id,'state',state,'size_bytes',size_bytes,'media_type',media_type,'sha256',sha256) AS "data!""#,workspace,session,id,input.file_id,input.size_bytes,input.media_type,input.sha256,cleanup_only).fetch_optional(db).await
}

/// Owned upload references and verified metadata; no bytes or names stored.
pub async fn uploads(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    session: Uuid,
) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',id,'file_id',file_id,'state',CASE WHEN state='reserved' AND created_at<=now()-interval '60 seconds' THEN 'cancelled' ELSE state END,'size_bytes',size_bytes,'media_type',media_type,'sha256',sha256) AS "data!" FROM assistant_uploads WHERE workspace_id=$1 AND session_id=$2 ORDER BY created_at,id LIMIT 10"#,workspace,session).fetch_all(db).await
}

/// One upload lookup remains scoped to owned session and workspace.
pub async fn upload(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    session: Uuid,
    id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',id,'file_id',file_id,'state',CASE WHEN state='reserved' AND created_at<=now()-interval '60 seconds' THEN 'cancelled' ELSE state END,'size_bytes',size_bytes,'media_type',media_type,'sha256',sha256) AS "data!" FROM assistant_uploads WHERE workspace_id=$1 AND session_id=$2 AND id=$3"#,workspace,session,id).fetch_optional(db).await
}

/// Release in-flight quota while retaining remote references for cleanup.
pub async fn cancel_upload(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    session: Uuid,
    id: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_uploads SET state=CASE WHEN file_id IS NULL THEN 'cancelled' ELSE 'cleanup_pending' END WHERE workspace_id=$1 AND session_id=$2 AND id=$3 AND state<>'cleaned'",workspace,session,id).execute(db).await?.rows_affected())
}

/// Minute sweep expires transient payloads and marks abandoned claims unknown.
pub async fn expire(db: &PgPool) -> Result<(), sqlx::Error> {
    reconcile_refunds(db).await?;
    super::batch_repo::expire(db).await?;
    sqlx::query!("UPDATE assistant_uploads SET state='cancelled' WHERE state='reserved' AND created_at<=now()-interval '60 seconds'").execute(db).await?;
    sqlx::query!("UPDATE assistant_actions SET state='expired',proposal=NULL,review_facts='{}'::jsonb,finished_at=now() WHERE state IN ('proposed','approved','batch_pending') AND expires_at<=now()").execute(db).await?;
    sqlx::query!("UPDATE assistant_actions SET state='unknown',proposal=NULL,review_facts='{}'::jsonb WHERE state='dispatching' AND dispatched_at<now()-interval '10 minutes'").execute(db).await?;
    Ok(())
}

/// Hourly resolved audit and context retention; unresolved work remains.
pub async fn prune(db: &PgPool) -> Result<u64, sqlx::Error> {
    expire(db).await?;
    let removed=sqlx::query!("DELETE FROM assistant_actions WHERE state IN ('completed','cancelled','expired','failed','queued','partial') AND finished_at<now()-interval '90 days' AND batch_verdict IS DISTINCT FROM 'unknown' AND (batch_id IS NULL OR state<>'completed' OR batch_verdict='completed')").execute(db).await?.rows_affected();
    super::batch_repo::prune(db).await?;
    crate::modules::invoices::delivery_repo::prune(db).await?;
    super::retention::prune(db).await?;
    super::gaps_repo::prune(db).await?;
    Ok(removed)
}

/// Global internal cleanup inventory: orphaned, closed, expired or revoked-login sessions.
pub async fn cleanup_due(db: &PgPool) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('session_id',s.id,'workspace_id',s.workspace_id,'conversation_id',s.conversation_id,'file_ids',ARRAY(SELECT u.file_id FROM assistant_uploads u WHERE u.workspace_id=s.workspace_id AND u.session_id=s.id AND u.file_id IS NOT NULL AND u.state<>'cleaned')) AS "data!" FROM assistant_sessions s WHERE s.state<>'cleaned' AND NOT EXISTS(SELECT 1 FROM assistant_uploads u WHERE u.workspace_id=s.workspace_id AND u.session_id=s.id AND u.state='reserved' AND u.created_at>now()-interval '60 seconds') AND (s.state='closed' OR s.expires_at<=now() OR s.last_seen_at<=now()-interval '15 minutes' OR NOT EXISTS(SELECT 1 FROM access_tokens t WHERE t.id=s.login_id AND t.expires_at>now())) ORDER BY s.last_seen_at LIMIT 100"#).fetch_all(db).await
}

/// Ack after provider confirms deletion; clear references and expire pending drafts atomically.
pub async fn cleanup_ack(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    let row=sqlx::query!("UPDATE assistant_sessions SET state='cleaned',cleaned_at=now(),conversation_id=NULL WHERE id=$1 AND state<>'cleaned' AND NOT EXISTS(SELECT 1 FROM assistant_uploads u WHERE u.workspace_id=assistant_sessions.workspace_id AND u.session_id=assistant_sessions.id AND u.state='reserved' AND u.created_at>now()-interval '60 seconds') AND (state='closed' OR expires_at<=now() OR last_seen_at<=now()-interval '15 minutes' OR NOT EXISTS(SELECT 1 FROM access_tokens t WHERE t.id=login_id AND t.expires_at>now())) RETURNING workspace_id",id).fetch_optional(&mut *tx).await?;
    let updated = row.is_some();
    if let Some(row) = row {
        sqlx::query!("UPDATE assistant_uploads SET state='cleaned',file_id=NULL WHERE workspace_id=$1 AND session_id=$2",row.workspace_id,id).execute(&mut *tx).await?;
        sqlx::query!("UPDATE assistant_actions SET state='expired',proposal=NULL,review_facts='{}'::jsonb,finished_at=now() WHERE workspace_id=$1 AND session_id=$2 AND state IN ('proposed','approved','batch_pending')",row.workspace_id,id).execute(&mut *tx).await?;
    }
    let confirmed=updated || sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM assistant_sessions WHERE id=$1 AND state='cleaned') AS "cleaned!""#,id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(confirmed)
}

fn projected_outcome(status: i32, evidence: &Value) -> (&'static str, bool) {
    if evidence["refusal"] == "review_stale" {
        return ("failed", true);
    }
    match evidence["refund"]["status"].as_str() {
        Some("creating" | "pending") => return ("queued", false),
        Some(_) => return ("unknown", false),
        None => {}
    }
    match evidence["permission_changes"]["status"].as_str() {
        Some("partial") => return ("partial", true),
        Some("applied") => return ("completed", true),
        Some("unknown") => return ("unknown", false),
        _ => {}
    }
    match evidence["generation"]["status"].as_str() {
        Some("partial") => return ("partial", true),
        Some("applied") => return ("completed", true),
        Some("unknown") => return ("unknown", false),
        _ => {}
    }
    match evidence["delivery"]["status"].as_str() {
        Some("accepted") => ("queued", true),
        Some("blocked" | "no_recipients") => ("failed", true),
        Some("manual") => ("partial", true),
        Some("queued") => ("queued", false),
        _ => {
            if (200..300).contains(&status) {
                ("completed", true)
            } else {
                ("unknown", false)
            }
        }
    }
}

/// One bounded lookup for every ordered member; callers project read permissions once.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn batch_actions(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',id,'session_id',session_id,'operation_id',operation_id,'step_index',step_index,'batch_id',batch_id,'batch_index',batch_index,'batch_verdict',batch_verdict,'state',CASE WHEN state IN ('proposed','approved','batch_pending') AND expires_at<=now() THEN 'expired' WHEN batch_verdict IS NOT NULL THEN batch_verdict WHEN batch_id IS NOT NULL AND state='completed' THEN 'unknown' WHEN state='batch_pending' THEN 'proposed' ELSE state END,'expires_at',expires_at,'request_hash',encode(request_hash,'hex'),'proposal',CASE WHEN state IN ('proposed','approved','batch_pending') AND expires_at>now() THEN proposal ELSE NULL END,'http_status',http_status,'finished_at',finished_at,'review_facts',CASE WHEN state IN ('proposed','approved','batch_pending') AND expires_at>now() THEN review_facts ELSE NULL END,'evidence',evidence) AS "data!" FROM assistant_actions WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND batch_id=$4 ORDER BY batch_index LIMIT 10"#,o.workspace_id,o.user_id,o.login_id,id).fetch_all(db).await
}

#[cfg(test)]
mod receipt_projection_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn truthful_mail_receipts_are_not_inbox_delivery() {
        for (status, expected, resolved) in [
            ("accepted", "queued", true),
            ("blocked", "failed", true),
            ("no_recipients", "failed", true),
            ("manual", "partial", true),
            ("queued", "queued", false),
        ] {
            assert_eq!(
                projected_outcome(200, &json!({"delivery":{"status":status}})),
                (expected, resolved),
                "{status}"
            );
        }
        assert_eq!(
            projected_outcome(409, &json!({"refusal":"review_stale"})),
            ("failed", true)
        );
        assert_eq!(projected_outcome(422, &json!({})), ("unknown", false));
    }
    #[test]
    fn partial_team_permission_changes_never_claim_completion() {
        assert_eq!(
            projected_outcome(
                200,
                &json!({"permission_changes":{"status":"partial","applied_count":1,"not_applied_count":1,"ignored_count":0}})
            ),
            ("partial", true)
        );
        assert_eq!(
            projected_outcome(
                201,
                &json!({"permission_changes":{"status":"applied","applied_count":2,"not_applied_count":0,"ignored_count":0}})
            ),
            ("completed", true)
        );
    }

    #[test]
    fn generation_receipts_are_partial_or_unknown_until_proven_complete() {
        for (receipt, expected) in [
            (
                json!({"status":"partial","requested":3,"created":2,"skipped":1}),
                ("partial", true),
            ),
            (
                json!({"status":"applied","requested":2,"created":2,"skipped":0}),
                ("completed", true),
            ),
            (json!({"status":"unknown"}), ("unknown", false)),
        ] {
            assert_eq!(
                projected_outcome(201, &json!({"generation":receipt})),
                expected
            );
        }
    }

    #[test]
    fn refund_receipts_remain_pending_until_native_settlement_is_verified() {
        assert_eq!(
            projected_outcome(201, &json!({"refund":{"status":"creating"}})),
            ("queued", false)
        );
        assert_eq!(
            projected_outcome(201, &json!({"refund":{"status":"pending"}})),
            ("queued", false)
        );
        assert_eq!(
            projected_outcome(201, &json!({"refund":{"status":"unknown"}})),
            ("unknown", false)
        );
    }

    #[sqlx::test]
    async fn durable_mail_acceptance_survives_late_http_failure(db: PgPool) {
        let user:Uuid=sqlx::query_scalar("INSERT INTO users(email,first_name,last_name,password_hash) VALUES('receipt@example.test','A','B','h') RETURNING id").fetch_one(&db).await.unwrap();
        let workspace: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces(name,created_by_user_id) VALUES('receipt', $1) RETURNING id",
        )
        .bind(user)
        .fetch_one(&db)
        .await
        .unwrap();
        let owner = Owner {
            workspace_id: workspace,
            user_id: user,
            login_id: Uuid::now_v7(),
        };
        let session = create_session(&db, owner, "en").await.unwrap();
        let id:Uuid=sqlx::query_scalar(r#"INSERT INTO assistant_actions(workspace_id,user_id,login_id,session_id,operation_id,step_index,request_hash,state,finished_at,evidence) VALUES($1,$2,$3,$4,'messages_send',1,decode(repeat('00',32),'hex'),'queued',now(),'{"delivery":{"status":"accepted"}}') RETURNING id"#).bind(workspace).bind(user).bind(owner.login_id).bind(session).fetch_one(&db).await.unwrap();
        finish(&db, owner, id, 503, &json!({"meaning":"outcome_unknown"}))
            .await
            .unwrap();
        let row = action(&db, owner, id).await.unwrap().unwrap();
        assert_eq!(row["state"], "queued");
        assert_eq!(row["evidence"]["delivery"]["status"], "accepted");
        assert!(row["finished_at"].is_string());
    }
}
