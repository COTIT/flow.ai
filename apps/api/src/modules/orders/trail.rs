//! Order trail (`order_events`): one row per action on an order and who did it. Written in
//! the action's own transaction; read newest first. Only orders acted on since migration
//! `order_events` have rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// Who did an action.
#[derive(Clone, Copy, Debug)]
pub enum Actor {
    /// A member, from the office.
    User(Uuid),
    /// A till user at a register.
    TillUser(Uuid),
    /// An app key (the web shop's checkout).
    AppKey(Uuid),
    /// Nobody: a job or a payment provider.
    System,
}

/// Write one event; `at` None = now.
pub async fn log(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    order_id: Uuid,
    kind: &str,
    actor: Actor,
    meta: Option<Value>,
    at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    let (user, till_user, app_key) = match actor {
        Actor::User(id) => (Some(id), None, None),
        Actor::TillUser(id) => (None, Some(id), None),
        Actor::AppKey(id) => (None, None, Some(id)),
        Actor::System => (None, None, None),
    };
    sqlx::query!(
        "INSERT INTO order_events (workspace_id, order_id, kind, user_id, till_user_id,
                                   app_key_id, meta, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, coalesce($8, clock_timestamp()))",
        workspace_id,
        order_id,
        kind,
        user,
        till_user,
        app_key,
        meta,
        at,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// One event with its actor's display parts.
pub struct EventRow {
    pub id: Uuid,
    pub kind: String,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub user_id: Option<Uuid>,
    pub till_user_id: Option<Uuid>,
    pub app_key_id: Option<Uuid>,
    /// Set only while the user is a member of the workspace and not deleted.
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub till_user_name: Option<String>,
    pub app_key_name: Option<String>,
}

/// A page of `order_id`'s events, newest first, and the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    order_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EventRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT e.id, e.kind, e.meta, e.created_at, e.user_id, e.till_user_id, e.app_key_id,
                  u.first_name AS "first_name?", u.middle_name, u.last_name AS "last_name?",
                  t.name AS "till_user_name?", k.name AS "app_key_name?",
                  count(*) OVER () AS "total!"
           FROM order_events e
           LEFT JOIN workspace_members m ON m.workspace_id = e.workspace_id AND m.user_id = e.user_id
           LEFT JOIN users u ON u.id = m.user_id AND u.deleted_at IS NULL
           LEFT JOIN till_users t ON t.workspace_id = e.workspace_id AND t.id = e.till_user_id
           LEFT JOIN app_keys k ON k.workspace_id = e.workspace_id AND k.id = e.app_key_id
           WHERE e.workspace_id = $1 AND e.order_id = $2
           ORDER BY e.created_at DESC, e.id DESC
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        order_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|r| EventRow {
            id: r.id,
            kind: r.kind,
            meta: r.meta,
            created_at: r.created_at,
            user_id: r.user_id,
            till_user_id: r.till_user_id,
            app_key_id: r.app_key_id,
            first_name: r.first_name,
            middle_name: r.middle_name,
            last_name: r.last_name,
            till_user_name: r.till_user_name,
            app_key_name: r.app_key_name,
        })
        .collect();
    Ok((rows, total))
}
