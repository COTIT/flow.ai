//! Gap ledger SQL: workspace-filtered writes and 90-day retention.

use super::gaps::GapInput;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// Insert one validated gap; returns its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    i: &GapInput,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO assistant_gaps(workspace_id,domain,language,intent,tools,outcome,http_status)
         VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id",
        workspace,
        i.domain,
        i.language,
        i.intent,
        &i.tools[..],
        i.outcome,
        i.http_status
    )
    .fetch_one(db)
    .await
}

/// `Some(true)` set, `Some(false)` already set or older than 10 minutes, `None` not in this workspace.
pub async fn follow_up(
    db: &PgPool,
    workspace: Uuid,
    id: Uuid,
    signal: &str,
    route: Option<&str>,
) -> Result<Option<bool>, sqlx::Error> {
    let set = sqlx::query!(
        "UPDATE assistant_gaps SET follow_up=$3,follow_up_route=$4
         WHERE id=$1 AND workspace_id=$2 AND follow_up IS NULL AND created_at>now()-interval '10 minutes'",
        id,
        workspace,
        signal,
        route
    )
    .execute(db)
    .await?
    .rows_affected();
    if set == 1 {
        return Ok(Some(true));
    }
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM assistant_gaps WHERE id=$1 AND workspace_id=$2) AS "e!""#,
        id,
        workspace
    )
    .fetch_one(db)
    .await?;
    Ok(exists.then_some(false))
}

/// Hourly: drop gaps older than 90 days.
pub async fn prune(db: &PgPool) -> Result<u64, sqlx::Error> {
    // shortcut: unbatched delete; batch like retention::prune once gaps exceed ~100k/day.
    Ok(
        sqlx::query!("DELETE FROM assistant_gaps WHERE created_at<now()-interval '90 days'")
            .execute(db)
            .await?
            .rows_affected(),
    )
}
