//! Customer returns and credits (Flow ERP M4, masterplan §12 CUSTOMER CREDIT): drafts that
//! reserve credit capacity per recognition line, reviewed post of the original proportional
//! money once, separate restock, typed corrections. Active books only. Lock order: command
//! row → the sale's order (or invoice) → shared book gate → recognition FOR UPDATE (every
//! route crediting one sale queues here) → return row → stock → open items → journal.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::Rules,
        list::{Page, Paginated},
        workspace::WorkspaceAccess,
    },
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{Action, Create},
            journals::{SourceHeader, SourceLine, post_source_lines},
            service::{created, ok, stale, unsupported},
            settlement::{self, NewAllocation, NewOpenItem, Side},
        },
        customer_deposits, farm_supply, invoices, orders,
        purchasing::{
            dto::{CancelPayload, PageQuery},
            service::{amount, qty},
        },
        sales_returns::{
            dto::{
                ActionReview, FingerprintPayload, LegacyCostPayload, LegacyCostResource,
                PostReview, RestockLine, RestockLineResource, RestockPayload, RestockResource,
                RestockReversePayload, ReturnCreatePayload, ReturnInput, ReturnLineResource,
                ReturnUpdatePayload, ReversePayload, SalesReturnResource,
            },
            repo::{self, LineRow, NewReturn, RestockRow, ReturnRow, Sale},
        },
        shipments::recognition,
        stock::{
            self,
            repo::{Cause, Floor, Mutated, Slot},
            reserve,
            value::{self, Valued},
        },
    },
};

const SETUP: &str = "Customer returns are recorded here once the accounting book is active.";

fn not_draft() -> AppError {
    AppError::Unprocessable("Only draft returns can change.".into())
}

/// 422 `unsupported_active_erp_operation` in a setup book (takes the shared gate).
pub async fn require_active(conn: &mut PgConnection, ws: Uuid) -> Result<(), AppError> {
    if value::active(&mut *conn, ws).await? {
        Ok(())
    } else {
        Err(unsupported(SETUP))
    }
}

/// Lock the path to sale `rec`: its order (or invoice), the book gate, the recognition.
pub async fn lock_sale(conn: &mut PgConnection, ws: Uuid, rec: Uuid) -> Result<Sale, AppError> {
    let (order, invoice) = repo::anchor(&mut *conn, ws, rec)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(o) = order {
        orders::repo::lock(&mut *conn, ws, o)
            .await?
            .ok_or(AppError::NotFound)?;
    } else if let Some(i) = invoice {
        invoices::repo::lock(&mut *conn, ws, i)
            .await?
            .ok_or(AppError::NotFound)?;
    }
    require_active(&mut *conn, ws).await?;
    repo::lock_sale(conn, ws, rec).await
}

/// Credit capacity under the sale lock: per line, Σ draft + posted credits (other returns)
/// + this one ≤ recognised. 422 per offending line under `prefix`.
pub async fn check_capacity(
    conn: &mut PgConnection,
    ws: Uuid,
    (sale, except): (Uuid, Option<Uuid>),
    input: &[(Uuid, Decimal)],
    prefix: &str,
) -> Result<(), AppError> {
    let ids: Vec<Uuid> = input.iter().map(|l| l.0).collect();
    let left = repo::left(&mut *conn, ws, (sale, except), &ids).await?;
    let mut r = Rules::default();
    for (i, (id, q)) in input.iter().enumerate() {
        match left.iter().find(|l| l.0 == *id) {
            None => r.add(
                &format!("{prefix}{i}.recognition_line_id"),
                "The selected line is not part of this sale.",
            ),
            Some((_, left)) => r.check(
                &format!("{prefix}{i}.quantity"),
                q <= left,
                format!("Only {} is left to credit on this line.", qty(*left)),
            ),
        }
    }
    r.into_result()
}

/// Insert a draft credit of `input` on the locked sale; its id.
pub async fn create_locked(
    conn: &mut PgConnection,
    ws: Uuid,
    sale: &Sale,
    (client_ref, credit_invoice_id): (Uuid, Option<Uuid>),
    (input, prefix): (&ReturnInput, &str),
    by: Uuid,
) -> Result<Uuid, AppError> {
    check_capacity(&mut *conn, ws, (sale.id, None), &input.lines, prefix).await?;
    let id = repo::insert(
        &mut *conn,
        ws,
        &NewReturn {
            client_ref,
            kind: if sale.billed {
                "invoice_credit"
            } else {
                "receipt_credit"
            },
            sale,
            credit_invoice_id,
            reason: &input.reason,
            date: input.accounting_date,
            by,
        },
    )
    .await?;
    repo::replace_lines(conn, ws, (id, sale.id), &input.lines).await?;
    Ok(id)
}

fn money(v: Option<i64>, dp: i16) -> Option<String> {
    v.map(|m| amount(m, dp))
}

fn line_resource(l: &LineRow, dp: i16) -> ReturnLineResource {
    ReturnLineResource {
        id: l.id,
        recognition_line_id: l.recognition_line_id,
        product_id: l.product_id,
        quantity: qty(l.quantity),
        net: money(l.net, dp),
        tax: money(l.tax, dp),
        restocked: qty(l.restocked),
    }
}

/// Restocks of one return from its line rows (one row per restock line).
fn restock_resources(rows: Vec<RestockRow>, dp: i16) -> Vec<RestockResource> {
    let mut out: Vec<RestockResource> = Vec::new();
    for r in rows {
        let line = RestockLineResource {
            return_line_id: r.return_line_id,
            quantity: qty(r.quantity),
            lot_id: r.lot_id,
            value: amount(r.value, dp),
            legacy_return_cost_id: r.legacy_return_cost_id,
        };
        match out.last_mut() {
            Some(last) if last.id == r.id => last.lines.push(line),
            _ => out.push(RestockResource {
                id: r.id,
                warehouse_id: r.warehouse_id,
                state: r.state,
                version: r.version,
                accounting_date: r.accounting_date,
                lines: vec![line],
                created_by: r.created_by,
                created_at: r.created_at,
                reversed_at: r.reversed_at,
                reversal_reason: r.reversal_reason,
            }),
        }
    }
    out
}

fn resource(
    r: ReturnRow,
    ws: Uuid,
    lines: &[LineRow],
    restocks: Vec<RestockRow>,
) -> SalesReturnResource {
    let dp = r.decimals;
    let open_physical = lines.iter().any(|l| l.restocked < l.quantity);
    let allowed_actions = match r.state.as_str() {
        "draft" => vec!["update", "post", "cancel"],
        "posted" if open_physical => vec!["restock", "reverse"],
        "posted" => vec!["reverse"],
        _ => vec![],
    };
    let credit = match (r.net, r.tax, r.rounding) {
        (Some(n), Some(t), Some(x)) => Some(amount(n + t + x, dp)),
        _ => None,
    };
    SalesReturnResource {
        id: r.id,
        workspace_id: ws,
        kind: r.kind,
        recognition_id: r.recognition_id,
        order_id: r.order_id,
        relation_id: r.relation_id,
        credit_invoice_id: r.credit_invoice_id,
        version: r.version,
        reason: r.reason,
        accounting_date: r.accounting_date,
        net: money(r.net, dp),
        tax: money(r.tax, dp),
        rounding: money(r.rounding, dp),
        credit,
        credit_open_item_id: r.credit_item_id,
        lines: lines.iter().map(|l| line_resource(l, dp)).collect(),
        restocks: restock_resources(restocks, dp),
        created_by: r.created_by,
        created_at: r.created_at,
        updated_at: r.updated_at,
        posted_at: r.posted_at,
        reversed_at: r.reversed_at,
        reversal_reason: r.reversal_reason,
        cancelled_at: r.cancelled_at,
        cancel_reason: r.cancel_reason,
        state: r.state,
        allowed_actions,
    }
}

/// Rows as resources, lines and restocks in one query each.
async fn resources(
    conn: &mut PgConnection,
    ws: Uuid,
    rows: Vec<ReturnRow>,
) -> Result<Vec<SalesReturnResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut lines: HashMap<Uuid, Vec<LineRow>> = HashMap::new();
    for l in repo::lines(&mut *conn, ws, &ids).await? {
        lines.entry(l.return_id).or_default().push(l);
    }
    let mut restocks: HashMap<Uuid, Vec<RestockRow>> = HashMap::new();
    for s in repo::restocks(&mut *conn, ws, &ids).await? {
        restocks.entry(s.return_id).or_default().push(s);
    }
    Ok(rows
        .into_iter()
        .map(|r| {
            let l = lines.remove(&r.id).unwrap_or_default();
            let s = restocks.remove(&r.id).unwrap_or_default();
            resource(r, ws, &l, s)
        })
        .collect())
}

/// One return; 404 outside the workspace.
pub async fn get(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<SalesReturnResource, AppError> {
    let (rows, _) = repo::returns(&mut *conn, ws, Some(id), (1, 0)).await?;
    resources(conn, ws, rows)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// GET /sales-returns: newest first.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    q: &PageQuery,
) -> Result<Paginated<SalesReturnResource>, AppError> {
    let mut conn = state.db.acquire().await?;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) = repo::returns(&mut conn, ws, None, (page.size, page.offset())).await?;
    let data = resources(&mut conn, ws, rows).await?;
    Ok(Paginated::new(data, total, page))
}

/// POST /sales-returns: a draft (reserves capacity only).
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<ReturnCreatePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "sales_returns_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (rec, input) = &cmd.input.payload;
    require_active(&mut tx, ws).await?;
    let sale = lock_sale(&mut tx, ws, *rec).await?;
    let id = create_locked(
        &mut tx,
        ws,
        &sale,
        (cmd.input.client_ref, None),
        (input, "payload.lines."),
        access.user_id,
    )
    .await?;
    let answer = created(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Lock the sale of return `id` then the return; check the caller's version; (sale, state).
pub async fn lock_return(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    expected: Option<i64>,
) -> Result<(Sale, String), AppError> {
    let rec = repo::recognition_of(&mut *conn, ws, id).await?;
    let sale = lock_sale(&mut *conn, ws, rec).await?;
    let (_, state, version) = repo::lock(conn, ws, id).await?;
    if expected.is_some_and(|v| v != version) {
        return Err(stale());
    }
    Ok((sale, state))
}

/// PATCH /sales-returns/{id}: replace a draft's date, reason and lines.
pub async fn update(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReturnUpdatePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "sales_returns_update";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (sale, st) = lock_return(&mut tx, ws, id, Some(cmd.input.expected_version)).await?;
    if st != "draft" {
        return Err(not_draft());
    }
    let input = &cmd.input.payload;
    check_capacity(
        &mut tx,
        ws,
        (sale.id, Some(id)),
        &input.lines,
        "payload.lines.",
    )
    .await?;
    repo::update_draft(&mut tx, ws, id, (input.accounting_date, &input.reason)).await?;
    repo::replace_lines(&mut tx, ws, (id, sale.id), &input.lines).await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST /sales-returns/{id}/cancel: a draft only; frees its capacity.
pub async fn cancel(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<CancelPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "sales_returns_cancel";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (_, st) = lock_return(&mut tx, ws, id, Some(cmd.input.expected_version)).await?;
    if st != "draft" {
        return Err(not_draft());
    }
    repo::cancel(&mut tx, ws, id, (&cmd.input.payload, access.user_id)).await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Money of posting a return, computed under the sale lock (all in SQL).
#[derive(sqlx::FromRow)]
pub struct PostFacts {
    pub net: i64,
    pub tax: i64,
    /// Cash rounding share (signed): credit = net + tax + rounding.
    pub rounding: i64,
    pub credit: i64,
    /// Part applied at once to the sale's open receivable.
    pub applied: i64,
    /// Credit − applied: what stays owed to the customer.
    pub remainder: i64,
    pub rounding_debit: i64,
    pub rounding_credit: i64,
    /// The sale's current receivable item (billed first) and its role.
    pub item_id: Option<Uuid>,
    pub item_role: Option<String>,
}

/// Per return line (id, net, tax): the cumulative share of its recognition line up to the
/// credits still posted plus this one, less the share up to the posted ones; the credit
/// reaching the recognised quantity takes the exact rest. Lines with a tax snapshot (active
/// books, M6-A) take tax as Σ [`component_shares`].
async fn line_money(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Vec<(Uuid, i64, i64)>, AppError> {
    Ok(sqlx::query_as(
        "WITH x AS (
             SELECT x.id, x.quantity AS q, l.quantity AS whole, l.net AS ln, l.tax AS lt,
                    coalesce((SELECT sum(y.quantity) FROM sales_return_lines y
                              JOIN sales_returns r ON r.id = y.return_id
                              WHERE y.workspace_id = $1 AND y.recognition_line_id = l.id
                                AND r.state = 'posted'), 0) AS before
             FROM sales_return_lines x
             JOIN sales_recognition_lines l ON l.workspace_id = x.workspace_id
                                            AND l.id = x.recognition_line_id
             WHERE x.workspace_id = $1 AND x.return_id = $2),
         c AS (SELECT return_line_id, sum(tax)::int8 AS tax FROM component_shares($1, $2)
               GROUP BY return_line_id)
         SELECT x.id,
                (money_round_div(ln * (before + q), whole) - money_round_div(ln * before, whole))::int8,
                coalesce(c.tax, money_round_div(lt * (before + q), whole)
                                - money_round_div(lt * before, whole))::int8
         FROM x LEFT JOIN c ON c.return_line_id = x.id ORDER BY x.id",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(conn)
    .await?)
}

/// Freeze return `id`'s per-component tax reversal (`sales_return_line_taxes`) at post.
async fn write_component_shares(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO sales_return_line_taxes (workspace_id, return_line_id, line_no, code, tax)
         SELECT $1, return_line_id, line_no, code, tax FROM component_shares($1, $2)",
    )
    .bind(ws)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(())
}

/// What posting return `id` of the locked `sale` writes, from current facts.
pub async fn post_facts(
    conn: &mut PgConnection,
    ws: Uuid,
    sale: &Sale,
    id: Uuid,
) -> Result<(Vec<(Uuid, i64, i64)>, PostFacts), AppError> {
    let lines = line_money(&mut *conn, ws, id).await?;
    let net: Vec<i64> = lines.iter().map(|l| l.1).collect();
    let tax: Vec<i64> = lines.iter().map(|l| l.2).collect();
    // R = cash due − gross of a till receipt (its `received` journal's rounding line); the
    // return takes the cumulative share by gross, the last one the exact rest.
    let f: PostFacts = sqlx::query_as(
        "WITH m AS (
             SELECT (SELECT sum(n) FROM UNNEST($3::int8[]) n)::int8 AS net,
                    (SELECT sum(t) FROM UNNEST($4::int8[]) t)::int8 AS tax),
         r AS (
             SELECT coalesce((
                 SELECT sum(jl.credit - jl.debit) FROM accounting_effects e
                 JOIN journal_entries je ON je.workspace_id = e.workspace_id AND je.effect_id = e.id
                 JOIN journal_lines jl ON jl.entry_id = je.id
                 JOIN account_mappings am ON am.workspace_id = e.workspace_id
                                         AND am.role = 'rounding' AND am.account_id = jl.account_id
                 WHERE e.workspace_id = $1 AND e.source_kind = 'retail_sale'
                   AND e.source_id = $5 AND e.effect_kind = 'received'), 0) AS total,
                    coalesce((SELECT sum(net + tax) FROM sales_returns
                              WHERE workspace_id = $1 AND recognition_id = $2
                                AND state = 'posted'), 0) AS before),
         s AS (
             SELECT m.net, m.tax,
                    CASE WHEN $6::int8 = 0 THEN 0
                         ELSE money_round_div(r.total * (r.before + m.net + m.tax), $6)
                              - money_round_div(r.total * r.before, $6) END::int8 AS rounding
             FROM m, r),
         i AS (
             SELECT id, account_role, residual FROM accounting_open_items
             WHERE workspace_id = $1 AND recognition_id = $2 AND classification = 'receivable'
             ORDER BY account_role = 'receivables' DESC, created_at DESC, id DESC LIMIT 1)
         SELECT s.net, s.tax, s.rounding, (s.net + s.tax + s.rounding)::int8 AS credit,
                greatest(least(s.net + s.tax + s.rounding, coalesce(i.residual, 0)), 0)::int8 AS applied,
                (s.net + s.tax + s.rounding
                 - greatest(least(s.net + s.tax + s.rounding, coalesce(i.residual, 0)), 0))::int8
                    AS remainder,
                greatest(s.rounding, 0)::int8 AS rounding_debit,
                greatest(-s.rounding, 0)::int8 AS rounding_credit,
                i.id AS item_id, i.account_role AS item_role
         FROM s LEFT JOIN i ON true",
    )
    .bind(ws)
    .bind(sale.id)
    .bind(&net)
    .bind(&tax)
    .bind(sale.source_id)
    .bind(sale.gross)
    .fetch_one(conn)
    .await?;
    Ok((lines, f))
}

const NOTHING: &str = "This return credits nothing.";

/// Post draft `id` of the locked `sale` on `date`: journal DR sales_returns / DR tax_payable
/// / ± rounding / CR the sale's receivable role for the part applied at once to what the
/// customer still owes / CR receivables for the rest; one customer-credit open item
/// (`sales_return`/<id>, role receivables) for the whole credit.
pub async fn post_locked(
    conn: &mut PgConnection,
    ws: Uuid,
    sale: &Sale,
    id: Uuid,
    (date, by): (chrono::NaiveDate, Uuid),
) -> Result<(), AppError> {
    let (lines, f) = post_facts(&mut *conn, ws, sale, id).await?;
    if f.credit <= 0 {
        return Err(AppError::Unprocessable(NOTHING.into()));
    }
    let role = match f.item_role.as_deref() {
        Some("receivables") => "receivables",
        _ => "unbilled_receivables",
    };
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({ "sales_return": id, "net": f.net, "tax": f.tax, "rounding": f.rounding })
            .to_string(),
    )
    .into();
    let header = SourceHeader {
        source_kind: "sales_return",
        source_id: id,
        effect_kind: "posted",
        date,
        reason: "customer credit",
        fingerprint: &fingerprint,
        by,
    };
    let journal: Vec<SourceLine> = [
        ("sales_returns", f.net, 0, None),
        ("tax_payable", f.tax, 0, None),
        ("rounding", f.rounding_debit, f.rounding_credit, None),
        (role, 0, f.applied, sale.relation_id),
        ("receivables", 0, f.remainder, sale.relation_id),
    ]
    .into_iter()
    .filter(|l| l.1 > 0 || l.2 > 0)
    .map(|(role, debit, credit, relation_id)| SourceLine {
        side: Side::Role(role),
        debit,
        credit,
        relation_id,
        bank_account_id: None,
    })
    .collect();
    let (entry, effect) = post_source_lines(&mut *conn, ws, &header, &journal).await?;
    let account_id = recognition::role_account(&mut *conn, ws, "receivables").await?;
    let credit_invoice = repo::credit_invoice(&mut *conn, ws, id).await?;
    let item = settlement::open_item(
        &mut *conn,
        ws,
        &NewOpenItem {
            classification: "customer_credit",
            side: "credit",
            account_role: "receivables",
            account_id,
            source_kind: "sales_return",
            source_id: id,
            relation_id: sale.relation_id,
            order_id: sale.order_id,
            invoice_id: credit_invoice,
            recognition_id: Some(sale.id),
            currency_id: sale.currency_id,
            issue_date: date,
            due_date: None,
            origin: "posted",
            amount: f.credit,
            functional_amount: f.credit,
            effect_id: effect,
            by,
        },
    )
    .await?;
    if let (Some(target), true) = (f.item_id, f.applied > 0) {
        settlement::allocate(
            &mut *conn,
            ws,
            &NewAllocation {
                kind: "application",
                source_item_id: Some(item),
                target_item_id: target,
                date,
                currency_id: sale.currency_id,
                amount: f.applied,
                effect_id: effect,
                by,
            },
        )
        .await?;
    }
    settlement::link_source(
        &mut *conn,
        ws,
        "sales_return",
        id,
        Some(effect),
        "posted",
        &fingerprint,
    )
    .await?;
    write_component_shares(&mut *conn, ws, id).await?;
    repo::mark_posted(
        conn,
        ws,
        id,
        &lines,
        &repo::Posted {
            net: f.net,
            tax: f.tax,
            rounding: f.rounding,
            credit_item_id: item,
            journal_entry_id: entry,
            effect_id: effect,
        },
        by,
    )
    .await
}

/// The post review of draft `id` (sale and return locked by the caller).
async fn prepare_post(
    conn: &mut PgConnection,
    ws: Uuid,
    sale: &Sale,
    id: Uuid,
) -> Result<PostReview, AppError> {
    let r = get(&mut *conn, ws, id).await?;
    if r.state != "draft" {
        return Err(not_draft());
    }
    recognition::check_date(&mut *conn, ws, r.accounting_date, "payload.accounting_date").await?;
    let (_, f) = post_facts(&mut *conn, ws, sale, id).await?;
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({
            "action": "post", "sales_return": id, "version": r.version, "date": r.accounting_date,
            "lines": r.lines.iter().map(|l| json!([l.recognition_line_id, l.quantity])).collect::<Vec<_>>(),
            "money": [f.net, f.tax, f.rounding, f.applied],
        })
        .to_string(),
    )
    .into();
    let dp = sale.decimals;
    Ok(PostReview {
        return_id: id,
        version: r.version,
        accounting_date: r.accounting_date,
        net: amount(f.net, dp),
        tax: amount(f.tax, dp),
        rounding: amount(f.rounding, dp),
        credit: amount(f.credit, dp),
        applied: amount(f.applied, dp),
        review_fingerprint: hex::encode(fingerprint),
    })
}

/// POST /sales-returns/{id}/post/review: no writes.
pub async fn post_review(state: &AppState, ws: Uuid, id: Uuid) -> Result<PostReview, AppError> {
    let mut tx = state.db.begin().await?;
    let (sale, _) = lock_return(&mut tx, ws, id, None).await?;
    let review = prepare_post(&mut tx, ws, &sale, id).await?;
    tx.rollback().await?;
    Ok(review)
}

/// POST /sales-returns/{id}/post: post the reviewed draft once.
pub async fn post(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<FingerprintPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "sales_returns_post";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (sale, _) = lock_return(&mut tx, ws, id, Some(cmd.input.expected_version)).await?;
    let review = prepare_post(&mut tx, ws, &sale, id).await?;
    if review.review_fingerprint != cmd.input.payload {
        return Err(stale());
    }
    post_locked(
        &mut tx,
        ws,
        &sale,
        id,
        (review.accounting_date, access.user_id),
    )
    .await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

const NOT_POSTED: &str = "Only a posted credit takes restocks.";
const WAREHOUSE: &str =
    "Returned goods go back to the warehouse the sale left; its booked cost lives there.";

/// One checked restock line.
pub(crate) struct Back {
    line: RestockLine,
    product_id: Uuid,
}

/// Check restock `lines` of posted return `id` under the sale lock: each line of this return,
/// stock-kept, at its sale's warehouse, within what is left to put back, a legacy cost only
/// of its own order line. All lines go to one warehouse.
pub(crate) async fn check_restock(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    lines: &[RestockLine],
    prefix: &str,
) -> Result<(Uuid, Vec<Back>), AppError> {
    let ids: Vec<Uuid> = lines.iter().map(|l| l.return_line_id).collect();
    let found = repo::physical(&mut *conn, ws, id, &ids).await?;
    let mut r = Rules::default();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let f = |k: &str| format!("{prefix}{i}.{k}");
        let Some(p) = found.iter().find(|p| p.id == l.return_line_id) else {
            r.add(
                &f("return_line_id"),
                "The selected line is not part of this return.",
            );
            continue;
        };
        let (Some(origin), Some(product)) = (p.origin, p.product_id) else {
            r.add(
                &f("return_line_id"),
                "This line moved no stock; there is nothing to put back.",
            );
            continue;
        };
        r.check(
            &f("destination_warehouse_id"),
            l.warehouse_id == origin,
            WAREHOUSE,
        );
        r.check(
            &f("quantity"),
            l.quantity <= p.left,
            format!(
                "Only {} is left to put back on this line.",
                qty(p.left.max(Decimal::ZERO))
            ),
        );
        if let Some(c) = l.legacy_cost_id
            && !repo::legacy_fits(&mut *conn, ws, c, p.order_line_id).await?
        {
            r.add(
                &f("legacy_cost_evidence_id"),
                "This reviewed cost is for another sale line.",
            );
        }
        out.push(Back {
            line: *l,
            product_id: product,
        });
    }
    r.check(
        &format!("{prefix}0.destination_warehouse_id"),
        lines
            .iter()
            .all(|l| l.warehouse_id == lines[0].warehouse_id),
        "Put one restock's lines into one warehouse.",
    );
    r.into_result()?;
    Ok((lines[0].warehouse_id, out))
}

/// Put `backs` of the posted return `id` (sale locked) back on stock now: one restock, each
/// line into the lots the order took (no-lot row otherwise) at the original booked cost or a
/// reviewed legacy cost. The restock's id.
pub(crate) async fn restock_locked(
    conn: &mut PgConnection,
    ws: Uuid,
    sale: &Sale,
    (id, client_ref): (Uuid, Uuid),
    (warehouse, backs): (Uuid, &[Back]),
    by: Uuid,
) -> Result<Uuid, AppError> {
    let date = crate::modules::shipments::repo::book_today(&mut *conn, ws).await?;
    let restock =
        repo::insert_restock(&mut *conn, ws, (client_ref, id, warehouse), (date, by)).await?;
    let source: (&'static str, Uuid) = match (sale.order_id, sale.invoice_id) {
        (Some(o), _) => ("order", o),
        (None, Some(i)) => ("invoice", i),
        _ => return Err(AppError::Internal("sale without order or invoice".into())),
    };
    let reason = "customer return restock";
    for b in backs {
        let want = b.line.quantity;
        let lots: Vec<reserve::Back> = match sale.order_id {
            Some(order) => {
                let was = repo::restocked_of_order(&mut *conn, ws, order, b.product_id).await?;
                let before = std::collections::BTreeMap::from([(b.product_id, was)]);
                let after = std::collections::BTreeMap::from([(b.product_id, was + want)]);
                farm_supply::sale::restock_plan(&mut *conn, ws, order, &before, &after).await?
            }
            None => vec![reserve::Back {
                product_id: b.product_id,
                lot_id: None,
                quantity: want,
            }],
        };
        let cause = Cause {
            reason: Some(reason),
            user_id: Some(by),
            related: Some(("sales_return_restock", restock)),
            value: Valued::SaleReturn {
                sale: source,
                legacy: b.line.legacy_cost_id,
            },
            ..Cause::default()
        };
        for lot in lots {
            let before = repo::restock_value(&mut *conn, ws, restock).await?;
            reserve::restock_lots(&mut *conn, ws, warehouse, &[lot], &cause).await?;
            let value = repo::restock_value(&mut *conn, ws, restock).await? - before;
            repo::insert_restock_line(
                &mut *conn,
                ws,
                (restock, b.line.return_line_id),
                (lot.quantity, lot.lot_id, value, b.line.legacy_cost_id),
            )
            .await?;
        }
    }
    Ok(restock)
}

/// The restock review of posted return `id` (sale and return locked by the caller).
async fn prepare_restock(
    conn: &mut PgConnection,
    ws: Uuid,
    (id, state): (Uuid, &str),
    lines: &[RestockLine],
    prefix: &str,
) -> Result<((Uuid, Vec<Back>), ActionReview), AppError> {
    if state != "posted" {
        return Err(AppError::Unprocessable(NOT_POSTED.into()));
    }
    let checked = check_restock(&mut *conn, ws, id, lines, prefix).await?;
    let r = get(&mut *conn, ws, id).await?;
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({
            "action": "restock", "sales_return": id, "version": r.version,
            "lines": lines.iter().map(|l| json!([l.return_line_id, qty(l.quantity), l.warehouse_id, l.legacy_cost_id])).collect::<Vec<_>>(),
            "restocked": r.lines.iter().map(|l| json!([l.id, l.restocked])).collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .into();
    Ok((
        checked,
        ActionReview {
            return_id: id,
            version: r.version,
            value: None,
            review_fingerprint: hex::encode(fingerprint),
        },
    ))
}

/// POST /sales-returns/{id}/restock/review: no writes.
pub async fn restock_review(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
    lines: &[RestockLine],
) -> Result<ActionReview, AppError> {
    let mut tx = state.db.begin().await?;
    let (_, st) = lock_return(&mut tx, ws, id, None).await?;
    let (_, review) = prepare_restock(&mut tx, ws, (id, &st), lines, "lines.").await?;
    tx.rollback().await?;
    Ok(review)
}

/// POST /sales-returns/{id}/restock: put credited goods back on stock (separate effect).
pub async fn restock(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<RestockPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "sales_returns_restock";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (sale, st) = lock_return(&mut tx, ws, id, Some(cmd.input.expected_version)).await?;
    let (lines, fingerprint) = &cmd.input.payload;
    let ((warehouse, backs), review) =
        prepare_restock(&mut tx, ws, (id, &st), lines, "payload.lines.").await?;
    if &review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    let restock = restock_locked(
        &mut tx,
        ws,
        &sale,
        (id, cmd.input.client_ref),
        (warehouse, &backs),
        access.user_id,
    )
    .await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id, restock]).await?;
    tx.commit().await?;
    Ok(answer)
}

fn dependent(message: &str) -> AppError {
    AppError::Reason {
        status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        code: "dependent_effects_exist",
        message: message.into(),
    }
}

const REFUNDED: &str =
    "This credit was refunded or applied elsewhere. Reverse those first; the credit stays.";
const VALUED_SINCE: &str = "This stock was valued again since the restock; it can't be undone exactly. Record a stock adjustment instead.";

/// The financial reversal review of posted return `id` (sale and return locked).
async fn prepare_reverse(
    conn: &mut PgConnection,
    ws: Uuid,
    (id, date): (Uuid, chrono::NaiveDate),
    dp: i16,
) -> Result<(repo::Reversible, ActionReview), AppError> {
    let r = repo::reversible(&mut *conn, ws, id).await?;
    recognition::check_date(&mut *conn, ws, date, "payload.accounting_date").await?;
    if repo::dependents(&mut *conn, ws, r.credit_item_id, r.effect_id).await? > 0 {
        return Err(dependent(REFUNDED));
    }
    let version = get(&mut *conn, ws, id).await?.version;
    let credit = sqlx::query_scalar::<_, i64>("SELECT ($1::int8 + $2::int8 + $3::int8)::int8")
        .bind(r.net)
        .bind(r.tax)
        .bind(r.rounding)
        .fetch_one(&mut *conn)
        .await?;
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({ "action": "reverse", "sales_return": id, "version": version, "date": date,
                "credit": credit })
        .to_string(),
    )
    .into();
    Ok((
        r,
        ActionReview {
            return_id: id,
            version,
            value: Some(amount(credit, dp)),
            review_fingerprint: hex::encode(fingerprint),
        },
    ))
}

/// Reverse posted return `id` financially on `date`: undo its own application, post the
/// exact inverse journal, close the credit item against a reversal item. Restocks stay;
/// the credited capacity frees.
async fn reverse_locked(
    conn: &mut PgConnection,
    ws: Uuid,
    sale: &Sale,
    (id, r): (Uuid, &repo::Reversible),
    (date, reason, by): (chrono::NaiveDate, &str, Uuid),
) -> Result<(), AppError> {
    let own = repo::own_application(&mut *conn, ws, r.credit_item_id, r.effect_id).await?;
    let (applied, role) = match &own {
        Some((_, a, role)) if role == "receivables" => (*a, "receivables"),
        Some((_, a, _)) => (*a, "unbilled_receivables"),
        None => (0, "unbilled_receivables"),
    };
    let (credit, remainder, rounding_credit, rounding_debit): (i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT ($1::int8 + $2::int8 + $3::int8)::int8,
                    ($1::int8 + $2::int8 + $3::int8 - $4::int8)::int8,
                    greatest($3::int8, 0)::int8, greatest(-$3::int8, 0)::int8",
        )
        .bind(r.net)
        .bind(r.tax)
        .bind(r.rounding)
        .bind(applied)
        .fetch_one(&mut *conn)
        .await?;
    let fingerprint: [u8; 32] =
        Sha256::digest(json!({ "reverse": id, "credit": credit }).to_string()).into();
    let header = SourceHeader {
        source_kind: "sales_return",
        source_id: id,
        effect_kind: "reversed",
        date,
        reason: "customer credit reversal",
        fingerprint: &fingerprint,
        by,
    };
    let journal: Vec<SourceLine> = [
        (role, applied, 0, sale.relation_id),
        ("receivables", remainder, 0, sale.relation_id),
        ("sales_returns", 0, r.net, None),
        ("tax_payable", 0, r.tax, None),
        ("rounding", rounding_debit, rounding_credit, None),
    ]
    .into_iter()
    .filter(|l| l.1 > 0 || l.2 > 0)
    .map(|(role, debit, credit, relation_id)| SourceLine {
        side: Side::Role(role),
        debit,
        credit,
        relation_id,
        bank_account_id: None,
    })
    .collect();
    let (_, effect) = post_source_lines(&mut *conn, ws, &header, &journal).await?;
    if let Some((a, _, _)) = own {
        settlement::reverse(&mut *conn, ws, a, date, effect, by).await?;
    }
    // The credit item (residual = credit again) closes against a reversal item.
    let account_id = recognition::role_account(&mut *conn, ws, "receivables").await?;
    let undo = settlement::open_item(
        &mut *conn,
        ws,
        &NewOpenItem {
            classification: "receivable",
            side: "debit",
            account_role: "receivables",
            account_id,
            source_kind: "sales_return_reversal",
            source_id: id,
            relation_id: sale.relation_id,
            order_id: sale.order_id,
            invoice_id: None,
            recognition_id: Some(sale.id),
            currency_id: sale.currency_id,
            issue_date: date,
            due_date: None,
            origin: "posted",
            amount: credit,
            functional_amount: credit,
            effect_id: effect,
            by,
        },
    )
    .await?;
    settlement::allocate(
        &mut *conn,
        ws,
        &NewAllocation {
            kind: "application",
            source_item_id: Some(r.credit_item_id),
            target_item_id: undo,
            date,
            currency_id: sale.currency_id,
            amount: credit,
            effect_id: effect,
            by,
        },
    )
    .await?;
    repo::mark_reversed(conn, ws, id, (effect, reason, by)).await
}

/// POST /sales-returns/{id}/reverse/review: no writes.
pub async fn reverse_review(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
    date: chrono::NaiveDate,
) -> Result<ActionReview, AppError> {
    let mut tx = state.db.begin().await?;
    let (sale, _) = lock_return(&mut tx, ws, id, None).await?;
    let (_, review) = prepare_reverse(&mut tx, ws, (id, date), sale.decimals).await?;
    tx.rollback().await?;
    Ok(review)
}

/// POST /sales-returns/{id}/reverse: reverse the financial credit only.
pub async fn reverse(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "sales_returns_reverse";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (sale, _) = lock_return(&mut tx, ws, id, Some(cmd.input.expected_version)).await?;
    let (date, reason, fingerprint) = &cmd.input.payload;
    let (r, review) = prepare_reverse(&mut tx, ws, (id, *date), sale.decimals).await?;
    if &review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    reverse_locked(
        &mut tx,
        ws,
        &sale,
        (id, &r),
        (*date, reason, access.user_id),
    )
    .await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// The undo review of restock `restock` (sale and return locked): posted, nothing valued in
/// its pools since, its exact value.
async fn prepare_restock_reverse(
    conn: &mut PgConnection,
    ws: Uuid,
    (id, restock): (Uuid, Uuid),
    dp: i16,
) -> Result<((Uuid, Vec<repo::Undo>), ActionReview), AppError> {
    let (st, warehouse, lines) = repo::lock_restock(&mut *conn, ws, (id, restock)).await?;
    if st != "posted" {
        return Err(AppError::Unprocessable(
            "This restock is already reversed.".into(),
        ));
    }
    if repo::valued_since(&mut *conn, ws, restock).await? {
        return Err(dependent(VALUED_SINCE));
    }
    let value = repo::restock_value(&mut *conn, ws, restock).await?;
    let version = get(&mut *conn, ws, id).await?.version;
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({ "action": "restock_reverse", "restock": restock, "version": version,
                "value": value })
        .to_string(),
    )
    .into();
    Ok((
        (warehouse, lines),
        ActionReview {
            return_id: id,
            version,
            value: Some(amount(value, dp)),
            review_fingerprint: hex::encode(fingerprint),
        },
    ))
}

/// Take restock `restock`'s goods back off stock at their exact restored value (same lots,
/// only what is available and unreserved). The credit stays.
async fn restock_reverse_locked(
    conn: &mut PgConnection,
    ws: Uuid,
    sale: &Sale,
    restock: Uuid,
    (warehouse, lines): (Uuid, &[repo::Undo]),
    (reason, by): (&str, Uuid),
) -> Result<(), AppError> {
    let source: (&'static str, Uuid) = match (sale.order_id, sale.invoice_id) {
        (Some(o), _) => ("order", o),
        (None, Some(i)) => ("invoice", i),
        _ => return Err(AppError::Internal("sale without order or invoice".into())),
    };
    let slots: Vec<Slot> = lines
        .iter()
        .map(|l| Slot {
            product_id: l.product_id,
            warehouse_id: warehouse,
            bin: None,
            lot_id: l.lot_id,
        })
        .collect();
    stock::repo::lock_all(&mut *conn, ws, &slots).await?;
    for (l, slot) in lines.iter().zip(slots) {
        let cause = Cause {
            reason: Some("customer return restock reversal"),
            user_id: Some(by),
            related: Some(("sales_return_restock", restock)),
            value: Valued::SaleReturnUndo {
                sale: source,
                legacy: l.legacy_return_cost_id,
                value: l.value,
            },
            ..Cause::default()
        };
        match stock::repo::mutate(&mut *conn, ws, slot, -l.quantity, Floor::Available, &cause)
            .await?
        {
            Mutated::Done { .. } => {}
            _ => return Err(value::insufficient()),
        }
    }
    repo::mark_restock_reversed(conn, ws, restock, (reason, by)).await
}

/// POST /sales-returns/{id}/restocks/{restock}/reverse/review: no writes.
pub async fn restock_reverse_review(
    state: &AppState,
    ws: Uuid,
    (id, restock): (Uuid, Uuid),
) -> Result<ActionReview, AppError> {
    let mut tx = state.db.begin().await?;
    let (sale, _) = lock_return(&mut tx, ws, id, None).await?;
    let (_, review) = prepare_restock_reverse(&mut tx, ws, (id, restock), sale.decimals).await?;
    tx.rollback().await?;
    Ok(review)
}

/// POST /sales-returns/{id}/restocks/{restock}/reverse: physical undo only.
pub async fn restock_reverse(
    state: &AppState,
    access: &WorkspaceAccess,
    (id, restock): (Uuid, Uuid),
    cmd: Command<Action<RestockReversePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "sales_return_restocks_reverse";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (sale, _) = lock_return(&mut tx, ws, id, Some(cmd.input.expected_version)).await?;
    let (reason, fingerprint) = &cmd.input.payload;
    let (undo, review) = prepare_restock_reverse(&mut tx, ws, (id, restock), sale.decimals).await?;
    if &review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    restock_reverse_locked(
        &mut tx,
        ws,
        &sale,
        restock,
        (undo.0, &undo.1),
        (reason, access.user_id),
    )
    .await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id, restock]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST /legacy-return-costs: a reviewed restoration value for returns of a sale the book
/// has no booked cost for (active books).
pub async fn legacy_create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<LegacyCostPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "legacy_return_costs_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    require_active(&mut tx, ws).await?;
    let c = &cmd.input.payload;
    let (product, deducted) = repo::order_line(&mut tx, ws, (c.order_id, c.line_id))
        .await?
        .ok_or(AppError::NotFound)?;
    let mut r = Rules::default();
    r.check(
        "payload.target_product_id",
        c.target_product_id.is_none_or(|p| p == product),
        "A different product (SKU mapping) isn't supported yet; leave it empty.",
    );
    r.check(
        "payload.quantity",
        c.quantity <= deducted,
        format!("At most {} of this line left stock.", qty(deducted)),
    );
    r.into_result()?;
    let functional = crate::modules::accounting::repo::settings(&mut tx, ws, false)
        .await?
        .functional_currency_id
        .ok_or_else(crate::modules::accounting::service::not_configured)?;
    let dp: i16 =
        sqlx::query_scalar("SELECT decimals FROM currencies WHERE workspace_id = $1 AND id = $2")
            .bind(ws)
            .bind(functional)
            .fetch_one(&mut *tx)
            .await?;
    let minor =
        customer_deposits::service::minor(&mut tx, (c.amount, dp), "payload.restoration_amount")
            .await?;
    let row = repo::insert_legacy(
        &mut tx,
        ws,
        cmd.input.client_ref,
        c,
        (minor, access.user_id),
    )
    .await?;
    let id = row.id;
    let answer = created(&LegacyCostResource {
        id,
        workspace_id: ws,
        original_sale_id: row.order_id,
        line_id: row.order_line_id,
        quantity: qty(row.quantity),
        restoration_amount: amount(row.restoration_amount, dp),
        target_product_id: row.target_product_id,
        evidence_ref: row.evidence_ref,
        reason: row.reason,
        consumed_quantity: qty(row.consumed_quantity),
        reviewed_by: row.reviewed_by,
        created_at: row.created_at,
    });
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
