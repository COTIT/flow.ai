//! Shared payables posting pieces: journal sides from role names, review previews, mirror
//! (typed reversal) journals, and closing an AP-control open item with its opposite twin.

use chrono::NaiveDate;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    error::AppError,
    modules::{
        accounting::{
            journals::{SourceHeader, SourceLine, post_source_lines},
            settlement::{self, NewAllocation, NewOpenItem, Side},
        },
        payables::{
            bills::{JournalRow, PlanRow},
            dto::{JournalPreview, ReverseInput},
        },
        purchasing::service::amount,
    },
};

/// Roles payables journals use (the `Side::Role` names must be 'static).
const ROLES: [&str; 12] = [
    "grni",
    "recoverable_tax",
    "realized_fx",
    "pending_purchase_cost_variance",
    "purchase_return_variance",
    "payables",
    "inventory",
    "cogs",
    "cash",
    "bank",
    "inventory_adjustment",
    "rounding",
];

/// 422 `dependent_effects_exist`.
pub fn dependent(message: &str) -> AppError {
    AppError::Reason {
        status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        code: "dependent_effects_exist",
        message: message.into(),
    }
}

/// Journal side of a planned line: its concrete account, else its mapped role.
pub fn role_side(role: Option<&str>, account: Option<Uuid>) -> Side {
    match (account, role.and_then(|r| ROLES.iter().find(|x| **x == r))) {
        (Some(a), _) => Side::Account(a),
        (None, Some(r)) => Side::Role(r),
        // Unknown role: an account nobody has, refused as not configured by the poster.
        (None, None) => Side::Account(Uuid::nil()),
    }
}

/// Review rows of a planned journal, functional currency at `fdp`.
pub fn journal_preview(rows: &[JournalRow], fdp: i16) -> Vec<JournalPreview> {
    rows.iter()
        .map(|j| JournalPreview {
            role: j.role.clone(),
            account_id: j.account_id,
            debit: amount(j.debit, fdp),
            credit: amount(j.credit, fdp),
        })
        .collect()
}

/// (tax, total) nominal of a bill plan header, summed in SQL.
pub async fn bill_totals(conn: &mut PgConnection, head: &PlanRow) -> Result<(i64, i64), AppError> {
    Ok(
        sqlx::query_as("SELECT ($2::bigint + $3::bigint), ($1::bigint + $2::bigint + $3::bigint)")
            .bind(head.nominal.unwrap_or(0))
            .bind(head.tax_recoverable.unwrap_or(0))
            .bind(head.tax_nonrecoverable.unwrap_or(0))
            .fetch_one(conn)
            .await?,
    )
}

/// Lines of the journal posted under `effect`, sides swapped (the exact compensation).
pub async fn mirror(
    conn: &mut PgConnection,
    ws: Uuid,
    effect: Option<Uuid>,
) -> Result<Vec<JournalRow>, AppError> {
    let effect = effect.ok_or_else(|| AppError::Internal("posted fact without effect".into()))?;
    Ok(sqlx::query_as(
        "SELECT NULL::text AS role, l.account_id, l.functional_credit AS debit,
                l.functional_debit AS credit, l.relation_id
         FROM journal_lines l
         JOIN journal_entries e ON e.workspace_id = l.workspace_id AND e.id = l.entry_id
         WHERE l.workspace_id = $1 AND e.effect_id = $2 ORDER BY e.id, l.line_no",
    )
    .bind(ws)
    .bind(effect)
    .fetch_all(conn)
    .await?)
}

/// Post `journal` as the `reversed` effect of `source`; the effect id.
#[allow(clippy::too_many_arguments)]
pub async fn post_mirror(
    conn: &mut PgConnection,
    ws: Uuid,
    (kind, id): (&str, Uuid),
    input: &ReverseInput,
    journal: &[JournalRow],
    party: Option<Uuid>,
    fingerprint: &str,
    by: Uuid,
) -> Result<Uuid, AppError> {
    let fp = hex::decode(fingerprint)
        .ok()
        .and_then(|v| <[u8; 32]>::try_from(v).ok())
        .ok_or_else(|| AppError::Internal("fingerprint".into()))?;
    let lines: Vec<SourceLine> = journal
        .iter()
        .map(|j| SourceLine {
            side: role_side(j.role.as_deref(), j.account_id),
            debit: j.debit,
            credit: j.credit,
            relation_id: j
                .relation_id
                .or(party.filter(|_| j.role.as_deref() == Some("payables"))),
            bank_account_id: None,
        })
        .collect();
    let (_, effect) = post_source_lines(
        conn,
        ws,
        &SourceHeader {
            source_kind: kind,
            source_id: id,
            effect_kind: "reversed",
            date: input.date,
            reason: &input.reason,
            fingerprint: &fp,
            by,
        },
        &lines,
    )
    .await?;
    Ok(effect)
}

/// Close untouched AP-control item `item` of `source`: open its opposite twin (payable ↔
/// supplier_credit, same account, amount and functional residual) and apply it, fx 0. The
/// reversal journal (`effect`) already moved the GL.
#[allow(clippy::too_many_arguments)]
pub async fn close_item(
    conn: &mut PgConnection,
    ws: Uuid,
    item: Uuid,
    source_kind: &str,
    source_id: Uuid,
    date: NaiveDate,
    effect: Uuid,
    by: Uuid,
) -> Result<(), AppError> {
    let (classification, account, relation, currency, amount, residual, functional): (
        String,
        Uuid,
        Option<Uuid>,
        Uuid,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT classification, account_id, relation_id, currency_id, amount, residual,
                functional_residual
         FROM accounting_open_items WHERE workspace_id = $1 AND id = $2 FOR NO KEY UPDATE",
    )
    .bind(ws)
    .bind(item)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    if residual != amount {
        return Err(dependent("This item is already settled in part."));
    }
    let (twin, side) = match classification.as_str() {
        "payable" => ("supplier_credit", "debit"),
        "supplier_credit" => ("payable", "credit"),
        _ => return Err(AppError::Internal("unexpected item classification".into())),
    };
    let twin = settlement::open_item(
        conn,
        ws,
        &NewOpenItem {
            classification: twin,
            side,
            account_role: "payables",
            account_id: account,
            source_kind,
            source_id,
            relation_id: relation,
            order_id: None,
            invoice_id: None,
            recognition_id: None,
            currency_id: currency,
            issue_date: date,
            due_date: None,
            origin: "posted",
            amount: residual,
            functional_amount: functional,
            effect_id: effect,
            by,
        },
    )
    .await?;
    settlement::allocate(
        conn,
        ws,
        &NewAllocation {
            kind: "application",
            source_item_id: Some(twin),
            target_item_id: item,
            date,
            currency_id: currency,
            amount: residual,
            effect_id: effect,
            by,
        },
    )
    .await?;
    Ok(())
}

/// What applying `minor` of `source` to `target` relieves (the settlement trigger's formula,
/// in SQL) and the realized FX: positive `loss` = the debit side carried more functional than
/// the credit side relieves.
#[derive(sqlx::FromRow, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relief {
    pub minor: i64,
    pub source_functional: i64,
    pub target_functional: i64,
    pub loss: i64,
    pub loss_abs: i64,
}

/// Relief of applying `minor` of item `source` to item `target` now.
pub async fn relief(
    conn: &mut PgConnection,
    ws: Uuid,
    source: Uuid,
    target: Uuid,
    minor: i64,
) -> Result<Relief, AppError> {
    sqlx::query_as(
        "WITH r AS (
             SELECT s.side,
                    CASE WHEN $3 = s.residual THEN s.functional_residual
                         ELSE money_round_div(s.functional_residual::numeric * $3, s.residual)::bigint
                    END AS sf,
                    CASE WHEN $3 = t.residual THEN t.functional_residual
                         ELSE money_round_div(t.functional_residual::numeric * $3, t.residual)::bigint
                    END AS tf
             FROM accounting_open_items s, accounting_open_items t
             WHERE s.workspace_id = $1 AND s.id = $2 AND t.workspace_id = $1 AND t.id = $4
               AND s.residual > 0 AND t.residual > 0)
         SELECT $3::bigint AS minor, sf AS source_functional, tf AS target_functional,
                CASE WHEN side = 'debit' THEN sf - tf ELSE tf - sf END AS loss,
                abs(sf - tf) AS loss_abs
         FROM r",
    )
    .bind(ws)
    .bind(source)
    .bind(minor)
    .bind(target)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| AppError::Unprocessable("Nothing is left open to apply.".into()))
}

/// Realized FX of an application: loss DR realized_fx / CR payables, gain the other way.
pub fn fx_journal(r: &Relief) -> Vec<JournalRow> {
    if r.loss == 0 {
        return vec![];
    }
    let (debit, credit) = if r.loss > 0 {
        ("realized_fx", "payables")
    } else {
        ("payables", "realized_fx")
    };
    let row = |role: &str, debit, credit| JournalRow {
        role: Some(role.into()),
        account_id: None,
        debit,
        credit,
        relation_id: None,
    };
    vec![row(debit, r.loss_abs, 0), row(credit, 0, r.loss_abs)]
}

/// One application under its own effect (`source` identity): the FX journal when there is a
/// difference, else the bare effect; then the allocation, checked against the plan. Its
/// allocation id.
#[allow(clippy::too_many_arguments)]
pub async fn apply(
    conn: &mut PgConnection,
    ws: Uuid,
    (from, to): (Uuid, Uuid),
    r: &Relief,
    (date, currency, supplier): (NaiveDate, Uuid, Uuid),
    (kind, id, effect_kind): (&str, Uuid, &str),
    fingerprint: &[u8; 32],
    by: Uuid,
) -> Result<(Uuid, Uuid), AppError> {
    let journal = fx_journal(r);
    let effect = if journal.is_empty() {
        crate::modules::accounting::repo::insert_effect(
            &mut *conn,
            ws,
            kind,
            id,
            effect_kind,
            fingerprint,
            by,
        )
        .await?
    } else {
        let lines: Vec<SourceLine> = journal
            .iter()
            .map(|j| SourceLine {
                side: role_side(j.role.as_deref(), j.account_id),
                debit: j.debit,
                credit: j.credit,
                relation_id: (j.role.as_deref() == Some("payables")).then_some(supplier),
                bank_account_id: None,
            })
            .collect();
        post_source_lines(
            &mut *conn,
            ws,
            &SourceHeader {
                source_kind: kind,
                source_id: id,
                effect_kind,
                date,
                reason: "supplier settlement applied",
                fingerprint,
                by,
            },
            &lines,
        )
        .await?
        .1
    };
    let allocation = settlement::allocate(
        &mut *conn,
        ws,
        &NewAllocation {
            kind: "application",
            source_item_id: Some(from),
            target_item_id: to,
            date,
            currency_id: currency,
            amount: r.minor,
            effect_id: effect,
            by,
        },
    )
    .await?;
    let (sf, tf): (i64, i64) = sqlx::query_as(
        "SELECT source_functional, target_functional FROM accounting_allocations
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(allocation)
    .fetch_one(&mut *conn)
    .await?;
    if (sf, tf) != (r.source_functional, r.target_functional) {
        return Err(AppError::Internal(
            "allocation relief differs from its plan".into(),
        ));
    }
    Ok((allocation, effect))
}

/// 32-byte fingerprint from its hex.
pub fn fp32(hex_fp: &str) -> Result<[u8; 32], AppError> {
    hex::decode(hex_fp)
        .ok()
        .and_then(|v| <[u8; 32]>::try_from(v).ok())
        .ok_or_else(|| AppError::Internal("fingerprint".into()))
}
