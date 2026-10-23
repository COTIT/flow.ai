//! SQL of the job rail: enqueue, claim, lease renewal, the fencing checkpoint, outcomes,
//! reaping expired leases and pruning finished jobs. Every outcome is keyed on the claim's
//! token, so a worker that lost its lease changes nothing.

use std::time::Duration;

use serde_json::{Value, json};
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

/// `last_error` keeps at most this many characters (the column's CHECK).
const ERROR_CHARS: usize = 1000;
/// Rows one prune statement deletes, so a large backlog never holds one long lock.
const PRUNE_BATCH: i64 = 10_000;
/// Tries for a claim or an enqueue that lost a race on a unique key.
const RACE_TRIES: usize = 3;

/// A job to enqueue. `NewJob::new(kind)` gives the defaults; set fields with struct update.
#[derive(Debug, Clone)]
pub struct NewJob {
    /// One of `jobs::kinds::KINDS`.
    pub kind: &'static str,
    /// None: a system job (cleanup), outside every workspace cap.
    pub workspace_id: Option<Uuid>,
    /// JSON object (≤ 64 KiB); chunked jobs keep their cursor here.
    pub payload: Value,
    /// Higher runs first.
    pub priority: i16,
    /// Consecutive failures before `failed` (1..=100).
    pub max_attempts: i32,
    /// At most one running job per key (one import per workspace).
    pub concurrency_key: Option<String>,
    /// At most one queued or running job per key within its workspace (system jobs share one
    /// scope); enqueue returns the live one.
    pub dedupe_key: Option<String>,
    /// User who asked for it.
    pub created_by: Option<Uuid>,
}

impl NewJob {
    /// Defaults: empty payload, priority 0, 5 attempts, no keys.
    pub fn new(kind: &'static str) -> Self {
        Self {
            kind,
            workspace_id: None,
            payload: json!({}),
            priority: 0,
            max_attempts: 5,
            concurrency_key: None,
            dedupe_key: None,
            created_by: None,
        }
    }
}

/// A claimed job. `token` is this claim's lease token: outcomes and checkpoints need it.
#[derive(Debug, Clone)]
pub struct Claimed {
    /// Job id.
    pub id: Uuid,
    /// Job kind (`jobs::kinds`).
    pub kind: String,
    /// Tenant; None for system jobs.
    pub workspace_id: Option<Uuid>,
    /// Payload as of this claim (the cursor the last chunk saved).
    pub payload: Value,
    /// Claims since the last finished chunk, this one included.
    pub attempts: i32,
    /// Consecutive failures allowed.
    pub max_attempts: i32,
    /// User who enqueued it.
    pub created_by: Option<Uuid>,
    /// This claim's lease token (`locked_by`).
    pub token: Uuid,
}

impl Claimed {
    /// Save the cursor and progress inside the chunk's transaction. False: the lease is gone
    /// (another worker owns the job now), so the caller must roll the chunk back.
    pub async fn checkpoint(
        &self,
        conn: impl PgExecutor<'_>,
        payload: &Value,
        done: i64,
        total: Option<i64>,
    ) -> Result<bool, sqlx::Error> {
        let n = sqlx::query!(
            "UPDATE jobs SET payload = $3, progress_done = $4,
                 progress_total = coalesce($5, progress_total)
             WHERE id = $1 AND locked_by = $2 AND status = 'running'",
            self.id,
            self.token,
            payload,
            done,
            total,
        )
        .execute(conn)
        .await?
        .rows_affected();
        Ok(n == 1)
    }
}

/// Insert a job, or return the live job of the same workspace holding its `dedupe_key`. Takes
/// a connection because the dedupe lookup must see a row a concurrent insert just committed
/// (its own statement).
pub async fn enqueue(conn: &mut PgConnection, job: &NewJob) -> Result<Uuid, sqlx::Error> {
    for _ in 0..RACE_TRIES {
        let inserted = sqlx::query_scalar!(
            "INSERT INTO jobs (workspace_id, kind, payload, priority, max_attempts,
                               concurrency_key, dedupe_key, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (coalesce(workspace_id, '00000000-0000-0000-0000-000000000000'::uuid), dedupe_key)
                 WHERE status IN ('queued', 'running') AND dedupe_key IS NOT NULL
             DO NOTHING
             RETURNING id",
            job.workspace_id,
            job.kind,
            job.payload,
            job.priority,
            job.max_attempts,
            job.concurrency_key,
            job.dedupe_key,
            job.created_by,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(id) = inserted {
            return Ok(id);
        }
        // Same expression as the unique index: scoped to the workspace, and indexable.
        let live = sqlx::query_scalar!(
            "SELECT id FROM jobs
             WHERE coalesce(workspace_id, '00000000-0000-0000-0000-000000000000'::uuid)
                   = coalesce($1, '00000000-0000-0000-0000-000000000000'::uuid)
               AND dedupe_key = $2 AND status IN ('queued', 'running')",
            job.workspace_id,
            job.dedupe_key,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(id) = live {
            return Ok(id);
        }
        // The live job finished between the two statements: insert again.
    }
    Err(sqlx::Error::Protocol(format!(
        "enqueue lost its dedupe race {RACE_TRIES} times"
    )))
}

/// Insert a workspace job unless one of the same kind and workspace is queued and hasn't
/// started yet (it will see this change too). A started one (running, or queued between
/// chunks with its cursor moved on) doesn't count: it may have passed the change. Pair it with
/// a `concurrency_key` so the new job waits for the running one. Call it inside the
/// transaction that makes the change: the advisory lock serialises concurrent callers, and the
/// share lock on the queued job keeps workers from claiming it until that change commits.
/// None: coalesced.
pub async fn enqueue_coalesced(
    conn: &mut PgConnection,
    job: &NewJob,
) -> Result<Option<Uuid>, sqlx::Error> {
    let workspace = job
        .workspace_id
        .ok_or_else(|| sqlx::Error::Protocol("enqueue_coalesced needs a workspace job".into()))?;
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended('jobs.coalesce:' || $1 || ':' || $2::uuid::text, 0))",
        job.kind,
        workspace,
    )
    .execute(&mut *conn)
    .await?;
    let pending = sqlx::query_scalar!(
        "SELECT id FROM jobs
         WHERE kind = $1 AND workspace_id = $2 AND status = 'queued' AND started_at IS NULL
         LIMIT 1
         FOR SHARE",
        job.kind,
        workspace,
    )
    .fetch_optional(&mut *conn)
    .await?;
    if pending.is_some() {
        return Ok(None);
    }
    sqlx::query_scalar!(
        "INSERT INTO jobs (workspace_id, kind, payload, priority, max_attempts,
                           concurrency_key, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
        workspace,
        job.kind,
        job.payload,
        job.priority,
        job.max_attempts,
        job.concurrency_key,
        job.created_by,
    )
    .fetch_one(&mut *conn)
    .await
    .map(Some)
}

/// Claim the next due job of `kinds`: priority, then age. Skips workspaces already running
/// `cap` jobs (soft: two claims at the same moment can pass it by one) and concurrency keys
/// a running job holds (hard: unique index). None when nothing is due.
///
/// Fast path: the 32 oldest due jobs overall, first one not blocked. Only when there were
/// candidates and none could run (a capped tenant's backlog at the head, or all locked by
/// other claims) does the fallback walk each workspace's first due job, so that backlog costs
/// one index probe instead of a scan through it. An empty queue never reaches the fallback.
pub async fn claim(
    db: &PgPool,
    kinds: &[&str],
    cap: i64,
    lease: Duration,
) -> Result<Option<Claimed>, sqlx::Error> {
    let lease = lease.as_secs_f64();
    for _ in 0..RACE_TRIES {
        let found = match claim_fast(db, kinds, cap, lease).await {
            Ok(Fast::Claimed(job)) => Ok(Some(job)),
            Ok(Fast::Empty) => Ok(None),
            Ok(Fast::Blocked) => claim_fallback(db, kinds, cap, lease).await,
            Err(e) => Err(e),
        };
        match found {
            Ok(job) => return Ok(job),
            // Another worker took the same concurrency key a moment ago: look again.
            Err(e) if is_unique_violation(&e) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

/// What the fast path found.
enum Fast {
    Claimed(Claimed),
    /// Nothing due.
    Empty,
    /// Due jobs, all blocked (capped workspace, held key) or locked by other claims.
    Blocked,
}

async fn claim_fast(
    db: &PgPool,
    kinds: &[&str],
    cap: i64,
    lease: f64,
) -> Result<Fast, sqlx::Error> {
    let r = sqlx::query!(
        r#"WITH busy AS (
               SELECT workspace_id FROM jobs
               WHERE status = 'running' AND workspace_id IS NOT NULL
               GROUP BY workspace_id HAVING count(*) >= $2
           ),
           held AS (
               SELECT concurrency_key FROM jobs
               WHERE status = 'running' AND concurrency_key IS NOT NULL
           ),
           head AS (
               SELECT id, workspace_id, concurrency_key FROM jobs
               WHERE status = 'queued' AND run_at <= now() AND kind = ANY($1)
               ORDER BY priority DESC, run_at, id
               LIMIT 32
           ),
           candidates AS (
               SELECT h.id FROM head h
               WHERE (h.workspace_id IS NULL OR h.workspace_id NOT IN (SELECT workspace_id FROM busy))
                 AND (h.concurrency_key IS NULL
                      OR h.concurrency_key NOT IN (SELECT concurrency_key FROM held))
           ),
           next AS (
               -- Re-checked on the locked version: another claim may have taken or delayed it.
               SELECT id FROM jobs
               WHERE id IN (SELECT id FROM candidates)
                 AND status = 'queued' AND run_at <= now() AND kind = ANY($1)
               ORDER BY priority DESC, run_at, id
               LIMIT 1
               FOR UPDATE SKIP LOCKED
           ),
           claimed AS (
               UPDATE jobs j
               SET status = 'running', attempts = j.attempts + 1, locked_by = uuidv7(),
                   locked_until = now() + make_interval(secs => $3),
                   started_at = coalesce(j.started_at, now())
               FROM next WHERE j.id = next.id
               RETURNING j.id, j.kind, j.workspace_id, j.payload, j.attempts, j.max_attempts,
                         j.created_by, j.locked_by
           )
           SELECT (SELECT count(*) FROM head) AS "seen!",
                  c.id AS "id?", c.kind AS "kind?", c.workspace_id, c.payload AS "payload?",
                  c.attempts AS "attempts?", c.max_attempts AS "max_attempts?", c.created_by,
                  c.locked_by AS "token?"
           FROM (SELECT 1) AS one LEFT JOIN claimed c ON true"#,
        kinds as &[&str],
        cap,
        lease,
    )
    .fetch_one(db)
    .await?;
    Ok(
        match (r.id, r.kind, r.payload, r.attempts, r.max_attempts, r.token) {
            (
                Some(id),
                Some(kind),
                Some(payload),
                Some(attempts),
                Some(max_attempts),
                Some(token),
            ) => Fast::Claimed(Claimed {
                id,
                kind,
                workspace_id: r.workspace_id,
                payload,
                attempts,
                max_attempts,
                created_by: r.created_by,
                token,
            }),
            _ if r.seen == 0 => Fast::Empty,
            _ => Fast::Blocked,
        },
    )
}

async fn claim_fallback(
    db: &PgPool,
    kinds: &[&str],
    cap: i64,
    lease: f64,
) -> Result<Option<Claimed>, sqlx::Error> {
    let row = sqlx::query!(
        r#"WITH RECURSIVE busy AS (
               SELECT workspace_id FROM jobs
               WHERE status = 'running' AND workspace_id IS NOT NULL
               GROUP BY workspace_id HAVING count(*) >= $2
           ),
           held AS (
               SELECT concurrency_key FROM jobs
               WHERE status = 'running' AND concurrency_key IS NOT NULL
           ),
           -- The first due job of each workspace, walking `jobs_queued_idx` one workspace at
           -- a time. The ORDER BY leads with workspace_id so only that index can serve it,
           -- whatever the planner's statistics say.
           heads AS (
               (SELECT workspace_id, id FROM jobs
                WHERE status = 'queued' AND workspace_id IS NOT NULL
                  AND run_at <= now() AND kind = ANY($1)
                  AND (concurrency_key IS NULL
                       OR concurrency_key NOT IN (SELECT concurrency_key FROM held))
                ORDER BY workspace_id, priority DESC, run_at, id
                LIMIT 1)
               UNION ALL
               SELECT n.workspace_id, n.id FROM heads h
               CROSS JOIN LATERAL (
                   SELECT workspace_id, id FROM jobs
                   WHERE status = 'queued' AND workspace_id > h.workspace_id
                     AND run_at <= now() AND kind = ANY($1)
                     AND (concurrency_key IS NULL
                          OR concurrency_key NOT IN (SELECT concurrency_key FROM held))
                   ORDER BY workspace_id, priority DESC, run_at, id
                   LIMIT 1
               ) n
           ),
           candidates AS (
               SELECT id FROM heads
               WHERE workspace_id NOT IN (SELECT workspace_id FROM busy)
               UNION ALL
               (SELECT id FROM jobs
                WHERE status = 'queued' AND workspace_id IS NULL
                  AND run_at <= now() AND kind = ANY($1)
                  AND (concurrency_key IS NULL
                       OR concurrency_key NOT IN (SELECT concurrency_key FROM held))
                ORDER BY workspace_id, priority DESC, run_at, id
                LIMIT 1)
           ),
           next AS (
               -- Re-checked on the locked version: another claim may have taken or delayed it.
               SELECT id FROM jobs
               WHERE id IN (SELECT id FROM candidates)
                 AND status = 'queued' AND run_at <= now() AND kind = ANY($1)
               ORDER BY priority DESC, run_at, id
               LIMIT 1
               FOR UPDATE SKIP LOCKED
           )
           UPDATE jobs j
           SET status = 'running', attempts = j.attempts + 1, locked_by = uuidv7(),
               locked_until = now() + make_interval(secs => $3),
               started_at = coalesce(j.started_at, now())
           FROM next WHERE j.id = next.id
           RETURNING j.id, j.kind, j.workspace_id, j.payload, j.attempts, j.max_attempts,
                     j.created_by, j.locked_by AS "token!""#,
        kinds as &[&str],
        cap,
        lease,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| Claimed {
        id: r.id,
        kind: r.kind,
        workspace_id: r.workspace_id,
        payload: r.payload,
        attempts: r.attempts,
        max_attempts: r.max_attempts,
        created_by: r.created_by,
        token: r.token,
    }))
}

/// Extend a held lease. False: the lease is gone.
pub async fn renew(
    db: &PgPool,
    id: Uuid,
    token: Uuid,
    lease: Duration,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE jobs SET locked_until = now() + make_interval(secs => $3)
         WHERE id = $1 AND locked_by = $2 AND status = 'running'",
        id,
        token,
        lease.as_secs_f64(),
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// The job is done.
pub async fn finish(db: &PgPool, id: Uuid, token: Uuid) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE jobs SET status = 'succeeded', finished_at = now(), attempts = 0,
             locked_by = NULL, locked_until = NULL
         WHERE id = $1 AND locked_by = $2 AND status = 'running'",
        id,
        token,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// A chunk is done and more remain: back of the line, attempts reset (they count failures).
pub async fn requeue(db: &PgPool, id: Uuid, token: Uuid) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE jobs SET status = 'queued', run_at = now(), attempts = 0,
             locked_by = NULL, locked_until = NULL
         WHERE id = $1 AND locked_by = $2 AND status = 'running'",
        id,
        token,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// A failure worth retrying: queued again after a jittered backoff (10 s doubling, 1 h max),
/// or failed when the attempts are used up.
pub async fn retry(db: &PgPool, id: Uuid, token: Uuid, error: &str) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE jobs SET
             status = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'queued' END,
             finished_at = CASE WHEN attempts >= max_attempts THEN now() END,
             run_at = now() + make_interval(secs =>
                 least(10 * power(2, attempts - 1), 3600) * (0.8 + random() * 0.4)),
             last_error = $3, locked_by = NULL, locked_until = NULL
         WHERE id = $1 AND locked_by = $2 AND status = 'running'",
        id,
        token,
        cap(error),
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// A failure retrying can't fix (bad payload, deleted target): failed at once.
pub async fn fail(db: &PgPool, id: Uuid, token: Uuid, error: &str) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "UPDATE jobs SET status = 'failed', finished_at = now(), last_error = $3,
             locked_by = NULL, locked_until = NULL
         WHERE id = $1 AND locked_by = $2 AND status = 'running'",
        id,
        token,
        cap(error),
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Running jobs whose lease ran out (worker died or stalled): retried with backoff like
/// `retry`, or failed when the attempts are used up. Skips rows another transaction holds
/// (a late chunk commit), so the scheduler tick never waits. Returns how many it touched.
pub async fn reap(db: impl PgExecutor<'_>) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE jobs SET
             status = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'queued' END,
             finished_at = CASE WHEN attempts >= max_attempts THEN now() END,
             run_at = now() + make_interval(secs =>
                 least(10 * power(2, attempts - 1), 3600) * (0.8 + random() * 0.4)),
             last_error = 'lease expired', locked_by = NULL, locked_until = NULL
         WHERE id IN (SELECT id FROM jobs
                      WHERE status = 'running' AND locked_until < now()
                      FOR UPDATE SKIP LOCKED)",
    )
    .execute(db)
    .await?
    .rows_affected())
}

/// Delete succeeded and cancelled jobs after 7 days, failed ones after 30, in batches.
pub async fn prune(db: &PgPool) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    loop {
        let n = sqlx::query!(
            // `= ANY(ARRAY(..))` deletes by primary key; `IN (subquery)` planned a full scan.
            "DELETE FROM jobs WHERE id = ANY(ARRAY(
                 SELECT id FROM jobs
                 WHERE (status IN ('succeeded', 'cancelled') AND finished_at < now() - interval '7 days')
                    OR (status = 'failed' AND finished_at < now() - interval '30 days')
                 LIMIT $1))",
            PRUNE_BATCH,
        )
        .execute(db)
        .await?
        .rows_affected();
        total += n;
        if n < PRUNE_BATCH.unsigned_abs() {
            return Ok(total);
        }
    }
}

/// First `ERROR_CHARS` characters, without NUL (Postgres text refuses it).
fn cap(error: &str) -> String {
    error
        .chars()
        .filter(|c| *c != '\0')
        .take(ERROR_CHARS)
        .collect()
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}
