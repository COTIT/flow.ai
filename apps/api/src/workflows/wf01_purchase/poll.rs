//! `wf01_poll` (PLAN §6a, facts O1/O5): O5 controls first, then the write_date/id cursor with a
//! 120 s overlap; one Postgres transaction per page holds the requests, events and the cursor
//! advance, so a crash mid-page leaves the cursor where it was and the page is read again.

use chrono::Duration;
use sqlx::PgPool;

use super::{
    Stamp, companies,
    evaluate::{self, PairCtx, Seen},
    job_pair, pause_for_drift, provider, ready, source_client, stamp, unmet,
};
use crate::{
    error::AppError,
    integrations::{
        capability::{Cursor, PurchaseRequestSource},
        cursors,
        providers::SourceClient,
    },
    jobs::{kinds::Ctx, repo::Claimed, runner::Step},
};

/// Odoo commits can surface after a later write_date (facts L96-98); re-read this window.
pub const OVERLAP_SECS: i64 = 120;
/// Records per page.
pub const PAGE: u32 = 100;
/// Pages per run; more continues in the next chunk (`Step::More`).
pub const MAX_PAGES: usize = 20;

/// Cursor stream of a pair.
pub fn stream(pair: &PairCtx) -> String {
    format!("wf01_purchase:{}", pair.id)
}

/// Job entry.
pub async fn run(ctx: &Ctx, job: &Claimed) -> Result<Step, AppError> {
    let (ws, pair) = job_pair(job)?;
    let Some(r) = ready(&ctx.db, ws, pair).await? else {
        return Ok(Step::Done);
    };
    let client = source_client(ctx, &r)?;
    let step = poll_pair(&ctx.db, &r.pair, &client).await?;
    if step == Step::Done {
        stamp(&ctx.db, ws, pair, Stamp::Polled).await?;
    }
    Ok(step)
}

/// Check controls, then walk changed RFQs page by page.
pub async fn poll_pair(
    db: &PgPool,
    pair: &PairCtx,
    client: &SourceClient,
) -> Result<Step, AppError> {
    let companies = companies(db, pair.ws, pair.id).await?;
    if companies.is_empty() {
        return Ok(Step::Done);
    }
    let controls = client.read_controls(&companies).await.map_err(provider)?;
    let bad = unmet(&companies, &controls);
    if !bad.is_empty() {
        pause_for_drift(db, pair, &bad).await?;
        return Ok(Step::Done);
    }

    let stream = stream(pair);
    let stored = cursors::get(db, pair.ws, pair.source, &stream).await?;
    let start = match &stored {
        Some(c) => c.ts,
        None => {
            pair.activated_at.unwrap_or_else(chrono::Utc::now)
                - Duration::days(pair.setting("backfill_days", 0))
        }
    };
    let mut high = stored.map(|c| (c.ts, c.id));
    let mut query = Cursor {
        ts: start - Duration::seconds(OVERLAP_SECS),
        id: 0,
    };
    for _ in 0..MAX_PAGES {
        let page = client
            .list_changed(&companies, &query, PAGE)
            .await
            .map_err(provider)?;
        let Some(last) = page.items.last() else {
            return Ok(Step::Done);
        };
        let last = (last.changed_at, last.id.parse::<i64>().unwrap_or(0));
        let ids: Vec<String> = page.items.iter().map(|i| i.id.clone()).collect();
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
        // The overlap re-reads older positions; the cursor never moves back.
        if high.is_none_or(|h| last > h) {
            cursors::advance(
                &mut tx,
                pair.ws,
                pair.source,
                &stream,
                &Cursor {
                    ts: last.0,
                    id: last.1,
                },
            )
            .await?;
            high = Some(last);
        }
        tx.commit().await?;

        match page.next {
            Some(next) => query = next,
            None => return Ok(Step::Done),
        }
    }
    Ok(Step::More)
}
