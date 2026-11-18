//! `wf01_full_scan` (R3-§6a): every eligible RFQ id per mapped company, paged by id, read and
//! run through intake exactly as the poll would. Catches changes the cursor skipped (a late
//! commit carrying an older write_date) within one scan period.

use sqlx::PgPool;

use super::{
    Stamp, companies,
    evaluate::{self, PairCtx, Seen},
    job_pair, provider, ready, source_client, stamp,
};
use crate::{
    error::AppError,
    integrations::{capability::PurchaseRequestSource, providers::SourceClient},
    jobs::{kinds::Ctx, repo::Claimed, runner::Step},
};

/// Ids per page.
const PAGE: u32 = 100;

/// Job entry.
pub async fn run(ctx: &Ctx, job: &Claimed) -> Result<Step, AppError> {
    let (ws, pair) = job_pair(job)?;
    let Some(r) = ready(&ctx.db, ws, pair).await? else {
        return Ok(Step::Done);
    };
    let client = source_client(ctx, &r)?;
    scan_pair(&ctx.db, &r.pair, &client).await?;
    stamp(&ctx.db, ws, pair, Stamp::FullScan).await?;
    Ok(Step::Done)
}

/// One full pass, one transaction per page of ids.
pub async fn scan_pair(db: &PgPool, pair: &PairCtx, client: &SourceClient) -> Result<(), AppError> {
    for company in companies(db, pair.ws, pair.id).await? {
        let mut after = 0_i64;
        loop {
            let ids = client
                .list_eligible_ids(&company, after, PAGE)
                .await
                .map_err(provider)?;
            let Some(last) = ids.last() else { break };
            after = last.parse().unwrap_or(i64::MAX);
            let (found, _) = client.read(&ids).await.map_err(provider)?;
            let mut tx = db.begin().await?;
            for r in &found {
                evaluate::intake(&mut tx, pair, &r.id, Seen::Found(r)).await?;
            }
            tx.commit().await?;
            if ids.len() < PAGE as usize {
                break;
            }
        }
    }
    Ok(())
}
