//! SQL for `email_send_logs` and `email_send_log_recipients`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A send about to be attempted.
pub struct NewLog<'a> {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub provider: &'a str,
    pub from_email: &'a str,
    pub subject: &'a str,
    pub to: &'a [String],
    pub cc: &'a [String],
    pub bcc: &'a [String],
}

/// A log row for the list.
pub struct LogRow {
    pub id: Uuid,
    pub subject: String,
    pub status: String,
    pub provider: String,
    pub to_recipients: Value,
    pub cc_recipients: Value,
    pub bcc_recipients: Value,
    pub recipient_count: i64,
    pub opened_count: i64,
    pub sent_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub bounced_at: Option<DateTime<Utc>>,
    pub opened_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// The email of the live contact `id` of this workspace: None without such a contact,
/// Some(None) when it has no email.
pub async fn contact_email(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<Option<String>>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT email FROM contacts WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// The `pending` row of a send and one row per recipient.
pub async fn insert(tx: &mut PgConnection, l: &NewLog<'_>) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO email_send_logs (id, workspace_id, user_id, provider, from_email, subject,
                                      to_recipients, cc_recipients, bcc_recipients)
         VALUES ($1, $2, $3, $4, $5, $6, to_jsonb($7::text[]), to_jsonb($8::text[]),
                 to_jsonb($9::text[]))",
        l.id,
        l.workspace_id,
        l.user_id,
        l.provider,
        l.from_email,
        l.subject,
        l.to,
        l.cc,
        l.bcc,
    )
    .execute(&mut *tx)
    .await?;
    let (mut emails, mut kinds) = (Vec::new(), Vec::new());
    for (list, kind) in [(l.to, "to"), (l.cc, "cc"), (l.bcc, "bcc")] {
        for e in list {
            emails.push(e.as_str());
            kinds.push(kind);
        }
    }
    sqlx::query!(
        "INSERT INTO email_send_log_recipients (log_id, email, kind)
         SELECT $1, * FROM UNNEST($2::text[], $3::text[]) ON CONFLICT DO NOTHING",
        l.id,
        &emails as &[&str],
        &kinds as &[&str],
    )
    .execute(tx)
    .await?;
    Ok(())
}

/// Move an open send on: `sending`, `accepted` (with the sent time), `failed` (with a short
/// class, never a server's words), `blocked` or `unknown`. A finished row never moves back;
/// one the sweep closed as `unknown` still takes the real outcome.
pub async fn mark(
    db: impl PgExecutor<'_>,
    id: Uuid,
    status: &str,
    message_id: Option<&str>,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE email_send_logs SET status = $2,
            provider_message_id = COALESCE($3, provider_message_id),
            error_message = COALESCE(left($4, 255), error_message),
            sent_at = CASE WHEN $2 = 'accepted' THEN now() ELSE sent_at END,
            updated_at = now()
         WHERE id = $1 AND status IN ('pending', 'sending', 'unknown')",
        id,
        status,
        message_id,
        error,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Sends a dead request left `pending` or `sending` for over 10 minutes, marked `unknown`.
pub async fn close_stuck(db: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE email_send_logs SET status = 'unknown', updated_at = now()
         WHERE status IN ('pending', 'sending') AND updated_at < now() - interval '10 minutes'",
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

/// One page of a workspace's sends, newest first, plus the total.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LogRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        LogRow,
        r#"SELECT l.id, l.subject, l.status, l.provider, l.to_recipients, l.cc_recipients,
                  l.bcc_recipients,
                  (SELECT count(*) FROM email_send_log_recipients r WHERE r.log_id = l.id)
                    AS "recipient_count!",
                  (SELECT COALESCE(sum(r.opened_count), 0) FROM email_send_log_recipients r
                   WHERE r.log_id = l.id)::bigint AS "opened_count!",
                  l.sent_at, l.delivered_at, l.bounced_at, l.opened_at, l.created_at
           FROM email_send_logs l
           WHERE l.workspace_id = $1
           ORDER BY l.created_at DESC, l.id DESC
           LIMIT $2 OFFSET $3"#,
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM email_send_logs WHERE workspace_id = $1"#,
        workspace_id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}
