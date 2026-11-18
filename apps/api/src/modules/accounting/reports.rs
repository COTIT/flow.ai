//! Read-only accounting reports (§15): trial balance, general ledger, party ledger, P&L,
//! balance sheet, receivables/payables aging, inventory valuation. Posted facts only, stored
//! functional amounts, workspace-scoped; history rebuilt from dated rows (§13). All sums, signs and
//! formatting happen in SQL (`numeric`); amounts leave as exact major-unit strings.

use chrono::NaiveDate;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    error::AppError,
    http::list::{Page, Paginated},
    modules::accounting::{
        dto::{
            AgingColumns, AgingItem, AgingParty, AgingResource, AmountRow, BalanceSheetResource,
            CurrencyRef, GeneralLedgerResource, InventoryValuationResource, LedgerLine, NamedRef,
            PartyLedgerLine, PartyLedgerResource, ProfitAndLossResource, Range, ReportAccount,
            TrialBalanceColumns, TrialBalanceResource, TrialBalanceRow, ValuationRow,
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

/// Cutover day in the book timezone; None before activation.
async fn cutover(conn: &mut PgConnection, ws: Uuid) -> Result<Option<NaiveDate>, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT (cutover_at AT TIME ZONE timezone)::date FROM accounting_settings
         WHERE workspace_id = $1",
        ws
    )
    .fetch_one(&mut *conn)
    .await?)
}

/// A relation of this workspace by name; 404 otherwise.
async fn relation(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<NamedRef, AppError> {
    let r = sqlx::query!(
        r#"SELECT id, coalesce(company_name, alias, '') AS "name!" FROM relations
           WHERE workspace_id = $1 AND id = $2"#,
        ws,
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(NamedRef {
        id: r.id,
        name: r.name,
    })
}

/// Receivables (`payables` false) or payables aging on `as_of`. Each item's residual is
/// rebuilt from allocations dated ≤ as_of (reversals add back), never today's projection.
/// Opening items count from the cutover day. Parties paged; items only for one relation.
pub async fn aging(
    conn: &mut PgConnection,
    ws: Uuid,
    as_of: NaiveDate,
    relation_id: Option<Uuid>,
    payables: bool,
    page: Page,
) -> Result<AgingResource, AppError> {
    let currency = functional(conn, ws).await?;
    if let Some(id) = relation_id {
        relation(conn, ws, id).await?;
    }
    let classes: &[&str] = if payables {
        &["payable", "supplier_credit", "supplier_advance", "received_not_invoiced"]
    } else {
        // recovery_receivable is a controlled suspense item, not customer AR (§8).
        &["receivable", "customer_credit", "customer_deposit"]
    };
    let classes: Vec<String> = classes.iter().map(|c| (*c).to_owned()).collect();
    let dp = i32::from(currency.decimals);
    let rows = sqlx::query!(
        r#"WITH s AS (SELECT (cutover_at AT TIME ZONE timezone)::date AS cutover
                      FROM accounting_settings WHERE workspace_id = $1),
           i AS (
               SELECT o.id, o.relation_id, o.classification, o.source_kind, o.source_id,
                      o.invoice_id, o.issue_date, o.due_date, o.currency_id, o.amount,
                      $2::date - coalesce(o.due_date, o.issue_date) AS days,
                      CASE WHEN o.classification IN ('customer_credit', 'supplier_credit') THEN 'credits'
                           WHEN o.classification IN ('customer_deposit', 'supplier_advance') THEN 'deposits'
                           WHEN o.classification = 'received_not_invoiced'
                                OR o.account_role = 'unbilled_receivables' THEN 'unbilled'
                           WHEN $2::date - coalesce(o.due_date, o.issue_date) <= 0 THEN 'not_due'
                           WHEN $2::date - coalesce(o.due_date, o.issue_date) <= 30 THEN '1_30'
                           WHEN $2::date - coalesce(o.due_date, o.issue_date) <= 60 THEN '31_60'
                           WHEN $2::date - coalesce(o.due_date, o.issue_date) <= 90 THEN '61_90'
                           ELSE 'over_90' END AS bucket,
                      o.amount - coalesce(r.amount, 0) AS residual,
                      o.functional_amount - coalesce(r.functional, 0) + coalesce(c.delta, 0)
                          AS functional_residual
               FROM accounting_open_items o
               CROSS JOIN s
               -- Relief as of the cutoff: reclassification relieves its source only.
               LEFT JOIN LATERAL (
                   SELECT sum(CASE WHEN a.reversal_of IS NULL THEN 1 ELSE -1 END * a.amount) AS amount,
                          sum(CASE WHEN a.reversal_of IS NULL THEN 1 ELSE -1 END
                              * CASE WHEN a.source_item_id = o.id THEN a.source_functional
                                     ELSE a.target_functional END) AS functional
                   FROM accounting_allocations a
                   WHERE a.workspace_id = o.workspace_id AND a.accounting_date <= $2
                     AND (a.source_item_id = o.id
                          OR (a.target_item_id = o.id AND a.kind <> 'reclassification'))) r ON true
               LEFT JOIN LATERAL (
                   SELECT sum(x.functional_delta) AS delta FROM accounting_carrying_adjustments x
                   WHERE x.workspace_id = o.workspace_id AND x.item_id = o.id
                     AND x.accounting_date <= $2) c ON true
               WHERE o.workspace_id = $1 AND o.issue_date <= $2 AND o.classification = ANY($3)
                 AND ($4::uuid IS NULL OR o.relation_id = $4)
                 AND (o.origin = 'posted' OR $2 >= s.cutover)),
           g AS (
               SELECT grouping(i.relation_id) AS gr, grouping(i.id) AS gi, i.relation_id, i.id,
                      i.classification, i.source_kind, i.source_id, i.invoice_id, i.issue_date,
                      i.due_date, i.days, i.bucket, i.currency_id, i.amount, i.residual,
                      sum(i.functional_residual) AS fres,
                      coalesce(sum(i.functional_residual) FILTER (WHERE i.bucket = 'not_due'), 0) AS nd,
                      coalesce(sum(i.functional_residual) FILTER (WHERE i.bucket = '1_30'), 0) AS b1,
                      coalesce(sum(i.functional_residual) FILTER (WHERE i.bucket = '31_60'), 0) AS b2,
                      coalesce(sum(i.functional_residual) FILTER (WHERE i.bucket = '61_90'), 0) AS b3,
                      coalesce(sum(i.functional_residual) FILTER (WHERE i.bucket = 'over_90'), 0) AS b4,
                      coalesce(sum(i.functional_residual) FILTER (WHERE i.bucket = 'credits'), 0) AS cr,
                      coalesce(sum(i.functional_residual) FILTER (WHERE i.bucket = 'deposits'), 0) AS dep,
                      coalesce(sum(i.functional_residual) FILTER (WHERE i.bucket = 'unbilled'), 0) AS unb
               FROM i WHERE i.residual > 0
               GROUP BY GROUPING SETS ((), (i.relation_id), (i.relation_id, i.id, i.classification,
                   i.source_kind, i.source_id, i.invoice_id, i.issue_date, i.due_date, i.days,
                   i.bucket, i.currency_id, i.amount, i.residual))),
           p AS (
               SELECT g.*, coalesce(rel.company_name, rel.alias, '') AS name,
                      CASE WHEN g.gr = 0 AND g.gi = 1 THEN row_number() OVER (
                          PARTITION BY g.gr = 0 AND g.gi = 1
                          ORDER BY g.relation_id IS NULL, coalesce(rel.company_name, rel.alias, ''),
                                   g.relation_id) END AS rn
               FROM g LEFT JOIN relations rel
                 ON rel.workspace_id = $1 AND rel.id = g.relation_id)
           SELECT p.gr AS "gr!", p.gi AS "gi!", p.relation_id, p.name AS "name!", p.id,
                  p.classification, p.source_kind, p.source_id, p.invoice_id, p.issue_date,
                  p.due_date, p.days, p.bucket,
                  cur.id AS "currency_id?", cur.code AS "currency_code?",
                  cur.decimals AS "currency_decimals?",
                  round(p.amount / 10::numeric ^ cur.decimals, cur.decimals)::text AS amount,
                  round(p.residual / 10::numeric ^ cur.decimals, cur.decimals)::text AS residual,
                  round(p.fres / 10::numeric ^ $5::int, $5::int)::text AS fres,
                  (SELECT count(*) FROM g WHERE g.gr = 0 AND g.gi = 1) AS "parties!",
                  round(p.nd / 10::numeric ^ $5::int, $5::int)::text AS "nd!",
                  round(p.b1 / 10::numeric ^ $5::int, $5::int)::text AS "b1!",
                  round(p.b2 / 10::numeric ^ $5::int, $5::int)::text AS "b2!",
                  round(p.b3 / 10::numeric ^ $5::int, $5::int)::text AS "b3!",
                  round(p.b4 / 10::numeric ^ $5::int, $5::int)::text AS "b4!",
                  round((p.nd + p.b1 + p.b2 + p.b3 + p.b4) / 10::numeric ^ $5::int, $5::int)::text
                      AS "total!",
                  round(p.cr / 10::numeric ^ $5::int, $5::int)::text AS "cr!",
                  round(p.dep / 10::numeric ^ $5::int, $5::int)::text AS "dep!",
                  round(p.unb / 10::numeric ^ $5::int, $5::int)::text AS "unb!"
           FROM p LEFT JOIN currencies cur ON cur.workspace_id = $1 AND cur.id = p.currency_id
           WHERE p.gr = 1
              OR (p.gi = 1 AND p.rn > $6 AND p.rn <= $6 + $7)
              OR (p.gi = 0 AND $4::uuid IS NOT NULL)
           ORDER BY p.gr DESC, p.rn, p.issue_date, p.id"#,
        ws,
        as_of,
        &classes,
        relation_id,
        dp,
        page.offset(),
        page.size,
    )
    .fetch_all(&mut *conn)
    .await?;
    // query! rows have no nameable type: a macro builds the columns from any of them.
    macro_rules! cols {
        ($r:expr) => {
            AgingColumns {
                not_due: $r.nd.clone(),
                d1_30: $r.b1.clone(),
                d31_60: $r.b2.clone(),
                d61_90: $r.b3.clone(),
                over_90: $r.b4.clone(),
                total: $r.total.clone(),
                credits: $r.cr.clone(),
                deposits: $r.dep.clone(),
                unbilled: $r.unb.clone(),
            }
        };
    }
    let total = rows.iter().find(|r| r.gr == 1).ok_or(AppError::NotFound)?;
    let (totals, parties) = (cols!(total), total.parties);
    let party_rows = rows
        .iter()
        .filter(|r| r.gr == 0 && r.gi == 1)
        .map(|r| AgingParty {
            relation: r.relation_id.map(|id| NamedRef {
                id,
                name: r.name.clone(),
            }),
            columns: cols!(r),
        })
        .collect();
    let items = rows
        .iter()
        .filter(|r| r.gi == 0)
        .filter_map(|r| {
            Some(AgingItem {
                id: r.id?,
                classification: r.classification.clone()?,
                source_kind: r.source_kind.clone()?,
                source_id: r.source_id?,
                invoice_id: r.invoice_id,
                issue_date: r.issue_date?,
                due_date: r.due_date,
                days_overdue: r.days?,
                bucket: r.bucket.clone()?,
                currency: CurrencyRef {
                    id: r.currency_id?,
                    code: r.currency_code.clone()?,
                    decimals: r.currency_decimals?,
                },
                amount: r.amount.clone()?,
                residual: r.residual.clone()?,
                functional_residual: r.fres.clone()?,
            })
        })
        .collect();
    Ok(AgingResource {
        functional_currency: currency,
        as_of,
        cutover_date: cutover(conn, ws).await?,
        relation_id,
        totals,
        parties: Paginated::new(party_rows, parties, page),
        items,
    })
}

/// One relation's posted lines in `[from, to)` on every account, oldest first, with a running
/// balance (debit-positive) from the balance before `from`. 404 for foreign relations.
pub async fn party_ledger(
    conn: &mut PgConnection,
    ws: Uuid,
    relation_id: Uuid,
    range: Range,
    page: Page,
) -> Result<PartyLedgerResource, AppError> {
    let currency = functional(conn, ws).await?;
    let party = relation(conn, ws, relation_id).await?;
    let dp = i32::from(currency.decimals);
    let head = sqlx::query!(
        r#"SELECT count(*) FILTER (WHERE je.accounting_date >= $3) AS "n!",
                  round(coalesce(sum(jl.functional_debit - jl.functional_credit)
                        FILTER (WHERE je.accounting_date < $3), 0) / 10::numeric ^ $5::int, $5::int)::text AS "opening!",
                  round(coalesce(sum(jl.functional_debit) FILTER (WHERE je.accounting_date >= $3), 0)
                        / 10::numeric ^ $5::int, $5::int)::text AS "debit!",
                  round(coalesce(sum(jl.functional_credit) FILTER (WHERE je.accounting_date >= $3), 0)
                        / 10::numeric ^ $5::int, $5::int)::text AS "credit!",
                  round(coalesce(sum(jl.functional_debit - jl.functional_credit), 0)
                        / 10::numeric ^ $5::int, $5::int)::text AS "closing!"
           FROM journal_lines jl
           JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
           WHERE jl.workspace_id = $1 AND jl.relation_id = $2 AND je.state = 'posted'
             AND je.accounting_date < $4"#,
        ws,
        relation_id,
        range.start,
        range.end,
        dp,
    )
    .fetch_one(&mut *conn)
    .await?;
    // Window runs over the whole range before LIMIT, so balances continue across pages.
    let lines = sqlx::query!(
        r#"WITH r AS (
               SELECT je.id AS entry_id, je.number, je.kind, je.accounting_date, je.reason,
                      jl.line_no, jl.relation_id, jl.source_ref, jl.account_id,
                      jl.functional_debit, jl.functional_credit,
                      sum(jl.functional_debit - jl.functional_credit)
                          OVER (ORDER BY je.accounting_date, je.number, jl.line_no) AS run
               FROM journal_lines jl
               JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
               WHERE jl.workspace_id = $1 AND jl.relation_id = $2 AND je.state = 'posted'
                 AND je.accounting_date >= $3 AND je.accounting_date < $4),
           o AS (
               SELECT coalesce(sum(jl.functional_debit - jl.functional_credit), 0) AS opening
               FROM journal_lines jl
               JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
               WHERE jl.workspace_id = $1 AND jl.relation_id = $2 AND je.state = 'posted'
                 AND je.accounting_date < $3)
           SELECT r.entry_id, r.number AS "number!", r.kind, r.accounting_date, r.reason,
                  r.line_no, r.relation_id, r.source_ref,
                  a.id AS account_id, a.code, a.name, a.type,
                  round(r.functional_debit / 10::numeric ^ $5::int, $5::int)::text AS "debit!",
                  round(r.functional_credit / 10::numeric ^ $5::int, $5::int)::text AS "credit!",
                  round((o.opening + r.run) / 10::numeric ^ $5::int, $5::int)::text AS "balance!"
           FROM r CROSS JOIN o
           JOIN accounts a ON a.workspace_id = $1 AND a.id = r.account_id
           ORDER BY r.accounting_date, r.number, r.line_no
           LIMIT $6 OFFSET $7"#,
        ws,
        relation_id,
        range.start,
        range.end,
        dp,
        page.size,
        page.offset(),
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|l| PartyLedgerLine {
        account: account(l.account_id, l.code, l.name, l.r#type),
        line: LedgerLine {
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
        },
    })
    .collect();
    Ok(PartyLedgerResource {
        functional_currency: currency,
        relation: party,
        from: range.start,
        to: range.end,
        opening_balance: head.opening,
        debit: head.debit,
        credit: head.credit,
        closing_balance: head.closing,
        lines: Paginated::new(lines, head.n, page),
    })
}

/// Pool quantity and value on `as_of`: the last value entry dated ≤ as_of, else the state
/// before the first entry (opening), else the pool itself. Nothing before the cutover day.
/// Without a warehouse filter also the posted GL balance of the `inventory` role on as_of.
pub async fn inventory_valuation(
    conn: &mut PgConnection,
    ws: Uuid,
    as_of: NaiveDate,
    warehouse_id: Option<Uuid>,
) -> Result<InventoryValuationResource, AppError> {
    let currency = functional(conn, ws).await?;
    if let Some(id) = warehouse_id {
        sqlx::query_scalar!(
            "SELECT id FROM warehouses WHERE workspace_id = $1 AND id = $2",
            ws,
            id
        )
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    }
    let dp = i32::from(currency.decimals);
    // Grouping set `()` is the totals row, present even when no pool qualifies.
    let rows = sqlx::query!(
        r#"WITH s AS (SELECT (cutover_at AT TIME ZONE timezone)::date AS cutover
                      FROM accounting_settings WHERE workspace_id = $1),
           v AS (
               SELECT p.product_id, p.warehouse_id,
                      coalesce(l.quantity_after, f.quantity_before, p.quantity) AS qty,
                      coalesce(l.value_after, f.value_before, p.value) AS value
               FROM inventory_cost_pools p
               CROSS JOIN s
               LEFT JOIN LATERAL (
                   SELECT e.quantity_after, e.value_after FROM inventory_value_entries e
                   WHERE e.workspace_id = p.workspace_id AND e.pool_id = p.id
                     AND e.accounting_date <= $2
                   ORDER BY e.sequence DESC LIMIT 1) l ON true
               LEFT JOIN LATERAL (
                   SELECT e.quantity_before, e.value_before FROM inventory_value_entries e
                   WHERE e.workspace_id = p.workspace_id AND e.pool_id = p.id
                   ORDER BY e.sequence LIMIT 1) f ON true
               WHERE p.workspace_id = $1 AND ($3::uuid IS NULL OR p.warehouse_id = $3)
                 AND $2 >= s.cutover),
           g AS (
               SELECT grouping(v.product_id) AS gr, v.product_id, pr.name AS product,
                      v.warehouse_id, wh.name AS warehouse, v.qty, coalesce(sum(v.value), 0) AS value
               FROM v
               JOIN products pr ON pr.workspace_id = $1 AND pr.id = v.product_id
               JOIN warehouses wh ON wh.workspace_id = $1 AND wh.id = v.warehouse_id
               WHERE v.qty <> 0 OR v.value <> 0
               GROUP BY GROUPING SETS ((), (v.product_id, pr.name, v.warehouse_id, wh.name, v.qty))),
           gl AS (
               SELECT coalesce(sum(jl.functional_debit - jl.functional_credit), 0) AS balance
               FROM journal_lines jl
               JOIN journal_entries je ON je.workspace_id = jl.workspace_id AND je.id = jl.entry_id
               JOIN account_mappings m ON m.workspace_id = jl.workspace_id
                                      AND m.account_id = jl.account_id AND m.role = 'inventory'
               WHERE jl.workspace_id = $1 AND je.state = 'posted' AND je.accounting_date <= $2)
           SELECT g.gr AS "gr!", g.product_id, g.product, g.warehouse_id, g.warehouse,
                  trim_scale(g.qty)::text AS quantity,
                  round(g.value / 10::numeric ^ $4::int, $4::int)::text AS "value!",
                  round(gl.balance / 10::numeric ^ $4::int, $4::int)::text AS "gl!",
                  round((g.value - gl.balance) / 10::numeric ^ $4::int, $4::int)::text AS "diff!"
           FROM g CROSS JOIN gl
           ORDER BY g.gr DESC, g.product, g.warehouse, g.product_id, g.warehouse_id"#,
        ws,
        as_of,
        warehouse_id,
        dp,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = rows.iter().find(|r| r.gr == 1).ok_or(AppError::NotFound)?;
    let whole = warehouse_id.is_none();
    Ok(InventoryValuationResource {
        functional_currency: currency,
        as_of,
        cutover_date: cutover(conn, ws).await?,
        warehouse_id,
        total_value: total.value.clone(),
        gl_balance: whole.then(|| total.gl.clone()),
        difference: whole.then(|| total.diff.clone()),
        rows: rows
            .iter()
            .filter_map(|r| {
                Some(ValuationRow {
                    product: NamedRef {
                        id: r.product_id?,
                        name: r.product.clone()?,
                    },
                    warehouse: NamedRef {
                        id: r.warehouse_id?,
                        name: r.warehouse.clone()?,
                    },
                    quantity: r.quantity.clone()?,
                    value: r.value.clone(),
                })
            })
            .collect(),
    })
}
