//! Write-only connection secrets in `integration_secrets`, sealed by `CredentialKey` under path
//! `integration:<connection_id>:<name>`. Reads give the mask or the plaintext to a provider
//! client; plaintext is never returned by a route or logged.

use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::{crypto::CredentialKey, error::AppError, modules::auth::token::PlainSecret};

/// Seal path of one secret.
fn path(connection_id: Uuid, name: &str) -> String {
    format!("integration:{connection_id}:{name}")
}

/// Names of the secrets stored for a connection.
pub async fn names(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    connection_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT name FROM integration_secrets WHERE workspace_id = $1 AND connection_id = $2",
        ws,
        connection_id
    )
    .fetch_all(db)
    .await
}

/// Names stored per connection, for a page of connections.
pub async fn names_for(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT connection_id, name FROM integration_secrets
         WHERE workspace_id = $1 AND connection_id = ANY($2)",
        ws,
        ids
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.connection_id, r.name))
        .collect())
}

/// Seal `plain` and store it (replacing any earlier value).
pub async fn store(
    tx: &mut PgConnection,
    key: &CredentialKey,
    ws: Uuid,
    connection_id: Uuid,
    name: &str,
    plain: &str,
    user: Uuid,
) -> Result<(), AppError> {
    let sealed = key.seal(ws, &path(connection_id, name), plain)?;
    sqlx::query!(
        "INSERT INTO integration_secrets (workspace_id, connection_id, name, key_id, sealed, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (connection_id, name) DO UPDATE
         SET key_id = EXCLUDED.key_id, sealed = EXCLUDED.sealed,
             updated_by = EXCLUDED.updated_by, updated_at = now()",
        ws,
        connection_id,
        name,
        key.id(),
        sealed,
        user
    )
    .execute(tx)
    .await?;
    Ok(())
}

/// Drop every secret of a connection.
pub async fn clear(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    connection_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM integration_secrets WHERE workspace_id = $1 AND connection_id = $2",
        ws,
        connection_id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Plaintext of one stored secret, for a provider client. `NotFound` when none is stored.
pub async fn open(
    db: impl PgExecutor<'_>,
    key: &CredentialKey,
    ws: Uuid,
    connection_id: Uuid,
    name: &str,
) -> Result<PlainSecret, AppError> {
    let sealed = sqlx::query_scalar!(
        "SELECT sealed FROM integration_secrets
         WHERE workspace_id = $1 AND connection_id = $2 AND name = $3",
        ws,
        connection_id,
        name
    )
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)?;
    key.open(ws, &path(connection_id, name), &sealed)
}
