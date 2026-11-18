//! Read-only accounting reports (§15): trial balance, general ledger, P&L, balance sheet.
//! Posted journals only, stored functional amounts, workspace-scoped. All sums, signs and
//! formatting happen in SQL (`numeric`); amounts leave as exact major-unit strings.

use chrono::NaiveDate;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    error::AppError,
    http::list::{Page, Paginated},
    modules::accounting::{
        dto::{
            AmountRow, BalanceSheetResource, CurrencyRef, GeneralLedgerResource, LedgerLine,
            ProfitAndLossResource, Range, ReportAccount, TrialBalanceColumns, TrialBalanceResource,
            TrialBalanceRow,
        },
        repo,
        service::not_configured,
    },
};

/// Functional currency; 422 `accounting_not_configured` when the book has none.
async fn functional(conn: &mut PgConnection, ws: Uuid) -> Result<CurrencyRef, AppError> {
    let id = repo::settings(&mut *conn, ws, false)
        .await?
        .functional_currency_id
        .ok_or_else(not_configured)?;
    repo::currency(&mut *conn, ws, id)
        .await?
        .ok_or_else(not_configured)
}

fn account(id: Uuid, code: String, name: String, kind: String) -> ReportAccount {
    ReportAccount {
        id,
        code,
        name,
        kind,
    }
}

/// Accounts with posted lines before `to`: balance before `from`, activity in `[from, to)`.
pub async fn trial_balance(
    conn: &mut PgConnection,
    ws: Uuid,
    range: Range,
) -> Result<TrialBalanceResource, AppError> {
    let currency = functional(conn, ws).await?;
    // Grouping set `()` adds the totals row, present even for an empty book.
    let rows = sqlx::query!(
        r#"WITH l AS (
               SELECT jl.account_id,
                      coalesce(sum(jl.functional_debit - jl.functional_credit)
                               FILTER (WHERE je.accounting_date < $2), 0) AS opening,
                      coalesce(sum(jl.functional_debit) FILTER (WHERE je.accounting_date >= $2), 0) AS debit,
                      coalesce(sum(jl.functional_credit) FILTER (WHERE je.accounting_date >= $2), 0) AS credit
               FROM journal_lines jl
               JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
               WHERE jl.workspace_id = $1 AND je.state = 'posted' AND je.accounting_date < $3
               GROUP BY jl.account_id),
           r AS (
               SELECT a.id, a.code, a.name, a.type,
                      greatest(l.opening, 0) AS od, greatest(-l.opening, 0) AS oc, l.debit, l.credit,
                      greatest(l.opening + l.debit - l.credit, 0) AS cd,
                      greatest(-(l.opening + l.debit - l.credit), 0) AS cc
               FROM l JOIN accounts a ON a.workspace_id = $1 AND a.id = l.account_id),
           t AS (
               SELECT id, code, name, type, GROUPING(id) AS total,
                      coalesce(sum(od), 0) AS od, coalesce(sum(oc), 0) AS oc,
                      coalesce(sum(debit), 0) AS debit, coalesce(sum(credit), 0) AS credit,
                      coalesce(sum(cd), 0) AS cd, coalesce(sum(cc), 0) AS cc
               FROM r GROUP BY GROUPING SETS ((id, code, name, type), ()))
           SELECT id AS "id?", code AS "code?", name AS "name?", type AS "type?", total AS "total!",
                  round(od / 10::numeric ^ $4::int, $4::int)::text AS "opening_debit!",
                  round(oc / 10::numeric ^ $4::int, $4::int)::text AS "opening_credit!",
                  round(debit / 10::numeric ^ $4::int, $4::int)::text AS "debit!",
                  round(credit / 10::numeric ^ $4::int, $4::int)::text AS "credit!",
                  round(cd / 10::numeric ^ $4::int, $4::int)::text AS "closing_debit!",
                  round(cc / 10::numeric ^ $4::int, $4::int)::text AS "closing_credit!"
           FROM t ORDER BY total, code"#,
        ws,
        range.start,
        range.end,
        i32::from(currency.decimals),
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    let mut totals = None;
    for r in rows {
        let cols = TrialBalanceColumns {
            opening_debit: r.opening_debit,
            opening_credit: r.opening_credit,
            debit: r.debit,
            credit: r.credit,
            closing_debit: r.closing_debit,
            closing_credit: r.closing_credit,
        };
        match (r.total, r.id, r.code, r.name, r.r#type) {
            (0, Some(id), Some(code), Some(name), Some(kind)) => out.push(TrialBalanceRow {
                account: account(id, code, name, kind),
                columns: cols,
            }),
            _ => totals = Some(cols),
        }
    }
    Ok(TrialBalanceResource {
        functional_currency: currency,
        from: range.start,
        to: range.end,
        rows: out,
        totals: totals.ok_or_else(|| AppError::Internal("report totals row missing".into()))?,
    })
}

/// One account's posted lines in `[from, to)`, oldest first, with a running balance
/// (debit-positive) that starts from the balance before `from`. 404 for foreign accounts.
pub async fn general_ledger(
    conn: &mut PgConnection,
    ws: Uuid,
    account_id: Uuid,
    range: Range,
    page: Page,
) -> Result<GeneralLedgerResource, AppError> {
    let currency = functional(conn, ws).await?;
    let dp = i32::from(currency.decimals);
    let head = sqlx::query!(
        r#"SELECT a.id, a.code, a.name, a.type, t.n AS "n!",
                  round(t.o / 10::numeric ^ $5::int, $5::int)::text AS "opening!",
                  round(t.d / 10::numeric ^ $5::int, $5::int)::text AS "debit!",
                  round(t.c / 10::numeric ^ $5::int, $5::int)::text AS "credit!",
                  round((t.o + t.d - t.c) / 10::numeric ^ $5::int, $5::int)::text AS "closing!"
           FROM accounts a
           CROSS JOIN LATERAL (
               SELECT coalesce(sum(jl.functional_debit - jl.functional_credit)
                               FILTER (WHERE je.accounting_date < $3), 0) AS o,
                      coalesce(sum(jl.functional_debit) FILTER (WHERE je.accounting_date >= $3), 0) AS d,
                      coalesce(sum(jl.functional_credit) FILTER (WHERE je.accounting_date >= $3), 0) AS c,
                      count(*) FILTER (WHERE je.accounting_date >= $3) AS n
               FROM journal_lines jl
               JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
               WHERE jl.workspace_id = $1 AND jl.account_id = a.id AND je.state = 'posted'
                 AND je.accounting_date < $4) t
           WHERE a.workspace_id = $1 AND a.id = $2"#,
        ws,
        account_id,
        range.start,
        range.end,
        dp,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    // Window runs over the whole range before LIMIT, so balances continue across pages.
    let lines = sqlx::query!(
        r#"WITH r AS (
               SELECT je.id AS entry_id, je.number, je.kind, je.accounting_date, je.reason,
                      jl.line_no, jl.relation_id, jl.source_ref,
                      jl.functional_debit, jl.functional_credit,
                      sum(jl.functional_debit - jl.functional_credit)
                          OVER (ORDER BY je.accounting_date, je.number, jl.line_no) AS run
               FROM journal_lines jl
               JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
               WHERE jl.workspace_id = $1 AND jl.account_id = $2 AND je.state = 'posted'
                 AND je.accounting_date >= $3 AND je.accounting_date < $4),
           o AS (
               SELECT coalesce(sum(jl.functional_debit - jl.functional_credit), 0) AS opening
               FROM journal_lines jl
               JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
               WHERE jl.workspace_id = $1 AND jl.account_id = $2 AND je.state = 'posted'
                 AND je.accounting_date < $3)
           SELECT r.entry_id, r.number AS "number!", r.kind, r.accounting_date, r.reason,
                  r.line_no, r.relation_id, r.source_ref,
                  round(r.functional_debit / 10::numeric ^ $5::int, $5::int)::text AS "debit!",
                  round(r.functional_credit / 10::numeric ^ $5::int, $5::int)::text AS "credit!",
                  round((o.opening + r.run) / 10::numeric ^ $5::int, $5::int)::text AS "balance!"
           FROM r CROSS JOIN o
           ORDER BY r.accounting_date, r.number, r.line_no
           LIMIT $6 OFFSET $7"#,
        ws,
        account_id,
        range.start,
        range.end,
        dp,
        page.size,
        page.offset(),
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|l| LedgerLine {
        entry_id: l.entry_id,
        number: l.number,
        kind: l.kind,
        accounting_date: l.accounting_date,
        reason: l.reason,
        line_no: l.line_no,
        relation_id: l.relation_id,
        source_ref: l.source_ref,
        debit: l.debit,
        credit: l.credit,
        balance: l.balance,
    })
    .collect();
    Ok(GeneralLedgerResource {
        functional_currency: currency,
        account: account(head.id, head.code, head.name, head.r#type),
        from: range.start,
        to: range.end,
        opening_balance: head.opening,
        debit: head.debit,
        credit: head.credit,
        closing_balance: head.closing,
        lines: Paginated::new(lines, head.n, page),
    })
}

/// Income and expense in `[from, to)`; the typed year-end close transfer is left out.
pub async fn profit_and_loss(
    conn: &mut PgConnection,
    ws: Uuid,
    range: Range,
) -> Result<ProfitAndLossResource, AppError> {
    let currency = functional(conn, ws).await?;
    let rows = sqlx::query!(
        r#"WITH r AS (
               SELECT a.id, a.code, a.name, a.type,
                      CASE a.type WHEN 'income'
                          THEN sum(jl.functional_credit - jl.functional_debit)
                          ELSE sum(jl.functional_debit - jl.functional_credit) END AS amount
               FROM journal_lines jl
               JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
               JOIN accounts a ON a.workspace_id = jl.workspace_id AND a.id = jl.account_id
               WHERE jl.workspace_id = $1 AND je.state = 'posted' AND je.kind <> 'year_end_close'
                 AND je.accounting_date >= $2 AND je.accounting_date < $3
                 AND a.type IN ('income', 'expense')
               GROUP BY a.id),
           t AS (
               SELECT id, code, name, type, GROUPING(id) AS total,
                      coalesce(sum(amount), 0) AS amount,
                      coalesce(sum(amount) FILTER (WHERE type = 'income'), 0) AS income,
                      coalesce(sum(amount) FILTER (WHERE type = 'expense'), 0) AS expenses
               FROM r GROUP BY GROUPING SETS ((id, code, name, type), ()))
           SELECT id AS "id?", code AS "code?", name AS "name?", type AS "type?", total AS "total!",
                  round(amount / 10::numeric ^ $4::int, $4::int)::text AS "amount!",
                  round(income / 10::numeric ^ $4::int, $4::int)::text AS "income!",
                  round(expenses / 10::numeric ^ $4::int, $4::int)::text AS "expenses!",
                  round((income - expenses) / 10::numeric ^ $4::int, $4::int)::text AS "net!"
           FROM t ORDER BY total, code"#,
        ws,
        range.start,
        range.end,
        i32::from(currency.decimals),
    )
    .fetch_all(&mut *conn)
    .await?;
    let (mut income, mut expenses, mut totals) = (Vec::new(), Vec::new(), None);
    for r in rows {
        match (r.total, r.id, r.code, r.name, r.r#type) {
            (0, Some(id), Some(code), Some(name), Some(kind)) => {
                let list = if kind == "income" {
                    &mut income
                } else {
                    &mut expenses
                };
                list.push(AmountRow {
                    account: account(id, code, name, kind),
                    amount: r.amount,
                });
            }
            _ => totals = Some((r.income, r.expenses, r.net)),
        }
    }
    let (total_income, total_expenses, net_profit) =
        totals.ok_or_else(|| AppError::Internal("report totals row missing".into()))?;
    Ok(ProfitAndLossResource {
        functional_currency: currency,
        from: range.start,
        to: range.end,
        income,
        expenses,
        total_income,
        total_expenses,
        net_profit,
    })
}

/// Balances on `as_of` (inclusive), every posted kind. `current_earnings` = income − expense
/// not yet moved by a year-end close; a close shifts it into equity, so it counts once.
pub async fn balance_sheet(
    conn: &mut PgConnection,
    ws: Uuid,
    as_of: NaiveDate,
) -> Result<BalanceSheetResource, AppError> {
    let currency = functional(conn, ws).await?;
    let rows = sqlx::query!(
        r#"WITH r AS (
               SELECT a.id, a.code, a.name, a.type,
                      CASE a.type WHEN 'asset'
                          THEN sum(jl.functional_debit - jl.functional_credit)
                          ELSE sum(jl.functional_credit - jl.functional_debit) END AS amount
               FROM journal_lines jl
               JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
               JOIN accounts a ON a.workspace_id = jl.workspace_id AND a.id = jl.account_id
               WHERE jl.workspace_id = $1 AND je.state = 'posted' AND je.accounting_date <= $2
               GROUP BY a.id),
           t AS (
               SELECT id, code, name, type, GROUPING(id) AS total,
                      coalesce(sum(amount), 0) AS amount,
                      coalesce(sum(amount) FILTER (WHERE type = 'asset'), 0) AS assets,
                      coalesce(sum(amount) FILTER (WHERE type = 'liability'), 0) AS liabilities,
                      coalesce(sum(amount) FILTER (WHERE type = 'equity'), 0) AS equity,
                      coalesce(sum(amount) FILTER (WHERE type IN ('income', 'expense')), 0) AS earnings
               FROM r GROUP BY GROUPING SETS ((id, code, name, type), ()))
           SELECT id AS "id?", code AS "code?", name AS "name?", type AS "type?", total AS "total!",
                  round(amount / 10::numeric ^ $3::int, $3::int)::text AS "amount!",
                  round(assets / 10::numeric ^ $3::int, $3::int)::text AS "assets!",
                  round(liabilities / 10::numeric ^ $3::int, $3::int)::text AS "liabilities!",
                  round(equity / 10::numeric ^ $3::int, $3::int)::text AS "equity!",
                  round(earnings / 10::numeric ^ $3::int, $3::int)::text AS "earnings!",
                  round((liabilities + equity + earnings) / 10::numeric ^ $3::int, $3::int)::text
                      AS "liabilities_and_equity!"
           FROM t
           WHERE total = 1 OR type IN ('asset', 'liability', 'equity')
           ORDER BY total, code"#,
        ws,
        as_of,
        i32::from(currency.decimals),
    )
    .fetch_all(&mut *conn)
    .await?;
    let (mut assets, mut liabilities, mut equity, mut totals) =
        (Vec::new(), Vec::new(), Vec::new(), None);
    for r in rows {
        match (r.total, r.id, r.code, r.name, r.r#type) {
            (0, Some(id), Some(code), Some(name), Some(kind)) => {
                let list = match kind.as_str() {
                    "asset" => &mut assets,
                    "liability" => &mut liabilities,
                    _ => &mut equity,
                };
                list.push(AmountRow {
                    account: account(id, code, name, kind),
                    amount: r.amount,
                });
            }
            _ => {
                totals = Some((
                    r.assets,
                    r.liabilities,
                    r.equity,
                    r.earnings,
                    r.liabilities_and_equity,
                ))
            }
        }
    }
    let (total_assets, total_liabilities, total_equity, current_earnings, total_le) =
        totals.ok_or_else(|| AppError::Internal("report totals row missing".into()))?;
    Ok(BalanceSheetResource {
        functional_currency: currency,
        as_of,
        assets,
        liabilities,
        equity,
        total_assets,
        total_liabilities,
        total_equity,
        current_earnings,
        total_liabilities_and_equity: total_le,
    })
}
