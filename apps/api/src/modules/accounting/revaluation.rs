//! Period FX revaluation (masterplan §13 "Period revaluation", D23, §15): open foreign
//! monetary items revalued at each currency's rate in force on `rate_date`, dated the period's
//! last day. Per item with a non-zero difference: one `fx_revaluation` journal (item control
//! account vs `unrealized_fx`) and one carrying adjustment that moves the item's functional
//! carrying, so later settlement realizes only the rest. One run row + one effect per post.
//! Items with an allocation dated after the rate date stay out (their FX is already realized).
//! No reversal (masterplan has none). Lock order: command row → exclusive book gate → period
//! FOR UPDATE → items FOR UPDATE by id → accounts FOR SHARE → effect → journal numbers last.

use chrono::{Days, NaiveDate};
use rust_decimal::Decimal;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{
                Action, NotRevalued, RevaluationItem, RevaluationRate,
                RevaluationRef, RevaluationResource, RevaluePayload,
            },
            gate,
            journals::{SourceLine, active_book, post_lines},
            repo,
            service::{ok, period_closed, stale, unsupported},
            settlement::Side,
        },
        purchasing::receipts::rate,
    },
};

/// Effect source kind and journal kind of a run.
const SOURCE: &str = "fx_revaluation";

/// Monetary classifications (D23). Deposits/advances are nonmonetary; GRNI is relieved at
/// receipt basis by bills.
const MONETARY: [&str; 5] = [
    "receivable",
    "payable",
    "customer_credit",
    "supplier_credit",
    "recovery_receivable",
];

#[derive(FromRow)]
struct PeriodRow {
    end_date: NaiveDate,
    open: bool,
    version: i64,
}

#[derive(FromRow)]
struct Candidate {
    id: Uuid,
    currency_id: Uuid,
    settled_after: bool,
    revalued_after: bool,
}

#[derive(FromRow)]
struct ItemRow {
    id: Uuid,
    classification: String,
    side: String,
    account_id: Uuid,
    relation_id: Option<Uuid>,
    currency_id: Uuid,
    residual: i64,
    carrying: i64,
    target: i64,
    delta: i64,
    /// |delta|: the journal amount.
    magnitude: i64,
}

struct Prepared {
    resource: RevaluationResource,
    rows: Vec<ItemRow>,
    fingerprint: [u8; 32],
}

/// Minor units as an exact decimal string at `dp` places (display only).
fn amount(minor: i64, dp: i16) -> String {
    Decimal::new(minor, u32::try_from(dp).unwrap_or(0)).to_string()
}

/// The period (404 outside the workspace), optionally locked FOR UPDATE.
async fn period(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    lock: bool,
) -> Result<PeriodRow, AppError> {
    macro_rules! q {
        ($tail:literal) => {
            concat!(
                "SELECT p.end_date, p.state = 'open' AND y.state = 'open' AS open, p.version
                 FROM accounting_periods p
                 JOIN accounting_fiscal_years y
                   ON y.workspace_id = p.workspace_id AND y.id = p.fiscal_year_id
                 WHERE p.workspace_id = $1 AND p.id = $2",
                $tail
            )
        };
    }
    let sql = if lock { q!(" FOR UPDATE OF p") } else { q!("") };
    sqlx::query_as(sql)
        .bind(ws)
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)
}

/// Last day of a period (`end_date` is exclusive).
fn last_day(end: NaiveDate) -> NaiveDate {
    end.checked_sub_days(Days::new(1)).unwrap_or(end)
}

/// Open foreign monetary items issued by `last`, with their exclusion flags; locked FOR
/// UPDATE in id order when `lock`.
async fn candidates(
    conn: &mut PgConnection,
    ws: Uuid,
    functional: Uuid,
    rate_date: NaiveDate,
    last: NaiveDate,
    lock: bool,
) -> Result<Vec<Candidate>, AppError> {
    macro_rules! q {
        ($tail:literal) => {
            concat!(
                "SELECT i.id, i.currency_id,
                EXISTS (SELECT 1 FROM accounting_allocations a
                        WHERE a.workspace_id = i.workspace_id
                          AND (a.source_item_id = i.id OR a.target_item_id = i.id)
                          AND a.reversal_of IS NULL AND a.accounting_date > $3
                          AND NOT EXISTS (SELECT 1 FROM accounting_allocations r
                                          WHERE r.workspace_id = a.workspace_id
                                            AND r.reversal_of = a.id)) AS settled_after,
                EXISTS (SELECT 1 FROM accounting_carrying_adjustments c
                        WHERE c.workspace_id = i.workspace_id AND c.item_id = i.id
                          AND c.accounting_date > $4) AS revalued_after
         FROM accounting_open_items i
         WHERE i.workspace_id = $1 AND i.residual > 0 AND i.currency_id <> $2
           AND i.issue_date <= $4 AND i.classification = ANY($5)
         ORDER BY i.id",
                $tail
            )
        };
    }
    let sql = if lock { q!(" FOR UPDATE OF i") } else { q!("") };
    Ok(sqlx::query_as(sql)
        .bind(ws)
        .bind(functional)
        .bind(rate_date)
        .bind(last)
        .bind(&MONETARY[..])
        .fetch_all(conn)
        .await?)
}

/// Compute the run (no writes): rates, per-item revalued carrying (SQL), exclusions,
/// fingerprint. 422 `accounting_not_configured` on a setup book, `accounting_period_closed`
/// on a closed period/year, field error on `rate_date` after the period or a missing rate,
/// `unsupported_active_erp_operation` when an open residual converts to zero.
async fn prepare(
    conn: &mut PgConnection,
    ws: Uuid,
    period_id: Uuid,
    rate_date: NaiveDate,
    lock: bool,
) -> Result<Prepared, AppError> {
    let functional = active_book(&mut *conn, ws).await?;
    let p = period(&mut *conn, ws, period_id, lock).await?;
    if !p.open {
        return Err(period_closed());
    }
    let last = last_day(p.end_date);
    if rate_date > last {
        return Err(AppError::field(
            "rate_date",
            format!("The rate date must be on or before the period's last day ({last})."),
        ));
    }
    let found = candidates(&mut *conn, ws, functional.id, rate_date, last, lock).await?;
    let mut not_revalued = Vec::new();
    let mut ids = Vec::new();
    let mut currencies: Vec<Uuid> = Vec::new();
    for c in &found {
        let reason = if c.settled_after {
            Some("settled_after_rate_date")
        } else if c.revalued_after {
            Some("revalued_after_accounting_date")
        } else {
            None
        };
        match reason {
            Some(reason) => not_revalued.push(NotRevalued {
                open_item_id: c.id,
                reason,
            }),
            None => {
                ids.push(c.id);
                if !currencies.contains(&c.currency_id) {
                    currencies.push(c.currency_id);
                }
            }
        }
    }
    currencies.sort_unstable();
    let mut rates = Vec::new();
    let mut values = Vec::new();
    let mut inverses = Vec::new();
    for id in &currencies {
        let c = repo::currency(&mut *conn, ws, *id)
            .await?
            .ok_or(AppError::NotFound)?;
        let r = rate(&mut *conn, ws, &c, &functional, rate_date, "rate_date").await?;
        values.push(r.0);
        inverses.push(r.1);
        rates.push(RevaluationRate {
            currency: c,
            rate: r.0.normalize().to_string(),
            inverse: r.1,
            source: r.2.to_owned(),
            effective_on: r.3,
        });
    }
    // Money: revalued carrying and difference computed here, half away from zero.
    let rows: Vec<ItemRow> = sqlx::query_as(
        "SELECT i.id, i.classification, i.side, i.account_id, i.relation_id, i.currency_id,
                i.residual, i.functional_residual AS carrying, t.target,
                t.target - i.functional_residual AS delta,
                abs(t.target - i.functional_residual) AS magnitude
         FROM accounting_open_items i
         JOIN unnest($2::uuid[], $3::numeric[], $4::bool[]) AS r(currency_id, rate, inverse)
           ON r.currency_id = i.currency_id
         JOIN currencies c ON c.workspace_id = i.workspace_id AND c.id = i.currency_id
         CROSS JOIN LATERAL (SELECT money_convert(i.residual, r.rate, c.decimals, $5,
                                                  r.inverse) AS target) t
         WHERE i.workspace_id = $1 AND i.id = ANY($6)
         ORDER BY i.id",
    )
    .bind(ws)
    .bind(&currencies)
    .bind(&values)
    .bind(&inverses)
    .bind(i32::from(functional.decimals))
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    if rows.iter().any(|r| r.target == 0) {
        return Err(unsupported(
            "An open foreign amount converts to zero at this rate. Settle or correct it first.",
        ));
    }
    let rows: Vec<ItemRow> = rows.into_iter().filter(|r| r.delta != 0).collect();
    let fdp = functional.decimals;
    let items: Vec<RevaluationItem> = rows
        .iter()
        .map(|r| {
            let c = &rates
                .iter()
                .find(|x| x.currency.id == r.currency_id)
                .map(|x| x.currency.clone())
                .unwrap_or_else(|| functional.clone());
            RevaluationItem {
                open_item_id: r.id,
                classification: r.classification.clone(),
                relation_id: r.relation_id,
                currency: c.clone(),
                residual: amount(r.residual, c.decimals),
                carrying: amount(r.carrying, fdp),
                revalued: amount(r.target, fdp),
                delta: amount(r.delta, fdp),
                journal_id: None,
            }
        })
        .collect();
    let facts = json!({
        "period": period_id, "version": p.version, "rate_date": rate_date,
        "rates": rates.iter().map(|r| json!([r.currency.id, r.rate, r.inverse, r.effective_on]))
            .collect::<Vec<_>>(),
        "items": rows.iter().map(|r| json!([r.id, r.residual, r.carrying, r.target]))
            .collect::<Vec<_>>(),
        "not_revalued": not_revalued.iter().map(|n| json!([n.open_item_id, n.reason]))
            .collect::<Vec<_>>(),
    });
    let fingerprint: [u8; 32] = Sha256::digest(facts.to_string().as_bytes()).into();
    Ok(Prepared {
        resource: RevaluationResource {
            id: None,
            period_id,
            period_version: p.version,
            rate_date,
            accounting_date: last,
            functional_currency: functional,
            rates,
            items,
            not_revalued,
            review_fingerprint: hex::encode(fingerprint),
        },
        rows,
        fingerprint,
    })
}

/// POST /accounting/periods/{id}/revalue/review: the run as it would post; no writes.
pub async fn review(
    state: &AppState,
    ws: Uuid,
    period_id: Uuid,
    rate_date: NaiveDate,
) -> Result<RevaluationResource, AppError> {
    let mut tx = state.db.begin().await?;
    let p = prepare(&mut tx, ws, period_id, rate_date, false).await?;
    tx.rollback().await?;
    Ok(p.resource)
}

/// POST /accounting/periods/{id}/revalue: post the reviewed run; the run with journal ids.
pub async fn post(
    state: &AppState,
    access: &WorkspaceAccess,
    period_id: Uuid,
    cmd: Command<Action<RevaluePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_periods_revalue";
    let ws = access.workspace_id;
    let by = access.user_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, by, &cmd).await? {
        return Ok(a);
    }
    gate::exclusive(&mut tx, ws).await?;
    let (rate_date, review) = &cmd.input.payload;
    let mut p = prepare(&mut tx, ws, period_id, *rate_date, true).await?;
    if p.resource.period_version != cmd.input.expected_version
        || hex::encode(p.fingerprint) != *review
    {
        return Err(stale());
    }
    let run: Uuid = sqlx::query_scalar("SELECT uuidv7()")
        .fetch_one(&mut *tx)
        .await?;
    let effect = repo::insert_effect(&mut tx, ws, SOURCE, run, "posted", &p.fingerprint, by).await?;
    let date = p.resource.accounting_date;
    sqlx::query(
        "INSERT INTO accounting_revaluations (id, workspace_id, period_id, rate_date,
             accounting_date, effect_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(run)
    .bind(ws)
    .bind(period_id)
    .bind(rate_date)
    .bind(date)
    .bind(effect)
    .bind(by)
    .execute(&mut *tx)
    .await?;
    let reason = format!("FX revaluation at rates of {rate_date}");
    for (row, item) in p.rows.iter().zip(p.resource.items.iter_mut()) {
        // Debit-side item worth more, or credit-side item worth less: DR control.
        let control_debit = (row.side == "debit") == (row.delta > 0);
        let (d, c) = if control_debit {
            (row.magnitude, 0)
        } else {
            (0, row.magnitude)
        };
        let lines = [
            SourceLine {
                side: Side::Account(row.account_id),
                debit: d,
                credit: c,
                relation_id: row.relation_id,
                bank_account_id: None,
            },
            SourceLine {
                side: Side::Role("unrealized_fx"),
                debit: c,
                credit: d,
                relation_id: None,
                bank_account_id: None,
            },
        ];
        let entry = post_lines(&mut tx, ws, SOURCE, (date, &reason, by), &lines, effect).await?;
        sqlx::query(
            "INSERT INTO accounting_carrying_adjustments (workspace_id, item_id, accounting_date,
                 functional_delta, kind, effect_id, created_by)
             VALUES ($1, $2, $3, $4, 'revaluation', $5, $6)",
        )
        .bind(ws)
        .bind(row.id)
        .bind(date)
        .bind(row.delta)
        .bind(effect)
        .bind(by)
        .execute(&mut *tx)
        .await?;
        item.journal_id = Some(entry);
    }
    sqlx::query(
        "UPDATE accounting_periods SET version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(period_id)
    .execute(&mut *tx)
    .await?;
    p.resource.id = Some(run);
    let answer = ok(&p.resource);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[period_id, run]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Latest run of each of `periods`.
pub(crate) async fn latest(
    conn: &mut PgConnection,
    ws: Uuid,
    periods: &[Uuid],
) -> Result<Vec<(Uuid, RevaluationRef)>, AppError> {
    let rows: Vec<(Uuid, Uuid, NaiveDate, NaiveDate, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as(
            "SELECT DISTINCT ON (period_id) period_id, id, rate_date, accounting_date, created_at
             FROM accounting_revaluations
             WHERE workspace_id = $1 AND period_id = ANY($2)
             ORDER BY period_id, created_at DESC, id DESC",
        )
        .bind(ws)
        .bind(periods)
        .fetch_all(conn)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(p, id, rate_date, accounting_date, created_at)| {
            (
                p,
                RevaluationRef {
                    id,
                    rate_date,
                    accounting_date,
                    created_at,
                },
            )
        })
        .collect())
}

/// "Revaluation done for period" (for period close): true when no open foreign monetary item
/// needs it (none, or all left out), or when the latest run, recomputed at its rate date, has
/// nothing left to post. A missing rate or closed period surfaces as its 422.
pub(crate) async fn current(
    conn: &mut PgConnection,
    ws: Uuid,
    period_id: Uuid,
) -> Result<bool, AppError> {
    let run = latest(&mut *conn, ws, &[period_id]).await?;
    match run.first() {
        Some((_, r)) => Ok(prepare(conn, ws, period_id, r.rate_date, false)
            .await?
            .rows
            .is_empty()),
        None => {
            let functional = active_book(&mut *conn, ws).await?;
            let last = last_day(period(&mut *conn, ws, period_id, false).await?.end_date);
            let found = candidates(conn, ws, functional.id, last, last, false).await?;
            Ok(found.iter().all(|c| c.settled_after || c.revalued_after))
        }
    }
}
