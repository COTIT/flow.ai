//! Period close, year-end close and close checks (masterplan §13 "PERIOD CLOSE" / "YEAR-END —
//! FINAL ORDER", §15). Checks run as of the period's last day; a close takes the exclusive
//! book gate, locks the period (and year) FOR UPDATE and rechecks under lock. The final period
//! of a year closes only through the year-end close, which posts one `year_end_close` journal
//! (income/expense → `retained_earnings`) while that period is open, then closes period and
//! year in the same transaction. No reopen or year reversal (§13, §15 list none).

use axum::http::StatusCode;
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
    modules::accounting::{
        dto::{
            Action, CloseCheck, CloseChecksResource, ClosePayload, CurrencyRef, PostPayload,
            YearCloseLine, YearCloseResource,
        },
        gate,
        journals::{SourceLine, post_lines},
        repo, reports, revaluation,
        service::{not_configured, ok, period_closed, period_resource, stale, unsupported},
        settlement::Side,
    },
};

/// Effect source kind and journal kind of a year-end close.
const YEAR_END: &str = "year_end_close";

#[derive(FromRow)]
struct PeriodRow {
    id: Uuid,
    fiscal_year_id: Uuid,
    start_date: NaiveDate,
    end_date: NaiveDate,
    open: bool,
    version: i64,
    final_period: bool,
}

#[derive(FromRow)]
struct YearRow {
    start_date: NaiveDate,
    end_date: NaiveDate,
    state: String,
    version: i64,
}

#[derive(FromRow)]
struct CloseRow {
    account_id: Uuid,
    debit: i64,
    credit: i64,
}

/// Last day of a `[start, end)` range.
fn last_day(end: NaiveDate) -> NaiveDate {
    end.checked_sub_days(Days::new(1)).unwrap_or(end)
}

/// Minor units as an exact decimal string at `dp` places (display only).
fn amount(minor: i64, dp: i16) -> String {
    Decimal::new(minor, u32::try_from(dp).unwrap_or(0)).to_string()
}

/// The functional currency of an active book; 422 `accounting_not_configured` otherwise.
async fn active_book(conn: &mut PgConnection, ws: Uuid) -> Result<CurrencyRef, AppError> {
    let s = repo::settings(&mut *conn, ws, false).await?;
    let id = s
        .functional_currency_id
        .filter(|_| s.state == "active")
        .ok_or_else(not_configured)?;
    repo::currency(&mut *conn, ws, id)
        .await?
        .ok_or_else(not_configured)
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
                "SELECT p.id, p.fiscal_year_id, p.start_date, p.end_date,
                        p.state = 'open' AND y.state = 'open' AS open, p.version,
                        p.end_date = y.end_date AS final_period
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

/// The §13 checks of `p` as of its last day, in a fixed order.
async fn checks(
    conn: &mut PgConnection,
    ws: Uuid,
    p: &PeriodRow,
) -> Result<Vec<CloseCheck>, AppError> {
    let last = last_day(p.end_date);
    let counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "WITH s AS (SELECT timezone FROM accounting_settings WHERE workspace_id = $1),
         o AS (SELECT o.id FROM provider_observations o CROSS JOIN s
               WHERE o.workspace_id = $1
                 AND (o.occurred_at AT TIME ZONE s.timezone)::date <= $4),
         -- Each open item's functional carrying rebuilt as of the cutoff (as receivables aging).
         i AS (
             SELECT x.account_id,
                    CASE x.side WHEN 'debit' THEN 1 ELSE -1 END
                    * (x.functional_amount - coalesce(r.functional, 0) + coalesce(c.delta, 0))
                        AS carrying
             FROM accounting_open_items x
             LEFT JOIN LATERAL (
                 SELECT sum(CASE WHEN a.reversal_of IS NULL THEN 1 ELSE -1 END
                            * CASE WHEN a.source_item_id = x.id THEN a.source_functional
                                   ELSE a.target_functional END) AS functional
                 FROM accounting_allocations a
                 WHERE a.workspace_id = x.workspace_id AND a.accounting_date <= $4
                   AND (a.source_item_id = x.id
                        OR (a.target_item_id = x.id AND a.kind <> 'reclassification'))) r ON true
             LEFT JOIN LATERAL (
                 SELECT sum(d.functional_delta) AS delta FROM accounting_carrying_adjustments d
                 WHERE d.workspace_id = x.workspace_id AND d.item_id = x.id
                   AND d.accounting_date <= $4) c ON true
             WHERE x.workspace_id = $1 AND x.issue_date <= $4),
         b AS (SELECT account_id, sum(carrying) AS items FROM i GROUP BY account_id),
         g AS (
             SELECT jl.account_id, sum(jl.functional_debit - jl.functional_credit) AS gl
             FROM journal_lines jl
             JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
             WHERE jl.workspace_id = $1 AND je.state = 'posted' AND je.accounting_date <= $4
               AND jl.account_id IN (SELECT account_id FROM b)
             GROUP BY jl.account_id)
         SELECT
           (SELECT count(*) FROM accounting_source_links l JOIN o ON o.id = l.source_id
            WHERE l.workspace_id = $1 AND l.source_kind = 'provider_observation'
              AND l.disposition = 'pending_observation'),
           (SELECT count(*) FROM (
                SELECT e.id FROM journal_entries e
                JOIN journal_lines l ON l.workspace_id = e.workspace_id AND l.entry_id = e.id
                WHERE e.workspace_id = $1 AND e.state = 'posted' AND e.period_id = $2
                GROUP BY e.id
                HAVING sum(l.functional_debit) <> sum(l.functional_credit)) u),
           (SELECT count(*) FROM b LEFT JOIN g USING (account_id)
            WHERE b.items <> coalesce(g.gl, 0)),
           (SELECT count(*) FROM purchase_receipt_lines r
            WHERE r.workspace_id = $1 AND r.booked_at IS NULL
              AND r.effective_date >= $3 AND r.effective_date < $5),
           (SELECT CASE WHEN coalesce(sum(jl.functional_debit - jl.functional_credit), 0) <> 0
                        THEN 1 ELSE 0 END::bigint
            FROM journal_lines jl
            JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
            JOIN account_mappings m ON m.workspace_id = jl.workspace_id
                                   AND m.account_id = jl.account_id
                                   AND m.role = 'pending_purchase_cost_variance'
            WHERE jl.workspace_id = $1 AND je.state = 'posted' AND je.accounting_date <= $4),
           (SELECT count(*) FROM accounting_exceptions x JOIN o ON o.id = x.observation_id
            WHERE x.workspace_id = $1 AND x.state = 'pending')",
    )
    .bind(ws)
    .bind(p.id)
    .bind(p.start_date)
    .bind(last)
    .bind(p.end_date)
    .fetch_one(&mut *conn)
    .await?;
    let inventory = reports::inventory_valuation(&mut *conn, ws, last, None)
        .await?
        .difference
        .and_then(|d| d.parse::<Decimal>().ok())
        .is_none_or(|d| d.is_zero());
    let fx = match revaluation::current(&mut *conn, ws, p.id).await {
        Ok(done) => done,
        // A missing closing rate is a failing check, not a refused report.
        Err(AppError::Validation(_)) => false,
        Err(e) => return Err(e),
    };
    let (effects, unbalanced, control, receipts, variance, exceptions) = counts;
    let flag = |ok: bool| (ok, i64::from(!ok));
    let rows = [
        ("missing_effects", (effects == 0, effects)),
        ("unbalanced_journals", (unbalanced == 0, unbalanced)),
        ("inventory_difference", flag(inventory)),
        ("open_item_control_difference", (control == 0, control)),
        ("unbooked_receipts", (receipts == 0, receipts)),
        ("pending_cost_variance", (variance == 0, variance)),
        ("fx_revaluation", flag(fx)),
        ("pending_exceptions", (exceptions == 0, exceptions)),
    ];
    Ok(rows
        .into_iter()
        .map(|(code, (passed, count))| CloseCheck {
            code,
            passed,
            count,
        })
        .collect())
}

/// 422 for failing checks: the first failing check's documented reason (§5), every failing
/// code in the message. None when all pass.
fn refusal(checks: &[CloseCheck]) -> Option<AppError> {
    let failing: Vec<&CloseCheck> = checks.iter().filter(|c| !c.passed).collect();
    let first = failing.first()?;
    let code = match first.code {
        "unbooked_receipts" => "receipt_booking_required",
        "missing_effects" | "pending_exceptions" => "external_financial_fact_pending",
        _ => "unsupported_active_erp_operation",
    };
    let names: Vec<&str> = failing.iter().map(|c| c.code).collect();
    Some(AppError::Reason {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code,
        message: format!(
            "The period can't close until these checks pass: {}.",
            names.join(", ")
        ),
    })
}

fn checks_fingerprint(p: &PeriodRow, checks: &[CloseCheck]) -> String {
    let facts = json!({
        "period": p.id, "version": p.version,
        "checks": checks.iter().map(|c| json!([c.code, c.passed, c.count])).collect::<Vec<_>>(),
    });
    hex::encode(Sha256::digest(facts.to_string().as_bytes()))
}

/// The checks report of an open period of an active book.
async fn report(
    conn: &mut PgConnection,
    ws: Uuid,
    period_id: Uuid,
    lock: bool,
) -> Result<(PeriodRow, CloseChecksResource), AppError> {
    let p = period(&mut *conn, ws, period_id, lock).await?;
    active_book(&mut *conn, ws).await?;
    if !p.open {
        return Err(period_closed());
    }
    let checks = checks(&mut *conn, ws, &p).await?;
    let resource = CloseChecksResource {
        period_id: p.id,
        period_version: p.version,
        fiscal_year_id: p.fiscal_year_id,
        accounting_date: last_day(p.end_date),
        final_period: p.final_period,
        ready: checks.iter().all(|c| c.passed),
        checks_fingerprint: checks_fingerprint(&p, &checks),
        checks,
    };
    Ok((p, resource))
}

/// GET /accounting/reports/close-checks: the period's checks; no writes.
pub async fn close_checks(
    state: &AppState,
    ws: Uuid,
    period_id: Uuid,
) -> Result<CloseChecksResource, AppError> {
    let mut tx = state.db.begin().await?;
    let (_, r) = report(&mut tx, ws, period_id, false).await?;
    tx.rollback().await?;
    Ok(r)
}

/// POST /accounting/periods/{id}/close: close an ordinary period whose checks pass.
pub async fn close_period(
    state: &AppState,
    access: &WorkspaceAccess,
    period_id: Uuid,
    cmd: Command<Action<ClosePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_periods_close";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::exclusive(&mut tx, ws).await?;
    let p = period(&mut tx, ws, period_id, true).await?;
    active_book(&mut tx, ws).await?;
    if !p.open {
        return Err(period_closed());
    }
    if p.final_period {
        return Err(unsupported(
            "This is the final period of its fiscal year. Close it with the year-end close.",
        ));
    }
    let (_, r) = report(&mut tx, ws, period_id, true).await?;
    if r.period_version != cmd.input.expected_version || r.checks_fingerprint != cmd.input.payload {
        return Err(stale());
    }
    if let Some(e) = refusal(&r.checks) {
        return Err(e);
    }
    close_row(&mut tx, ws, "accounting_periods", period_id).await?;
    let row = repo::periods(&mut *tx, ws, Some(period_id), 1, 0)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    let last = revaluation::latest(&mut tx, ws, &[period_id])
        .await?
        .pop()
        .map(|(_, r)| r);
    let answer = ok(&period_resource(row, ws, last));
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[period_id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Mark a period or fiscal year closed (state + version).
async fn close_row(
    conn: &mut PgConnection,
    ws: Uuid,
    table: &str,
    id: Uuid,
) -> Result<(), AppError> {
    let sql = if table == "accounting_periods" {
        "UPDATE accounting_periods SET state = 'closed', version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2"
    } else {
        "UPDATE accounting_fiscal_years SET state = 'closed', version = version + 1,
                updated_at = now()
         WHERE workspace_id = $1 AND id = $2"
    };
    sqlx::query(sql).bind(ws).bind(id).execute(conn).await?;
    Ok(())
}

struct YearPrepared {
    resource: YearCloseResource,
    rows: Vec<CloseRow>,
    /// (retained earnings debit, credit).
    net: (i64, i64),
}

/// Year-end facts (no writes): calendar, earlier periods closed, checks of the open final
/// period, closing lines (SQL), fingerprint. Locks year and periods FOR UPDATE when `lock`.
async fn prepare_year(
    conn: &mut PgConnection,
    ws: Uuid,
    year_id: Uuid,
    lock: bool,
) -> Result<YearPrepared, AppError> {
    let sql = if lock {
        "SELECT start_date, end_date, state, version FROM accounting_fiscal_years
         WHERE workspace_id = $1 AND id = $2 FOR UPDATE"
    } else {
        "SELECT start_date, end_date, state, version FROM accounting_fiscal_years
         WHERE workspace_id = $1 AND id = $2"
    };
    let y: YearRow = sqlx::query_as(sql)
        .bind(ws)
        .bind(year_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    let functional = active_book(&mut *conn, ws).await?;
    if y.state != "open" {
        return Err(period_closed());
    }
    let sql = if lock {
        "SELECT id, end_date, start_date, state FROM accounting_periods
         WHERE workspace_id = $1 AND fiscal_year_id = $2 ORDER BY start_date FOR UPDATE"
    } else {
        "SELECT id, end_date, start_date, state FROM accounting_periods
         WHERE workspace_id = $1 AND fiscal_year_id = $2 ORDER BY start_date"
    };
    let periods: Vec<(Uuid, NaiveDate, NaiveDate, String)> = sqlx::query_as(sql)
        .bind(ws)
        .bind(year_id)
        .fetch_all(&mut *conn)
        .await?;
    let mut next = y.start_date;
    for (_, end, start, _) in &periods {
        if *start != next {
            break;
        }
        next = *end;
    }
    let Some((final_id, ..)) = periods.last().filter(|_| next == y.end_date) else {
        return Err(unsupported(
            "The fiscal year's periods must cover it without gaps before it can close.",
        ));
    };
    if periods[..periods.len() - 1]
        .iter()
        .any(|(.., state)| state != "closed")
    {
        return Err(unsupported(
            "Close every earlier period of the fiscal year before the year-end close.",
        ));
    }
    let (fp, report) = report(&mut *conn, ws, *final_id, lock).await?;
    // Money: each income/expense account's year balance reversed, net to retained earnings.
    let rows: Vec<(Uuid, i64, i64, i64, i64)> = sqlx::query_as(
        "WITH b AS (
             SELECT a.id, a.code, sum(jl.functional_debit - jl.functional_credit) AS balance
             FROM journal_lines jl
             JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
             JOIN accounts a ON a.workspace_id = jl.workspace_id AND a.id = jl.account_id
             WHERE jl.workspace_id = $1 AND je.state = 'posted'
               AND je.accounting_date >= $2 AND je.accounting_date < $3
               AND a.type IN ('income', 'expense')
             GROUP BY a.id, a.code
             HAVING sum(jl.functional_debit - jl.functional_credit) <> 0)
         SELECT id,
                (CASE WHEN balance < 0 THEN -balance ELSE 0 END)::bigint,
                (CASE WHEN balance > 0 THEN balance ELSE 0 END)::bigint,
                (CASE WHEN sum(balance) OVER () > 0 THEN sum(balance) OVER () ELSE 0 END)::bigint,
                (CASE WHEN sum(balance) OVER () < 0 THEN -sum(balance) OVER () ELSE 0 END)::bigint
         FROM b ORDER BY code, id",
    )
    .bind(ws)
    .bind(y.start_date)
    .bind(y.end_date)
    .fetch_all(&mut *conn)
    .await?;
    let net = rows.first().map_or((0, 0), |r| (r.3, r.4));
    let rows: Vec<CloseRow> = rows
        .into_iter()
        .map(|(account_id, debit, credit, ..)| CloseRow {
            account_id,
            debit,
            credit,
        })
        .collect();
    let dp = functional.decimals;
    let mut lines: Vec<YearCloseLine> = rows
        .iter()
        .map(|r| YearCloseLine {
            account_id: r.account_id,
            debit: amount(r.debit, dp),
            credit: amount(r.credit, dp),
        })
        .collect();
    if net != (0, 0) {
        let retained = repo::mappings(&mut *conn, ws)
            .await?
            .into_iter()
            .find(|m| m.role == "retained_earnings")
            .map(|m| m.account_id)
            .ok_or_else(not_configured)?;
        lines.push(YearCloseLine {
            account_id: retained,
            debit: amount(net.0, dp),
            credit: amount(net.1, dp),
        });
    }
    let facts = json!({
        "year": year_id, "version": y.version, "final": fp.id, "final_version": fp.version,
        "checks": report.checks.iter().map(|c| json!([c.code, c.passed, c.count]))
            .collect::<Vec<_>>(),
        "lines": rows.iter().map(|r| json!([r.account_id, r.debit, r.credit]))
            .collect::<Vec<_>>(),
    });
    Ok(YearPrepared {
        resource: YearCloseResource {
            fiscal_year_id: year_id,
            fiscal_year_version: y.version,
            final_period_id: fp.id,
            accounting_date: last_day(fp.end_date),
            net_income: amount(net.1 - net.0, dp),
            functional_currency: functional,
            checks: report.checks,
            lines,
            review_fingerprint: hex::encode(Sha256::digest(facts.to_string().as_bytes())),
            journal_id: None,
            effect_id: None,
        },
        rows,
        net,
    })
}

/// POST /accounting/fiscal-years/{id}/close/review: the close as it would post; no writes.
pub async fn year_review(
    state: &AppState,
    ws: Uuid,
    year_id: Uuid,
) -> Result<YearCloseResource, AppError> {
    let mut tx = state.db.begin().await?;
    let p = prepare_year(&mut tx, ws, year_id, false).await?;
    tx.rollback().await?;
    Ok(p.resource)
}

/// POST /accounting/fiscal-years/{id}/close: post the reviewed closing journal while the final
/// period is open, then close that period and the year, atomically.
pub async fn close_year(
    state: &AppState,
    access: &WorkspaceAccess,
    year_id: Uuid,
    cmd: Command<Action<PostPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "accounting_year_close";
    let ws = access.workspace_id;
    let by = access.user_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, by, &cmd).await? {
        return Ok(a);
    }
    gate::exclusive(&mut tx, ws).await?;
    let mut p = prepare_year(&mut tx, ws, year_id, true).await?;
    if p.resource.fiscal_year_version != cmd.input.expected_version
        || p.resource.review_fingerprint != cmd.input.payload
    {
        return Err(stale());
    }
    if let Some(e) = refusal(&p.resource.checks) {
        return Err(e);
    }
    let fingerprint: [u8; 32] = Sha256::digest(p.resource.review_fingerprint.as_bytes()).into();
    let effect =
        repo::insert_effect(&mut tx, ws, YEAR_END, year_id, "posted", &fingerprint, by).await?;
    if !p.rows.is_empty() {
        let mut lines: Vec<SourceLine> = p
            .rows
            .iter()
            .map(|r| SourceLine {
                side: Side::Account(r.account_id),
                debit: r.debit,
                credit: r.credit,
                relation_id: None,
                bank_account_id: None,
            })
            .collect();
        // Income equal to expense: the reversed lines balance on their own.
        lines.extend((p.net != (0, 0)).then_some(SourceLine {
            side: Side::Role("retained_earnings"),
            debit: p.net.0,
            credit: p.net.1,
            relation_id: None,
            bank_account_id: None,
        }));
        let date = p.resource.accounting_date;
        let reason = format!("Year-end close of the fiscal year ending {date}");
        let entry = post_lines(&mut tx, ws, YEAR_END, (date, &reason, by), &lines, effect).await?;
        p.resource.journal_id = Some(entry);
    }
    let final_id = p.resource.final_period_id;
    close_row(&mut tx, ws, "accounting_periods", final_id).await?;
    close_row(&mut tx, ws, "accounting_fiscal_years", year_id).await?;
    p.resource.effect_id = Some(effect);
    p.resource.fiscal_year_version += 1;
    let answer = ok(&p.resource);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[year_id, final_id]).await?;
    tx.commit().await?;
    Ok(answer)
}
