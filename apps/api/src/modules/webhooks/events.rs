//! The fixed event list and `emit`, the outbox write. Next on the list (not emitted yet):
//! `order.created`, `lot.created`, `harvest.reported`, `passport.published`, `sale.attributed`.

use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    jobs::{
        kinds::WEBHOOK_FANOUT,
        repo::{self as jobs, NewJob},
    },
    modules::webhooks::repo,
};

/// Stock level of a product changed somewhere (`stock::repo::record`; a stock row is only
/// deleted at level 0, so deletes send nothing).
pub const STOCK_CHANGED: &str = "stock.changed";
/// A product or one of its variants was created, changed or deleted.
pub const PRODUCT_CHANGED: &str = "product.changed";

/// Events an endpoint may subscribe to.
pub const EVENTS: &[&str] = &[PRODUCT_CHANGED, STOCK_CHANGED];

/// Outbox write, in the caller's transaction: the event row when an active endpoint of the
/// workspace subscribes to `kind` (else one index probe and nothing), plus one fan-out job per
/// transaction. A rollback takes both with it.
pub async fn emit(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    kind: &'static str,
    payload: Value,
) -> Result<(), sqlx::Error> {
    let xid = repo::insert_event_if_subscribed(conn, workspace_id, kind, payload).await?;
    let Some(xid) = xid else {
        return Ok(());
    };
    // Dedupe per transaction (not per workspace): a fan-out already running may have read
    // past this uncommitted event, so a later transaction must queue its own.
    let job = NewJob {
        workspace_id: Some(workspace_id),
        dedupe_key: Some(format!("webhook_fanout:{xid}")),
        concurrency_key: Some(format!("webhook_fanout:{workspace_id}")),
        ..NewJob::new(WEBHOOK_FANOUT)
    };
    jobs::enqueue(conn, &job).await.map(|_| ())
}

/// `product.changed` for one product write (`action`: created, updated or deleted).
pub async fn product_changed(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    action: &'static str,
) -> Result<(), sqlx::Error> {
    let payload = serde_json::json!({ "product_id": product_id, "action": action });
    emit(conn, workspace_id, PRODUCT_CHANGED, payload).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::PgPool;

    async fn workspace_with_endpoint(pool: &PgPool, events: &str) -> Uuid {
        sqlx::query_scalar(
            "WITH u AS (INSERT INTO users (email, first_name, last_name, password_hash)
                        VALUES ('o@x.io', 'O', 'W', 'h') RETURNING id),
                  w AS (INSERT INTO workspaces (name, created_by_user_id)
                        SELECT 'acme', id FROM u RETURNING id),
                  e AS (INSERT INTO webhook_endpoints (workspace_id, url, events, secret_sealed,
                                                       secret_key_id)
                        SELECT id, 'https://a.io/h', $1::text[], '\\x00', 'k' FROM w)
             SELECT id FROM w",
        )
        .bind(events)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn count(pool: &PgPool, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
    }

    #[sqlx::test]
    async fn rollback_leaves_no_event_and_no_job(pool: PgPool) {
        let ws = workspace_with_endpoint(&pool, "{stock.changed}").await;
        let mut tx = pool.begin().await.unwrap();

        emit(&mut tx, ws, STOCK_CHANGED, json!({})).await.unwrap();
        tx.rollback().await.unwrap();

        assert_eq!(count(&pool, "SELECT count(*) FROM webhook_events").await, 0);
        assert_eq!(count(&pool, "SELECT count(*) FROM jobs").await, 0);
    }

    #[sqlx::test]
    async fn one_transaction_is_n_events_and_one_job_two_are_two_jobs(pool: PgPool) {
        let ws = workspace_with_endpoint(&pool, "{stock.changed}").await;
        let mut tx = pool.begin().await.unwrap();
        for _ in 0..3 {
            emit(&mut tx, ws, STOCK_CHANGED, json!({})).await.unwrap();
        }
        tx.commit().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        emit(&mut tx, ws, STOCK_CHANGED, json!({})).await.unwrap();
        tx.commit().await.unwrap();

        assert_eq!(count(&pool, "SELECT count(*) FROM webhook_events").await, 4);
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM jobs WHERE kind = 'webhook_fanout'"
            )
            .await,
            2
        );
    }
}
