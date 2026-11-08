//! Owner-filtered fixed batch SQL; no HTTP request replay or background executor.

use super::repo::Owner;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

/// Durable batch metadata; private run hash is never part of a resource.
pub struct Batch {
    pub id: Uuid,
    pub session_id: Uuid,
    pub fingerprint: Vec<u8>,
    pub action_count: i16,
    pub next_index: i16,
    pub state: String,
    pub reason: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub expired: bool,
    pub lease_expired: bool,
}
/// Exact private binding used once to seal reviewed membership.
pub struct Binding {
    pub id: Uuid,
    pub operation_id: String,
    pub request_hash: Vec<u8>,
    pub review_facts: Value,
    pub proposal: Option<Value>,
}
/// Current ledger step under its batch lock.
pub struct Step {
    pub id: Uuid,
    pub state: String,
    pub batch_index: Option<i16>,
    pub batch_verdict: Option<String>,
}

/// One owner-bound batch lookup, optionally locked by caller's transaction.
pub async fn get(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
) -> Result<Option<Batch>, sqlx::Error> {
    sqlx::query_as!(Batch,r#"SELECT id,session_id,fingerprint,action_count,next_index,state,reason,expires_at,started_at,(expires_at<=clock_timestamp()) AS "expired!",coalesce(run_expires_at<=clock_timestamp(),false) AS "lease_expired!" FROM assistant_batches WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4"#,o.workspace_id,o.user_id,o.login_id,id).fetch_optional(db).await
}
/// Lock ordering: session before batch when both are needed; then child action.
pub async fn lock(
    conn: &mut PgConnection,
    o: Owner,
    id: Uuid,
) -> Result<Option<Batch>, sqlx::Error> {
    sqlx::query_as!(Batch,r#"SELECT id,session_id,fingerprint,action_count,next_index,state,reason,expires_at,started_at,(expires_at<=clock_timestamp()) AS "expired!",coalesce(run_expires_at<=clock_timestamp(),false) AS "lease_expired!" FROM assistant_batches WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 FOR UPDATE"#,o.workspace_id,o.user_id,o.login_id,id).fetch_optional(conn).await
}
/// Proposed/approved or live running controllers block another pending review.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn active(db: impl PgExecutor<'_>, o: Owner, session: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM assistant_batches WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND session_id=$4 AND expires_at>clock_timestamp() AND (state IN ('proposed','approved') OR (state='running' AND run_expires_at>clock_timestamp()))) AS "active!""#,o.workspace_id,o.user_id,o.login_id,session).fetch_one(db).await
}
/// Same session lock also serializes single versus batch spoken consent.
pub async fn event_used(
    db: impl PgExecutor<'_>,
    o: Owner,
    session: Uuid,
    event: &str,
    except: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM assistant_batches WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND session_id=$4 AND approval_event_sha256=$5 AND id IS DISTINCT FROM $6) AS "used!""#,o.workspace_id,o.user_id,o.login_id,session,event,except).fetch_one(db).await
}
/// Existing single proofs remain authoritative after migration; no proof data copied.
pub async fn single_event_used(
    db: impl PgExecutor<'_>,
    o: Owner,
    session: Uuid,
    event: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM assistant_actions WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND session_id=$4 AND approval_event_sha256=$5) AS "used!""#,o.workspace_id,o.user_id,o.login_id,session,event).fetch_one(db).await
}
/// Insert only inside atomic proposal transaction.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn insert(
    db: impl PgExecutor<'_>,
    o: Owner,
    session: Uuid,
    count: i16,
    session_expiry: DateTime<Utc>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!("INSERT INTO assistant_batches(workspace_id,user_id,login_id,session_id,action_count,fingerprint,expires_at) VALUES($1,$2,$3,$4,$5,$6,least($7,now()+interval '120 seconds')) RETURNING id",o.workspace_id,o.user_id,o.login_id,session,count,&[0u8;32][..],session_expiry).fetch_one(db).await
}
/// Membership/order never changes after proposal commit.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn link(
    db: impl PgExecutor<'_>,
    o: Owner,
    batch: Uuid,
    action: Uuid,
    index: i16,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_actions SET batch_id=$4,batch_index=$6,state='batch_pending',expires_at=(SELECT expires_at FROM assistant_batches WHERE workspace_id=$1 AND id=$4) WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$5 AND state='proposed' AND batch_id IS NULL",o.workspace_id,o.user_id,o.login_id,batch,action,index).execute(db).await?.rows_affected())
}
/// Bounded private binding snapshot; never returned before permission projection.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn bindings(
    db: impl PgExecutor<'_>,
    o: Owner,
    batch: Uuid,
) -> Result<Vec<Binding>, sqlx::Error> {
    sqlx::query_as!(Binding,"SELECT id,operation_id,request_hash,review_facts,proposal FROM assistant_actions WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND batch_id=$4 ORDER BY batch_index LIMIT 10",o.workspace_id,o.user_id,o.login_id,batch).fetch_all(db).await
}
/// Seal actor/login, exact requests/material facts and immutable ordering.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn seal(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    hash: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_batches SET fingerprint=$5 WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND state='proposed'",o.workspace_id,o.user_id,o.login_id,id,hash).execute(db).await?;
    Ok(())
}
/// Unexpired standalone review cannot be silently absorbed into a batch.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn standalone_pending(
    db: impl PgExecutor<'_>,
    o: Owner,
    session: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM assistant_actions WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND session_id=$4 AND batch_id IS NULL AND state IN ('proposed','approved') AND expires_at>clock_timestamp()) AS "pending!""#,o.workspace_id,o.user_id,o.login_id,session).fetch_one(db).await
}
/// Atomic consent leaves children batch_pending, unreadable by old dispatch code.
pub async fn approve(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    event: Option<&str>,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_batches SET state='approved',approved_at=coalesce(approved_at,now()),approval_event_sha256=coalesce(approval_event_sha256,$5) WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND state IN ('proposed','approved') AND expires_at>clock_timestamp() AND (approval_event_sha256 IS NULL OR $5::text IS NULL OR approval_event_sha256=$5)",o.workspace_id,o.user_id,o.login_id,id,event).execute(db).await?.rows_affected())
}
/// One controller token can start once; plaintext is returned once and never persisted.
pub async fn start(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    hash: &[u8],
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!("UPDATE assistant_batches SET state='running',started_at=now(),run_hash=$5,run_expires_at=least(expires_at,clock_timestamp()+interval '45 seconds') WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND state='approved' AND started_at IS NULL AND expires_at>clock_timestamp()",o.workspace_id,o.user_id,o.login_id,id,hash).execute(db).await?.rows_affected()==1)
}
/// Hashed token lookup only; no plaintext comparison or logging.
pub async fn controller(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    hash: &[u8],
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM assistant_batches WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND run_hash=$5) AS "valid!""#,o.workspace_id,o.user_id,o.login_id,id,hash).fetch_one(db).await
}
/// Child state is checked under batch lock before acknowledging completion.
pub async fn step(
    db: impl PgExecutor<'_>,
    o: Owner,
    batch: Uuid,
    id: Uuid,
) -> Result<Option<Step>, sqlx::Error> {
    sqlx::query_as!(Step,"SELECT id,state,batch_index,batch_verdict FROM assistant_actions WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND batch_id=$4 AND id=$5",o.workspace_id,o.user_id,o.login_id,batch,id).fetch_optional(db).await
}
/// Only a verified completed result advances; pause/expiry never becomes running again.
pub async fn advance(
    conn: &mut PgConnection,
    o: Owner,
    batch: Uuid,
    id: Uuid,
    index: i16,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_actions SET batch_verdict='completed' WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND batch_id=$5 AND state='completed'",o.workspace_id,o.user_id,o.login_id,id,batch).execute(&mut *conn).await?;
    sqlx::query!("UPDATE assistant_batches SET next_index=$5+1,state=CASE WHEN $5+1=action_count THEN 'completed' WHEN state='running' AND expires_at<=clock_timestamp() THEN 'expired' WHEN state='running' AND run_expires_at<=clock_timestamp() THEN 'paused' ELSE state END,reason=CASE WHEN $5+1=action_count THEN NULL WHEN state='running' AND expires_at<=clock_timestamp() THEN 'expired' WHEN state='running' AND run_expires_at<=clock_timestamp() THEN 'interrupted' ELSE reason END,finished_at=CASE WHEN $5+1=action_count OR state IN ('paused','cancelled','expired') OR expires_at<=clock_timestamp() OR run_expires_at<=clock_timestamp() THEN coalesce(finished_at,now()) ELSE NULL END,run_expires_at=CASE WHEN state='running' AND expires_at>clock_timestamp() AND run_expires_at>clock_timestamp() THEN least(expires_at,clock_timestamp()+interval '45 seconds') ELSE run_expires_at END WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4 AND next_index=$5",o.workspace_id,o.user_id,o.login_id,batch,index).execute(conn).await?;
    Ok(())
}
/// Stop future admissions; current unverified HTTP completion stays conservative.
pub async fn stop(
    conn: &mut PgConnection,
    o: Owner,
    id: Uuid,
    reason: &str,
    cancel: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_actions a SET batch_verdict=CASE WHEN a.state IN ('queued','partial','failed') THEN a.state ELSE 'unknown' END FROM assistant_batches b WHERE a.workspace_id=$1 AND a.user_id=$2 AND a.login_id=$3 AND a.batch_id=$4 AND b.workspace_id=a.workspace_id AND b.id=a.batch_id AND a.batch_index=b.next_index AND a.state IN ('dispatching','completed','unknown','queued','partial','failed') AND a.batch_verdict IS NULL",o.workspace_id,o.user_id,o.login_id,id).execute(&mut *conn).await?;
    if cancel {
        sqlx::query!("UPDATE assistant_actions SET state='cancelled',batch_verdict='cancelled',proposal=NULL,review_facts='{}'::jsonb,finished_at=now() WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND batch_id=$4 AND state='batch_pending'",o.workspace_id,o.user_id,o.login_id,id).execute(&mut *conn).await?;
    } else {
        sqlx::query!("UPDATE assistant_actions SET proposal=NULL,review_facts='{}'::jsonb WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND batch_id=$4 AND state='batch_pending'",o.workspace_id,o.user_id,o.login_id,id).execute(&mut *conn).await?;
    }
    sqlx::query!("UPDATE assistant_batches SET state=CASE WHEN state IN ('completed','cancelled','expired') THEN state WHEN $5 THEN 'cancelled' ELSE 'paused' END,reason=CASE WHEN state='completed' THEN NULL WHEN state IN ('cancelled','expired') THEN reason ELSE $6 END,finished_at=coalesce(finished_at,now()) WHERE workspace_id=$1 AND user_id=$2 AND login_id=$3 AND id=$4",o.workspace_id,o.user_id,o.login_id,id,cancel,reason).execute(conn).await?;
    Ok(())
}
/// Bounded history summaries; no private snapshots or per-row network lookups.
pub async fn summaries(
    db: impl PgExecutor<'_>,
    o: Owner,
    session: Uuid,
) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',b.id,'session_id',b.session_id,'protocol_version',b.protocol_version,'fingerprint',encode(b.fingerprint,'hex'),'state',CASE WHEN b.state IN ('proposed','approved','running') AND b.expires_at<=clock_timestamp() THEN 'expired' WHEN b.state='running' AND b.run_expires_at<=clock_timestamp() THEN 'paused' ELSE b.state END,'reason',CASE WHEN b.expires_at<=clock_timestamp() AND b.state IN ('proposed','approved','running') THEN 'expired' WHEN b.state='running' AND b.run_expires_at<=clock_timestamp() THEN 'interrupted' ELSE b.reason END,'next_index',b.next_index,'action_count',b.action_count,'expires_at',b.expires_at,'started_at',b.started_at,'recoverable',b.state IN ('proposed','approved') AND b.started_at IS NULL AND b.expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM assistant_actions a WHERE a.workspace_id=b.workspace_id AND a.batch_id=b.id AND (a.proposal IS NULL OR coalesce(a.proposal->>'content_type','') LIKE 'multipart/%')),'steps','[]'::jsonb) AS "batch!" FROM assistant_batches b WHERE b.workspace_id=$1 AND b.user_id=$2 AND b.login_id=$3 AND b.session_id=$4 ORDER BY b.created_at DESC,b.id DESC LIMIT 50"#,o.workspace_id,o.user_id,o.login_id,session).fetch_all(db).await
}
/// Expiry never releases a consumed controller or permits replay.
pub async fn expire(db: &PgPool) -> Result<(), sqlx::Error> {
    // Terminal typed delivery proof changes receipt meaning only; ordering never advances.
    sqlx::query!(
        "WITH proven AS (
           SELECT workspace_id,id FROM assistant_actions
           WHERE batch_id IS NOT NULL AND batch_verdict='unknown'
             AND state IN ('queued','partial','failed') AND finished_at IS NOT NULL
             AND (
               (operation_id IN ('messages_send','messages_profile_test') AND (
                 (state='queued' AND evidence#>>'{delivery,status}'='accepted') OR
                 (state='failed' AND evidence#>>'{delivery,status}' IN ('blocked','no_recipients')) OR
                 (state='partial' AND evidence#>>'{delivery,status}'='manual'))) OR
               (operation_id IN ('invoices_send','invoices_resend') AND (
                 (state='queued' AND evidence#>>'{delivery,status}'='accepted') OR
                 (state='partial' AND evidence#>>'{delivery,status}' IN ('blocked','manual','failed')))))
           ORDER BY finished_at,id LIMIT 100 FOR UPDATE SKIP LOCKED
         ) UPDATE assistant_actions a SET batch_verdict=a.state FROM proven p
           WHERE a.workspace_id=p.workspace_id AND a.id=p.id"
    ).execute(db).await?;
    sqlx::query!(
        "WITH due AS (
           SELECT workspace_id,id FROM assistant_batches
           WHERE state IN ('proposed','approved','running')
             AND (expires_at<=clock_timestamp() OR (state='running' AND run_expires_at<=clock_timestamp()))
           ORDER BY expires_at,id LIMIT 100 FOR UPDATE SKIP LOCKED
         ) UPDATE assistant_batches b SET
             state=CASE WHEN expires_at<=clock_timestamp() THEN 'expired' ELSE 'paused' END,
             reason=CASE WHEN expires_at<=clock_timestamp() THEN 'expired' ELSE 'interrupted' END,
             finished_at=coalesce(finished_at,now())
           FROM due d WHERE b.workspace_id=d.workspace_id AND b.id=d.id"
    ).execute(db).await?;
    sqlx::query!(
        "WITH stopped AS (
           SELECT a.workspace_id,a.id FROM assistant_actions a JOIN assistant_batches b
             ON b.workspace_id=a.workspace_id AND b.id=a.batch_id
           WHERE a.state='batch_pending' AND b.state IN ('paused','cancelled','expired','completed')
             AND (a.proposal IS NOT NULL OR a.review_facts<>'{}'::jsonb)
           ORDER BY b.finished_at,a.id LIMIT 1000 FOR UPDATE OF a SKIP LOCKED
         ) UPDATE assistant_actions a SET proposal=NULL,review_facts='{}'::jsonb
           FROM stopped s WHERE a.workspace_id=s.workspace_id AND a.id=s.id"
    )
    .execute(db)
    .await?;
    Ok(())
}
/// Children must leave audit first; unresolved outcomes keep batch/session linkage.
pub async fn prune(db: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("WITH expired AS (SELECT b.workspace_id,b.id FROM assistant_batches b WHERE b.state IN ('completed','cancelled','expired','paused') AND b.finished_at<now()-interval '90 days' AND NOT EXISTS(SELECT 1 FROM assistant_actions a WHERE a.workspace_id=b.workspace_id AND a.batch_id=b.id) ORDER BY b.finished_at LIMIT 1000 FOR UPDATE SKIP LOCKED) DELETE FROM assistant_batches b USING expired e WHERE b.workspace_id=e.workspace_id AND b.id=e.id").execute(db).await?.rows_affected())
}

/// Exact incoming child request plus private one-controller proof.
pub struct Claim<'a> {
    pub batch_id: Uuid,
    pub action_id: Uuid,
    pub session_id: Uuid,
    pub operation: &'a str,
    pub request_hash: &'a [u8],
    pub run_hash: [u8; 32],
    pub needs_facts: bool,
}
/// Stop/advance and dispatch serialize on batch row before child claim commits.
pub async fn claim(db: &PgPool, o: Owner, c: Claim<'_>) -> Result<Option<Value>, sqlx::Error> {
    let mut tx = db.begin().await?;
    if lock(&mut tx, o, c.batch_id).await?.is_none() {
        return Ok(None);
    }
    let result=sqlx::query_scalar!(r#"UPDATE assistant_actions a SET state='dispatching',proposal=NULL,dispatched_at=clock_timestamp() FROM assistant_batches b WHERE a.workspace_id=$1 AND a.user_id=$2 AND a.login_id=$3 AND a.id=$4 AND a.session_id=$5 AND a.operation_id=$6 AND a.request_hash=$7 AND a.state='batch_pending' AND a.expires_at>clock_timestamp() AND a.batch_id=$9 AND b.workspace_id=a.workspace_id AND b.user_id=a.user_id AND b.login_id=a.login_id AND b.session_id=a.session_id AND b.id=a.batch_id AND a.batch_index=b.next_index AND b.state='running' AND b.expires_at>clock_timestamp() AND b.run_expires_at>clock_timestamp() AND b.run_hash=$10 AND (NOT $8 OR (a.review_facts->>'version'='1' AND a.review_facts->>'operation_id'=$6 AND a.review_facts->'facts'->>'kind' IS NOT NULL AND (coalesce(a.review_facts->'facts'->'locked','null'::jsonb)<>'null'::jsonb OR coalesce(a.review_facts->'facts'->'priced','null'::jsonb)<>'null'::jsonb))) AND EXISTS(SELECT 1 FROM assistant_sessions s WHERE s.workspace_id=$1 AND s.id=$5 AND s.user_id=$2 AND s.login_id=$3 AND s.state='active' AND s.expires_at>clock_timestamp() AND s.last_seen_at>clock_timestamp()-interval '15 minutes') AND EXISTS(SELECT 1 FROM access_tokens t WHERE t.id=$3 AND t.user_id=$2 AND t.expires_at>clock_timestamp()) RETURNING a.review_facts AS "facts!""#,o.workspace_id,o.user_id,o.login_id,c.action_id,c.session_id,c.operation,c.request_hash,c.needs_facts,c.batch_id,&c.run_hash[..]).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    Ok(result)
}
