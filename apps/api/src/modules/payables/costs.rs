//! Purchase cost adjustments (masterplan §11 TYPED CORRECTIONS, D20, §15): a reviewed split of
//! a posted bill's pending purchase cost variance into inventory (value-only pool entry, no
//! quantity) and COGS. Post: DR/CR inventory, DR/CR cogs, CR/DR pending variance. Reverse only
//! while every touched pool has no later valued event (else `dependent_effects_exist`: post a
//! new current-period adjustment). Lock order: command row → shared gate → adjustment FOR
//! UPDATE → bill FOR SHARE → receipt lines (id) → per line in (product, warehouse) order:
//! warehouse SHARE → pool UPDATE → accounts/period → effect/journal number.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
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
            gate,
            journals::{SourceHeader, SourceLine, post_source_lines},
            service::{created, ok, stale},
        },
        payables::{
            bills::{JournalRow, functional, lock_receipt_lines},
            common::{self, dependent, fp32, journal_preview, role_side},
            dto::{
                CostAdjustmentInput, CostAdjustmentLineResource, CostAdjustmentPayload,
                CostAdjustmentResource, CostAdjustmentReview, ListQuery, ReverseInput,
                ReversePayload, ReverseReview, ReviewedPayload,
            },
        },
        purchasing::service::amount,
        shipments::repo::book_today,
        stock::value,
    },
    money::minor,
};

const SOURCE: &str = "purchase_cost_adjustment";

/// (adjustment, id, line_no, receipt line, inventory, cogs, value entry).
type LineRow = (Uuid, Uuid, i32, Uuid, i64, i64, Option<Uuid>);
const NOT_DRAFT: &str = "Only a draft adjustment can be posted.";
const NOT_POSTED: &str = "Only a posted adjustment can be reversed.";

#[derive(FromRow)]
struct AdjRow {
    id: Uuid,
    bill_id: Uuid,
    state: String,
    version: i64,
    reason: String,
    evidence_ref: String,
    accounting_date: Option<NaiveDate>,
    pending: i64,
    posted_effect_id: Option<Uuid>,
    posted_at: Option<chrono::DateTime<chrono::Utc>>,
    reversed_at: Option<chrono::DateTime<chrono::Utc>>,
    reverse_reason: Option<String>,
    created_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Pending variance a bill still holds: Σ posted match variance − Σ posted adjustments.
const PENDING: &str = "(SELECT coalesce((SELECT sum(m.variance) FROM supplier_bill_matches m
                         WHERE m.bill_id = a.bill_id AND m.state = 'posted'), 0)
                     - coalesce((SELECT sum(l.inventory_amount + l.cogs_amount)
                         FROM purchase_cost_adjustment_lines l
                         JOIN purchase_cost_adjustments x ON x.id = l.adjustment_id
                         WHERE x.bill_id = a.bill_id AND x.state = 'posted'), 0))::bigint";

fn select() -> String {
    format!(
        "SELECT a.id, a.bill_id, a.state, a.version, a.reason, a.evidence_ref, a.accounting_date,
                {PENDING} AS pending, a.posted_effect_id, a.posted_at, a.reversed_at,
                a.reverse_reason, a.created_by, a.created_at, count(*) OVER () AS total_rows
         FROM purchase_cost_adjustments a WHERE a.workspace_id = $1"
    )
}

async fn resources(
    conn: &mut PgConnection,
    ws: Uuid,
    rows: Vec<AdjRow>,
) -> Result<Vec<CostAdjustmentResource>, AppError> {
    let fc = functional(&mut *conn, ws).await?;
    let ids: Vec<Uuid> = rows.iter().map(|a| a.id).collect();
    let lines: Vec<LineRow> = sqlx::query_as(
        "SELECT adjustment_id, id, line_no, receipt_line_id, inventory_amount, cogs_amount,
                value_entry_id
         FROM purchase_cost_adjustment_lines WHERE workspace_id = $1 AND adjustment_id = ANY($2)
         ORDER BY adjustment_id, line_no",
    )
    .bind(ws)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let fdp = fc.decimals;
    Ok(rows
        .into_iter()
        .map(|a| CostAdjustmentResource {
            id: a.id,
            workspace_id: ws,
            bill_id: a.bill_id,
            version: a.version,
            reason: a.reason,
            evidence_ref: a.evidence_ref,
            functional_currency: fc.clone(),
            accounting_date: a.accounting_date,
            bill_pending_variance: amount(a.pending, fdp),
            lines: lines
                .iter()
                .filter(|l| l.0 == a.id)
                .map(|l| CostAdjustmentLineResource {
                    id: l.1,
                    line_no: l.2,
                    receipt_line_id: l.3,
                    inventory_amount: amount(l.4, fdp),
                    cogs_amount: amount(l.5, fdp),
                    value_entry_id: l.6,
                })
                .collect(),
            posted_at: a.posted_at,
            reversed_at: a.reversed_at,
            reverse_reason: a.reverse_reason,
            created_by: a.created_by,
            created_at: a.created_at,
            allowed_actions: match a.state.as_str() {
                "draft" => vec!["post"],
                "posted" => vec!["reverse"],
                _ => vec![],
            },
            state: a.state,
        })
        .collect())
}

async fn row(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<AdjRow, AppError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{} AND a.id = $2", select())))
        .bind(ws)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)
}

/// One adjustment; 404 outside the workspace.
pub async fn get(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<CostAdjustmentResource, AppError> {
    let a = row(&mut *conn, ws, id).await?;
    resources(conn, ws, vec![a])
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// GET /purchase-cost-adjustments: newest first.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    q: &ListQuery,
) -> Result<Paginated<CostAdjustmentResource>, AppError> {
    use sqlx::Row;
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let mut conn = state.db.acquire().await?;
    let raw = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{} ORDER BY a.created_at DESC, a.id DESC LIMIT $2 OFFSET $3",
        select()
    )))
    .bind(ws)
    .bind(page.size)
    .bind(page.offset())
    .fetch_all(&mut *conn)
    .await?;
    let total = raw
        .first()
        .map(|r| r.try_get::<i64, _>("total_rows"))
        .transpose()?
        .unwrap_or(0);
    let rows = raw
        .iter()
        .map(AdjRow::from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let data = resources(&mut conn, ws, rows).await?;
    Ok(Paginated::new(data, total, page))
}

/// POST /purchase-cost-adjustments: a draft split of a posted bill's pending variance.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<CostAdjustmentPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_cost_adjustments_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::shared(&mut tx, &[ws]).await?;
    let input: &CostAdjustmentInput = &cmd.input.payload;
    let fc = functional(&mut tx, ws).await?;
    let posted: Option<bool> = sqlx::query_scalar(
        "SELECT state = 'posted' FROM supplier_bills WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(input.bill)
    .fetch_optional(&mut *tx)
    .await?;
    match posted {
        None => return Err(AppError::NotFound),
        Some(false) => {
            return Err(AppError::field(
                "payload.bill_id",
                "Only a posted bill's pending variance can be split.",
            ));
        }
        Some(true) => {}
    }
    let dp = u32::try_from(fc.decimals).unwrap_or(0);
    let mut r = Rules::default();
    let mut lines = Vec::with_capacity(input.allocations.len());
    for (i, (line, inv, cogs)) in input.allocations.iter().enumerate() {
        let of_bill: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM supplier_bill_matches m
                 WHERE m.workspace_id = $1 AND m.bill_id = $2 AND m.state = 'posted'
                   AND (m.receipt_line_id = $3 OR EXISTS (
                        SELECT 1 FROM purchase_landed_cost_allocations a
                        WHERE a.landed_cost_id = m.landed_cost_id AND a.receipt_line_id = $3)))",
        )
        .bind(ws)
        .bind(input.bill)
        .bind(line)
        .fetch_one(&mut *tx)
        .await?;
        r.check(
            &format!("payload.allocations.{i}.receipt_line_id"),
            of_bill,
            "The receipt line is not on this bill.",
        );
        let to_minor = |d: Decimal, f: &str, r: &mut Rules| match minor(d, dp) {
            Ok(m) => Some(m),
            Err(_) => {
                r.add(
                    &format!("payload.allocations.{i}.{f}"),
                    "The amount doesn't fit the functional currency.",
                );
                None
            }
        };
        if let (Some(a), Some(b)) = (
            to_minor(*inv, "inventory_amount", &mut r),
            to_minor(*cogs, "cogs_amount", &mut r),
        ) {
            lines.push((*line, a, b));
        }
    }
    r.into_result()?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO purchase_cost_adjustments (workspace_id, bill_id, reason, evidence_ref,
             created_by)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(ws)
    .bind(input.bill)
    .bind(&input.reason)
    .bind(&input.evidence_ref)
    .bind(access.user_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO purchase_cost_adjustment_lines (workspace_id, adjustment_id, line_no,
             receipt_line_id, inventory_amount, cogs_amount)
         SELECT $1, $2, n::int, x.line, x.inv, x.cogs
         FROM unnest($3::uuid[], $4::bigint[], $5::bigint[]) WITH ORDINALITY AS x(line, inv, cogs, n)",
    )
    .bind(ws)
    .bind(id)
    .bind(lines.iter().map(|l| l.0).collect::<Vec<_>>())
    .bind(lines.iter().map(|l| l.1).collect::<Vec<_>>())
    .bind(lines.iter().map(|l| l.2).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await?;
    let answer = created(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

struct PostPlan {
    review: CostAdjustmentReview,
    journal: Vec<JournalRow>,
}

/// The posting journal of adjustment `id`, signed in SQL: inventory X, cogs Y, pending −(X+Y).
async fn journal_of(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    flip: bool,
) -> Result<Vec<JournalRow>, AppError> {
    Ok(sqlx::query_as(
        "WITH t AS (SELECT CASE WHEN $3 THEN -1 ELSE 1 END AS s,
                           coalesce(sum(inventory_amount), 0) AS x,
                           coalesce(sum(cogs_amount), 0) AS y
                    FROM purchase_cost_adjustment_lines WHERE workspace_id = $1 AND adjustment_id = $2),
              j AS (SELECT 1 AS o, 'inventory'::text AS role, s * x AS amt FROM t
                    UNION ALL SELECT 2, 'cogs', s * y FROM t
                    UNION ALL SELECT 3, 'pending_purchase_cost_variance', -s * (x + y) FROM t)
         SELECT j.role, NULL::uuid AS account_id, greatest(j.amt, 0)::bigint AS debit,
                greatest(-j.amt, 0)::bigint AS credit
         FROM j WHERE j.amt <> 0 ORDER BY j.o",
    )
    .bind(ws)
    .bind(id)
    .bind(flip)
    .fetch_all(conn)
    .await?)
}

async fn prepare_post(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<PostPlan, AppError> {
    let a = row(&mut *conn, ws, id).await?;
    if a.state != "draft" {
        return Err(AppError::Unprocessable(NOT_DRAFT.into()));
    }
    let fc = functional(&mut *conn, ws).await?;
    let date = book_today(&mut *conn, ws).await?;
    // Same sign as the pending variance, every part too, and no more than it holds (SQL).
    let (total, fits): (i64, bool) = sqlx::query_as(
        "SELECT coalesce(sum(inventory_amount + cogs_amount), 0)::bigint,
                $3::bigint <> 0
                AND bool_and((inventory_amount = 0 OR sign(inventory_amount) = sign($3::bigint))
                             AND (cogs_amount = 0 OR sign(cogs_amount) = sign($3::bigint)))
                AND abs(sum(inventory_amount + cogs_amount)) <= abs($3::bigint)
         FROM purchase_cost_adjustment_lines WHERE workspace_id = $1 AND adjustment_id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(a.pending)
    .fetch_one(&mut *conn)
    .await?;
    if !fits {
        return Err(AppError::field(
            "allocations",
            "Split no more than the bill's pending variance, with its sign.",
        ));
    }
    let journal = journal_of(&mut *conn, ws, id, false).await?;
    let fp: [u8; 32] = Sha256::digest(
        json!({
            "action": "post", "adjustment": id, "version": a.version, "date": date,
            "pending": a.pending,
            "journal": journal.iter().map(|j| json!([j.role, j.debit, j.credit]))
                .collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .into();
    Ok(PostPlan {
        review: CostAdjustmentReview {
            adjustment_id: id,
            version: a.version,
            accounting_date: date,
            bill_pending_variance: amount(a.pending, fc.decimals),
            total: amount(total, fc.decimals),
            journal: journal_preview(&journal, fc.decimals),
            review_fingerprint: hex::encode(fp),
        },
        journal,
    })
}

/// POST /purchase-cost-adjustments/{id}/post/review: no writes.
pub async fn post_review(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
) -> Result<CostAdjustmentReview, AppError> {
    let mut tx = state.db.begin().await?;
    let plan = prepare_post(&mut tx, access.workspace_id, id).await?;
    tx.rollback().await?;
    Ok(plan.review)
}

/// Shared gate, adjustment FOR UPDATE, its bill FOR SHARE, its receipt lines; the version.
async fn lock(conn: &mut PgConnection, ws: Uuid, id: Uuid, expected: i64) -> Result<(), AppError> {
    gate::shared(&mut *conn, &[ws]).await?;
    let (version, bill): (i64, Uuid) = sqlx::query_as(
        "SELECT version, bill_id FROM purchase_cost_adjustments
         WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    if version != expected {
        return Err(stale());
    }
    sqlx::query("SELECT id FROM supplier_bills WHERE workspace_id = $1 AND id = $2 FOR SHARE")
        .bind(ws)
        .bind(bill)
        .execute(&mut *conn)
        .await?;
    let lines: Vec<Uuid> = sqlx::query_scalar(
        "SELECT receipt_line_id FROM purchase_cost_adjustment_lines
         WHERE workspace_id = $1 AND adjustment_id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    lock_receipt_lines(conn, ws, &lines).await
}

/// Inventory parts of adjustment `id` in (product, warehouse) order: (line, product,
/// warehouse, amount, value entry).
async fn inventory_parts(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Vec<(Uuid, Uuid, Uuid, i64, Option<Uuid>)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT l.id, r.product_id, r.warehouse_id, l.inventory_amount, l.value_entry_id
         FROM purchase_cost_adjustment_lines l
         JOIN purchase_receipt_lines r ON r.id = l.receipt_line_id
         WHERE l.workspace_id = $1 AND l.adjustment_id = $2 AND l.inventory_amount <> 0
         ORDER BY r.product_id, r.warehouse_id, l.id",
    )
    .bind(ws)
    .bind(id)
    .fetch_all(conn)
    .await?)
}

fn lines_of(journal: &[JournalRow]) -> Vec<SourceLine> {
    journal
        .iter()
        .map(|j| SourceLine {
            side: role_side(j.role.as_deref(), j.account_id),
            debit: j.debit,
            credit: j.credit,
            relation_id: j.relation_id,
            bank_account_id: None,
        })
        .collect()
}

/// POST /purchase-cost-adjustments/{id}/post: book the reviewed split once.
pub async fn post(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReviewedPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_cost_adjustments_post";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    lock(&mut tx, ws, id, cmd.input.expected_version).await?;
    let plan = prepare_post(&mut tx, ws, id).await?;
    if plan.review.review_fingerprint != cmd.input.payload {
        return Err(stale());
    }
    let date = plan.review.accounting_date;
    for (line, product, warehouse, delta, _) in inventory_parts(&mut tx, ws, id).await? {
        let (entry, _) =
            value::revalue(&mut tx, ws, (product, warehouse), delta, date, (SOURCE, id)).await?;
        sqlx::query(
            "UPDATE purchase_cost_adjustment_lines SET value_entry_id = $3
             WHERE workspace_id = $1 AND id = $2",
        )
        .bind(ws)
        .bind(line)
        .bind(entry)
        .execute(&mut *tx)
        .await?;
    }
    let fp = fp32(&plan.review.review_fingerprint)?;
    let (_, effect) = post_source_lines(
        &mut tx,
        ws,
        &SourceHeader {
            source_kind: SOURCE,
            source_id: id,
            effect_kind: "posted",
            date,
            reason: "purchase cost adjustment",
            fingerprint: &fp,
            by: access.user_id,
        },
        &lines_of(&plan.journal),
    )
    .await?;
    sqlx::query(
        "UPDATE purchase_cost_adjustments SET state = 'posted', accounting_date = $3,
             posted_effect_id = $4, posted_by = $5, posted_at = now(), version = version + 1,
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(date)
    .bind(effect)
    .bind(access.user_id)
    .execute(&mut *tx)
    .await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

struct ReversePlan {
    review: ReverseReview,
    journal: Vec<JournalRow>,
}

async fn prepare_reverse(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    input: &ReverseInput,
) -> Result<ReversePlan, AppError> {
    let a = row(&mut *conn, ws, id).await?;
    if a.state != "posted" {
        return Err(AppError::Unprocessable(NOT_POSTED.into()));
    }
    let fc = functional(&mut *conn, ws).await?;
    if input.date != book_today(&mut *conn, ws).await? {
        return Err(AppError::field(
            "payload.accounting_date",
            "A cost adjustment reverses on the book's business day.",
        ));
    }
    // Every inventory part still the last valued event of its pool.
    let moved: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM purchase_cost_adjustment_lines l
                        JOIN inventory_value_entries e ON e.id = l.value_entry_id
                        JOIN inventory_cost_pools p ON p.id = e.pool_id
                        WHERE l.workspace_id = $1 AND l.adjustment_id = $2
                          AND p.valuation_sequence <> e.sequence)",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if moved {
        return Err(dependent(
            "This stock moved or was valued since. Post a new adjustment instead.",
        ));
    }
    let journal = common::mirror(&mut *conn, ws, a.posted_effect_id).await?;
    let fp: [u8; 32] = Sha256::digest(
        json!({
            "action": "reverse", "adjustment": id, "version": a.version, "date": input.date,
            "reason": input.reason,
            "journal": journal.iter().map(|j| json!([j.account_id, j.debit, j.credit]))
                .collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .into();
    Ok(ReversePlan {
        review: ReverseReview {
            id,
            version: a.version,
            accounting_date: input.date,
            reason: input.reason.clone(),
            journal: Some(journal_preview(&journal, fc.decimals)),
            review_fingerprint: hex::encode(fp),
        },
        journal,
    })
}

/// POST /purchase-cost-adjustments/{id}/reverse/review: no writes.
pub async fn reverse_review(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    input: &ReverseInput,
) -> Result<ReverseReview, AppError> {
    let mut tx = state.db.begin().await?;
    let plan = prepare_reverse(&mut tx, access.workspace_id, id, input).await?;
    tx.rollback().await?;
    Ok(plan.review)
}

/// POST /purchase-cost-adjustments/{id}/reverse: exact undo while nothing depends on it.
pub async fn reverse(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<ReversePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "purchase_cost_adjustments_reverse";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    lock(&mut tx, ws, id, cmd.input.expected_version).await?;
    let (input, fingerprint) = &cmd.input.payload;
    let parts = inventory_parts(&mut tx, ws, id).await?;
    // Pools locked (in order) before the dependency check reads their sequence.
    for (_, product, warehouse, _, _) in &parts {
        if !crate::modules::stock::repo::share_warehouse(&mut tx, ws, *warehouse).await? {
            return Err(AppError::NotFound);
        }
        sqlx::query(
            "SELECT id FROM inventory_cost_pools
             WHERE workspace_id = $1 AND product_id = $2 AND warehouse_id = $3 FOR UPDATE",
        )
        .bind(ws)
        .bind(product)
        .bind(warehouse)
        .execute(&mut *tx)
        .await?;
    }
    let plan = prepare_reverse(&mut tx, ws, id, input).await?;
    if &plan.review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    for (_, product, warehouse, delta, _) in &parts {
        let undo: i64 = sqlx::query_scalar("SELECT -$1::bigint")
            .bind(delta)
            .fetch_one(&mut *tx)
            .await?;
        value::revalue(
            &mut tx,
            ws,
            (*product, *warehouse),
            undo,
            input.date,
            (SOURCE, id),
        )
        .await?;
    }
    let effect = common::post_mirror(
        &mut tx,
        ws,
        (SOURCE, id),
        input,
        &plan.journal,
        None,
        &plan.review.review_fingerprint,
        access.user_id,
    )
    .await?;
    sqlx::query(
        "UPDATE purchase_cost_adjustments SET state = 'reversed', reversal_date = $3,
             reverse_reason = $4, reversal_effect_id = $5, reversed_by = $6, reversed_at = now(),
             version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(input.date)
    .bind(&input.reason)
    .bind(effect)
    .bind(access.user_id)
    .execute(&mut *tx)
    .await?;
    let answer = ok(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
