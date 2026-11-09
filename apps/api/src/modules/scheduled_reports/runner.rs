//! The `scheduled_reports` job: claim due schedules, recheck the owner's access, move each to
//! its next slot, commit, then mail. At most once per slot: a crash after the commit loses
//! that one mail, never sends it twice; a long outage sends once, not a backlog.

use chrono::Utc;

use crate::{
    AppState,
    error::AppError,
    jobs::runner::Step,
    mail::Outgoing,
    modules::{
        auth::token,
        permissions::{model::Granted, repo as permissions},
        scheduled_reports::{
            repo::{self, Due},
            report, service,
        },
    },
};

/// Schedules claimed per transaction.
const BATCH: i64 = 20;
/// A run starts no new batch after this (job chunks aim for 10 s); the rest wait for the
/// next tick (5 min).
const BUDGET: std::time::Duration = std::time::Duration::from_secs(8);

/// One job run over due schedules, batch by batch within `BUDGET`.
pub async fn run(state: &AppState) -> Result<Step, AppError> {
    let started = std::time::Instant::now();
    while started.elapsed() < BUDGET {
        let mut tx = state.db.begin().await?;
        let due = repo::claim_due(&mut *tx, BATCH).await?;
        if due.is_empty() {
            break;
        }
        let mut ready = Vec::new();
        for d in due {
            let access = permissions::access(&mut *tx, d.workspace_id, d.user_id).await?;
            let granted = access
                .filter(|a| a.active)
                .map(|a| Granted::new(a.owner, a.permissions))
                .filter(|g| service::permission(&d.report).is_none_or(|p| g.allows(p)));
            let Some(granted) = granted else {
                repo::pause_lost(&mut *tx, d.id).await?;
                repo::log_run(&mut *tx, &d, "paused_access", None).await?;
                continue;
            };
            let next = repo::next_run(&mut *tx, d.workspace_id, &d.rule(), Utc::now()).await?;
            repo::advance(&mut *tx, d.id, next).await?;
            ready.push((d, granted));
        }
        // Slots move before any mail leaves (at most once).
        tx.commit().await?;
        for (d, granted) in ready {
            let outcome = match deliver(state, &d, &granted).await {
                Ok(o) => o,
                Err(err) => {
                    tracing::warn!(error = %err, schedule = %d.id, "scheduled report failed");
                    Outcome::Failed
                }
            };
            let (name, hash) = match &outcome {
                Outcome::Sent(h) => ("sent", Some(&h[..])),
                Outcome::Empty => ("skipped_empty", None),
                Outcome::Failed => ("failed", None),
            };
            repo::log_run(&state.db, &d, name, hash).await?;
        }
    }
    Ok(Step::Done)
}

enum Outcome {
    /// Mailed; sha256 of the unsubscribe token it carried.
    Sent([u8; 32]),
    Empty,
    Failed,
}

async fn deliver(state: &AppState, d: &Due, granted: &Granted) -> Result<Outcome, AppError> {
    let Some(r) = report::build(&state.db, d, |p| granted.allows(p), Utc::now()).await? else {
        return Ok(Outcome::Empty);
    };
    let to = repo::email(&state.db, d.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let workspace = repo::workspace_name(&state.db, d.workspace_id).await?;
    let zone = repo::zone(&state.db, d.workspace_id).await?;
    let (plain, hash) = token::generate()?;
    let url = &state.frontend_url;
    // shortcut: English plain text only (as invite mail); send in the user's language once
    // mail templates are ported.
    let body = format!(
        "{text}\n--\nYou asked Genesy to send you this report from {workspace} {when}.\n\
         Manage your scheduled reports: {url}/settings/scheduled-reports\n\
         Stop this report: {url}/unsubscribe?token={token}\n",
        text = r.text,
        when = report::schedule_words(d, &zone),
        token = plain.expose(),
    );
    state
        .mailer
        .send(Outgoing {
            to,
            subject: format!("Genesy report: {} – {workspace}", r.title),
            body,
        })
        .await?;
    Ok(Outcome::Sent(hash))
}
