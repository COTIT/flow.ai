//! Webhook SQL: endpoints, the event outbox, fan-out, deliveries and their prune. Every
//! workspace query filters `workspace_id`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

/// An endpoint, without its sealed secret.
pub struct EndpointRow {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub description: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A delivery with its event's type, for the history read.
pub struct DeliveryRow {
    pub id: Uuid,
    pub event_id: Uuid,
    pub event_type: String,
    pub status: String,
    pub attempts: i32,
    pub last_status: Option<i16>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

/// One page of an endpoint's deliveries, newest first, optionally one status, and the total.
pub async fn deliveries(
    db: &PgPool,
    workspace_id: Uuid,
    endpoint_id: Uuid,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<DeliveryRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        DeliveryRow,
        "SELECT d.id, d.event_id, e.type AS event_type, d.status, d.attempts, d.last_status,
                d.last_error, d.created_at, d.updated_at, d.delivered_at
         FROM webhook_deliveries d
         JOIN webhook_events e ON e.workspace_id = d.workspace_id AND e.id = d.event_id
         WHERE d.workspace_id = $1 AND d.endpoint_id = $2 AND ($3::text IS NULL OR d.status = $3)
         ORDER BY d.created_at DESC, d.id DESC
         LIMIT $4 OFFSET $5",
        workspace_id,
        endpoint_id,
        status,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM webhook_deliveries
           WHERE workspace_id = $1 AND endpoint_id = $2 AND ($3::text IS NULL OR status = $3)"#,
        workspace_id,
        endpoint_id,
        status,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Fields of a new endpoint; `id` is chosen first so the secret can be sealed to it.
pub struct NewEndpoint<'a> {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub url: &'a str,
    pub events: &'a [String],
    pub description: Option<&'a str>,
    pub secret_sealed: &'a [u8],
    pub secret_key_id: &'a str,
    pub created_by: Uuid,
}

/// Changes to an endpoint; None keeps the field.
pub struct EndpointPatch {
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    pub description: Option<Option<String>>,
    pub active: Option<bool>,
}

pub async fn insert(
    db: impl PgExecutor<'_>,
    e: &NewEndpoint<'_>,
) -> Result<EndpointRow, sqlx::Error> {
    sqlx::query_as!(
        EndpointRow,
        "INSERT INTO webhook_endpoints
             (id, workspace_id, url, events, description, secret_sealed, secret_key_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, url, events, description, active, created_at, updated_at",
        e.id,
        e.workspace_id,
        e.url,
        e.events,
        e.description,
        e.secret_sealed,
        e.secret_key_id,
        e.created_by,
    )
    .fetch_one(db)
    .await
}

pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<EndpointRow>, sqlx::Error> {
    sqlx::query_as!(
        EndpointRow,
        "SELECT id, url, events, description, active, created_at, updated_at
         FROM webhook_endpoints WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// One page, newest first, and the workspace's endpoint count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EndpointRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        EndpointRow,
        "SELECT id, url, events, description, active, created_at, updated_at
         FROM webhook_endpoints WHERE workspace_id = $1
         ORDER BY created_at DESC, id DESC
         LIMIT $2 OFFSET $3",
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM webhook_endpoints WHERE workspace_id = $1"#,
        workspace_id
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Endpoints of the workspace.
pub async fn count(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM webhook_endpoints WHERE workspace_id = $1"#,
        workspace_id
    )
    .fetch_one(db)
    .await
}

/// Apply `patch`; None when no such endpoint in this workspace.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    patch: &EndpointPatch,
) -> Result<Option<EndpointRow>, sqlx::Error> {
    sqlx::query_as!(
        EndpointRow,
        "UPDATE webhook_endpoints SET
             url = coalesce($3, url),
             events = coalesce($4, events),
             description = CASE WHEN $5 THEN $6 ELSE description END,
             active = coalesce($7, active),
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2
         RETURNING id, url, events, description, active, created_at, updated_at",
        workspace_id,
        id,
        patch.url,
        patch.events.as_deref(),
        patch.description.is_some(),
        patch.description.clone().flatten(),
        patch.active,
    )
    .fetch_optional(db)
    .await
}

/// Delete (cascades to its deliveries). False: no such endpoint in this workspace.
pub async fn delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "DELETE FROM webhook_endpoints WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Rows one prune statement deletes.
const PRUNE_BATCH: i64 = 1000;

/// Delete deliveries untouched for 30 days: finished ones, and pending ones a job that died
/// mid-run never finished (retries end within hours); how many.
pub async fn prune_deliveries(db: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    loop {
        let n = sqlx::query!(
            "DELETE FROM webhook_deliveries WHERE id = ANY(ARRAY(
                 SELECT id FROM webhook_deliveries
                 WHERE updated_at < now() - interval '30 days'
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

/// Delete events dispatched over 30 days ago that no delivery points at any more; how many.
pub async fn prune_events(db: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    loop {
        let n = sqlx::query!(
            "DELETE FROM webhook_events WHERE id = ANY(ARRAY(
                 SELECT e.id FROM webhook_events e
                 WHERE e.dispatched_at < now() - interval '30 days'
                   AND NOT EXISTS (SELECT 1 FROM webhook_deliveries d WHERE d.event_id = e.id)
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

/// Outbox write: the event row, only when an active endpoint of the workspace subscribes to
/// `kind`. Returns the transaction id when written (one fan-out per transaction).
pub async fn insert_event_if_subscribed(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    kind: &str,
    payload: Value,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"INSERT INTO webhook_events (workspace_id, type, payload)
           SELECT $1, $2, $3
           WHERE EXISTS (SELECT 1 FROM webhook_endpoints
                         WHERE workspace_id = $1 AND active AND $2 = ANY (events))
           RETURNING pg_current_xact_id()::text AS "xid!""#,
        workspace_id,
        kind,
        payload,
    )
    .fetch_optional(conn)
    .await
}

/// One fan-out chunk: lock the workspace's oldest `limit` pending events, write a delivery
/// per subscribed active endpoint (never twice) and mark the events dispatched. Returns how
/// many events it took and the new delivery ids.
pub async fn fan_out(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    limit: i64,
) -> Result<(i64, Vec<Uuid>), sqlx::Error> {
    let out = sqlx::query!(
        r#"WITH ev AS (
               SELECT id, type FROM webhook_events
               WHERE workspace_id = $1 AND dispatched_at IS NULL
               ORDER BY id
               LIMIT $2
               FOR UPDATE SKIP LOCKED
           ), ins AS (
               INSERT INTO webhook_deliveries (workspace_id, event_id, endpoint_id)
               SELECT $1, ev.id, ep.id
               FROM ev
               JOIN webhook_endpoints ep
                 ON ep.workspace_id = $1 AND ep.active AND ev.type = ANY (ep.events)
               ORDER BY ev.id, ep.id
               ON CONFLICT (event_id, endpoint_id) DO NOTHING
               RETURNING id
           ), done AS (
               UPDATE webhook_events SET dispatched_at = now()
               WHERE workspace_id = $1 AND id IN (SELECT id FROM ev)
               RETURNING 1
           )
           SELECT (SELECT count(*) FROM done) AS "events!",
                  coalesce((SELECT array_agg(id ORDER BY id) FROM ins), '{}') AS "deliveries!""#,
        workspace_id,
        limit,
    )
    .fetch_one(conn)
    .await?;
    Ok((out.events, out.deliveries))
}

/// Workspaces holding events still undispatched after `minutes` (their fan-out was lost),
/// at most `limit`. Reads the pending partial index only.
pub async fn stale_pending_workspaces(
    db: impl PgExecutor<'_>,
    minutes: i32,
    limit: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT DISTINCT workspace_id FROM webhook_events
         WHERE dispatched_at IS NULL AND created_at < now() - make_interval(mins => $1)
         LIMIT $2",
        minutes,
        limit,
    )
    .fetch_all(db)
    .await
}

/// What one delivery attempt needs: the event, its endpoint and the sealed secret.
pub struct ToSend {
    pub status: String,
    pub endpoint_id: Uuid,
    pub event_id: Uuid,
    pub kind: String,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
    pub url: String,
    pub active: bool,
    pub secret_sealed: Vec<u8>,
}

/// The delivery `id` of this workspace with its event and endpoint, else None.
pub async fn to_send(
    db: &PgPool,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<ToSend>, sqlx::Error> {
    sqlx::query_as!(
        ToSend,
        r#"SELECT d.status, d.endpoint_id, e.id AS event_id, e.type AS kind, e.payload,
                  e.created_at, ep.url, ep.active, ep.secret_sealed
           FROM webhook_deliveries d
           JOIN webhook_events e ON e.workspace_id = d.workspace_id AND e.id = d.event_id
           JOIN webhook_endpoints ep ON ep.workspace_id = d.workspace_id AND ep.id = d.endpoint_id
           WHERE d.workspace_id = $1 AND d.id = $2"#,
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Count one attempt and its result (`status`: pending, delivered or failed).
pub async fn record_attempt(
    db: &PgPool,
    workspace_id: Uuid,
    id: Uuid,
    status: &str,
    http_status: Option<i16>,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE webhook_deliveries SET
             attempts = attempts + 1, status = $3, last_status = $4, last_error = $5,
             delivered_at = CASE WHEN $3 = 'delivered' THEN now() END, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        status,
        http_status,
        error,
    )
    .execute(db)
    .await
    .map(|_| ())
}
