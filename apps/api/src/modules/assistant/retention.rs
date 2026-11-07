//! Retention for cleaned assistant context after every action has left audit.

use sqlx::PgPool;

/// Purge old cleaned sessions only after all action audit and remote references are gone.
pub async fn prune(db: &PgPool) -> Result<u64, sqlx::Error> {
    let mut tx = db.begin().await?;
    // shortcut: 100 sessions/hour; raise batch or cadence before retirement exceeds 2400/day.
    let expired = sqlx::query!(
        "SELECT s.workspace_id,s.id FROM assistant_sessions s
         WHERE s.state='cleaned' AND s.cleaned_at<now()-interval '90 days'
           AND s.conversation_id IS NULL
           AND NOT EXISTS(SELECT 1 FROM assistant_batches b WHERE b.workspace_id=s.workspace_id AND b.session_id=s.id)
           AND NOT EXISTS(SELECT 1 FROM assistant_actions a
               WHERE a.workspace_id=s.workspace_id AND a.session_id=s.id)
           AND NOT EXISTS(SELECT 1 FROM assistant_uploads u
               WHERE u.workspace_id=s.workspace_id AND u.session_id=s.id
                 AND (u.state<>'cleaned' OR u.file_id IS NOT NULL))
         ORDER BY s.cleaned_at,s.id LIMIT 100 FOR UPDATE OF s SKIP LOCKED"
    )
    .fetch_all(&mut *tx)
    .await?;
    if expired.is_empty() {
        tx.commit().await?;
        return Ok(0);
    }
    let workspaces: Vec<_> = expired.iter().map(|row| row.workspace_id).collect();
    let sessions: Vec<_> = expired.iter().map(|row| row.id).collect();
    let threads: Vec<_> = sessions.iter().map(ToString::to_string).collect();
    sqlx::query!(
        "DELETE FROM assistant_checkpoint.checkpoint_writes WHERE thread_id=ANY($1)",
        &threads[..]
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM assistant_checkpoint.checkpoint_blobs WHERE thread_id=ANY($1)",
        &threads[..]
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM assistant_checkpoint.checkpoints WHERE thread_id=ANY($1)",
        &threads[..]
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM assistant_uploads u
         USING unnest($1::uuid[],$2::uuid[]) AS expired(workspace_id,id)
         WHERE u.workspace_id=expired.workspace_id AND u.session_id=expired.id
           AND u.state='cleaned' AND u.file_id IS NULL",
        &workspaces[..],
        &sessions[..]
    )
    .execute(&mut *tx)
    .await?;
    let removed = sqlx::query!(
        "DELETE FROM assistant_sessions s
         USING unnest($1::uuid[],$2::uuid[]) AS expired(workspace_id,id)
         WHERE s.workspace_id=expired.workspace_id AND s.id=expired.id",
        &workspaces[..],
        &sessions[..]
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(removed)
}
