//! Workspace book gate (masterplan §6): transaction-scoped advisory lock `erp.book|<ws>`.
//! Ordinary affected writes take it shared; calendar/setup/activation/close take it exclusive.
//! Several workspaces lock in ascending UUID order, so two writers never wait on each other
//! in opposite order. Take it before any resource row lock.

use sqlx::PgConnection;
use uuid::Uuid;

/// Shared gate on every workspace in `workspaces` (sorted, deduplicated), held until commit.
pub async fn shared(conn: &mut PgConnection, workspaces: &[Uuid]) -> Result<(), sqlx::Error> {
    for ws in order(workspaces) {
        sqlx::query(
            "SELECT pg_advisory_xact_lock_shared(hashtextextended('erp.book|' || $1::text, 0))",
        )
        .bind(ws)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Exclusive gate on one workspace, held until commit: no shared holder runs beside it.
pub async fn exclusive(conn: &mut PgConnection, workspace: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('erp.book|' || $1::text, 0))")
        .bind(workspace)
        .execute(conn)
        .await?;
    Ok(())
}

/// Lock order: ascending, each workspace once.
fn order(workspaces: &[Uuid]) -> Vec<Uuid> {
    let mut ws = workspaces.to_vec();
    ws.sort_unstable();
    ws.dedup();
    ws
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_in_ascending_order_once_each() {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        assert_eq!(order(&[b, a, b, a]), vec![a, b]);
        assert!(order(&[]).is_empty());
    }

    /// Whether another session could take the gate in `shared` mode right now.
    async fn free_for_shared(pool: &sqlx::PgPool, ws: Uuid) -> bool {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query_scalar(
            "SELECT pg_try_advisory_xact_lock_shared(hashtextextended('erp.book|' || $1::text, 0))",
        )
        .bind(ws)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn exclusive_blocks_shared_and_shared_admits_shared(pool: sqlx::PgPool) {
        let (ws, other) = (Uuid::now_v7(), Uuid::now_v7());
        let mut holder = pool.begin().await.unwrap();
        shared(&mut holder, &[ws, other]).await.unwrap();
        assert!(free_for_shared(&pool, ws).await, "shared holders coexist");
        holder.rollback().await.unwrap();

        let mut holder = pool.begin().await.unwrap();
        exclusive(&mut holder, ws).await.unwrap();
        assert!(
            !free_for_shared(&pool, ws).await,
            "exclusive keeps writers out"
        );
        assert!(
            free_for_shared(&pool, other).await,
            "other books unaffected"
        );
        holder.commit().await.unwrap();
        assert!(free_for_shared(&pool, ws).await, "released at commit");
    }
}
