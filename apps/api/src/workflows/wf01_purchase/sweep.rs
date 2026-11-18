//! `wf01_sweep` (PLAN §6a, R3-§5): re-read every live request of the pair. Gone, cancelled or
//! ineligible sources invalidate; changed ones get a new revision; blocked ones re-evaluate
//! against current mappings. Requests with an execution only raise incidents (the sweep never
//! changes an execution).

use sqlx::PgPool;
use uuid::Uuid;

use super::{
    Stamp,
    evaluate::{self, PairCtx, Seen, WORKFLOW_KEY},
    job_pair, provider, ready, source_client, stamp,
};
use crate::{
    error::AppError,
    integrations::{capability::PurchaseRequestSource, providers::SourceClient},
    jobs::{kinds::Ctx, repo::Claimed, runner::Step},
};

/// Requests per source read.
const BATCH: i64 = 100;

/// Job entry.
pub async fn run(ctx: &Ctx, job: &Claimed) -> Result<Step, AppError> {
    let (ws, pair) = job_pair(job)?;
    let Some(r) = ready(&ctx.db, ws, pair).await? else {
        return Ok(Step::Done);
    };
    let client = source_client(ctx, &r)?;
    sweep_pair(&ctx.db, &r.pair, &client).await?;
    stamp(&ctx.db, ws, pair, Stamp::Swept).await?;
    Ok(Step::Done)
}

/// Every live request worth re-reading, in id batches, one transaction per batch.
pub async fn sweep_pair(
    db: &PgPool,
    pair: &PairCtx,
    client: &SourceClient,
) -> Result<(), AppError> {
    let days = i32::try_from(pair.setting("succeeded_watch_days", 30)).unwrap_or(30);
    let mut after = Uuid::nil();
    loop {
        // Live without execution, write in flight, or done within the watch window.
        let rows = sqlx::query!(
            "SELECT r.id, r.subject_ref FROM approval_requests r
             LEFT JOIN workflow_executions e ON e.approval_request_id = r.id
             WHERE r.workspace_id = $1 AND r.pair_id = $2 AND r.workflow_key = $3
               AND r.status IN ('blocked', 'pending', 'approved') AND r.id > $4
               AND (e.id IS NULL OR e.status = 'awaiting_reconciliation'
                    OR e.phase IN ('write_intent', 'write_sent', 'verifying')
                    OR (e.status IN ('succeeded', 'succeeded_with_variance', 'resolved_manual')
                        AND e.updated_at >= now() - make_interval(days => $5)))
             ORDER BY r.id LIMIT $6",
            pair.ws,
            pair.id,
            WORKFLOW_KEY,
            after,
            days,
            BATCH,
        )
        .fetch_all(db)
        .await?;
        let Some(last) = rows.last() else {
            return Ok(());
        };
        after = last.id;
        let ids: Vec<String> = rows
            .iter()
            .filter_map(|r| r.subject_ref.rsplit(':').next().map(str::to_owned))
            .collect();
        let (found, gone) = client.read(&ids).await.map_err(provider)?;
        let mut tx = db.begin().await?;
        for id in &ids {
            let seen = match found.iter().find(|r| &r.id == id) {
                Some(r) => Seen::Found(r),
                None if gone.contains(id) => Seen::Gone,
                None => continue,
            };
            evaluate::intake(&mut tx, pair, id, seen).await?;
        }
        tx.commit().await?;
        if rows.len() < BATCH as usize {
            return Ok(());
        }
    }
}
