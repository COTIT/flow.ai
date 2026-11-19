//! Poll cursors per connection and stream (`wf01_purchase:<pair_id>`). `advance` runs inside
//! the transaction that handled the page.

use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::{error::AppError, integrations::capability::Cursor};

/// The stored cursor, or None before the first page.
pub async fn get(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    connection_id: Uuid,
    stream: &str,
) -> Result<Option<Cursor>, AppError> {
    let row = sqlx::query!(
        "SELECT cursor_ts, cursor_id FROM integration_poll_cursors
         WHERE workspace_id = $1 AND connection_id = $2 AND stream = $3",
        ws,
        connection_id,
        stream
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| Cursor {
        ts: r.cursor_ts,
        id: r.cursor_id,
    }))
}

/// Store `cursor` for the stream.
pub async fn advance(
    tx: &mut PgConnection,
    ws: Uuid,
    connection_id: Uuid,
    stream: &str,
    cursor: &Cursor,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO integration_poll_cursors (workspace_id, connection_id, stream, cursor_ts, cursor_id)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (connection_id, stream) DO UPDATE
         SET cursor_ts = EXCLUDED.cursor_ts, cursor_id = EXCLUDED.cursor_id, updated_at = now()",
        ws,
        connection_id,
        stream,
        cursor.ts,
        cursor.id
    )
    .execute(tx)
    .await?;
    Ok(())
}
