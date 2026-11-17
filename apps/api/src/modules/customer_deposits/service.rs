//! Deposit receive and apply. Lock order (receive): command row → shared gate → money place
//! (`MoneyLocation::lock`) → accounts/period/effect/journal number → cash ledger row →
//! deposit + open item. Apply: command row → gate → deposit item FOR UPDATE → both items (id
//! order, `settlement::allocate`) → journal. Amount conversion and every sum run in SQL.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{Action, Create},
            journals::{SourceHeader, SourceLine, post_source_lines},
            repo as accounting,
            service::{created, not_configured, ok, stale, unsupported},
            settlement::{self, Located, MoneyLocation, NewAllocation, NewOpenItem, Side},
        },
        customer_deposits::dto::{ApplyInput, DepositInput, DepositResource, DepositReview},
        purchasing::service::amount as fmt,
        shipments::recognition::{check_date, role_account},
        stock::value,
    },
};

const SETUP: &str = "Customer deposits need an active book.";
const FOREIGN: &str = "Deposits are taken in the book's functional currency only.";

/// The functional currency (active book, gate taken) and its decimals.
async fn book(conn: &mut PgConnection, ws: Uuid) -> Result<(Uuid, i16), AppError> {
    if !value::active(&mut *conn, ws).await? {
        return Err(unsupported(SETUP));
    }
    let currency = accounting::settings(&mut *conn, ws, false)
        .await?
        .functional_currency_id
        .ok_or_else(not_configured)?;
    let dp: i16 =
        sqlx::query_scalar("SELECT decimals FROM currencies WHERE workspace_id = $1 AND id = $2")
            .bind(ws)
            .bind(currency)
            .fetch_one(&mut *conn)
            .await?;
    Ok((currency, dp))
}

/// `amount` in minor units of a `dp`-decimals currency (SQL); 422 on `field` when it has more
/// decimals than the currency.
async fn minor(
    conn: &mut PgConnection,
    (a, dp): (Decimal, i16),
    field: &str,
) -> Result<i64, AppError> {
    let m: Option<i64> = sqlx::query_scalar(
        "SELECT CASE WHEN scale(trim_scale($1::numeric)) <= $2
                     THEN ($1::numeric * 10::numeric ^ $2)::bigint END",
    )
    .bind(a)
    .bind(i32::from(dp))
    .fetch_one(conn)
    .await?;
    m.ok_or_else(|| AppError::field(field, format!("The amount may have at most {dp} decimals.")))
}

/// Facts of a receive, checked.
struct Receive {
    minor: i64,
    currency: Uuid,
    located: Located,
    review: DepositReview,
}

fn side_name(s: Side) -> String {
    match s {
        Side::Role(r) => r.to_owned(),
        Side::Account(a) => format!("account:{a}"),
    }
}

async fn prepare_receive(
    conn: &mut PgConnection,
    ws: Uuid,
    i: &DepositInput,
    prefix: &str,
) -> Result<Receive, AppError> {
    let (currency, dp) = book(&mut *conn, ws).await?;
    check_date(
        &mut *conn,
        ws,
        i.received_on,
        &format!("{prefix}received_on"),
    )
    .await?;
    if accounting::relations_found(&mut *conn, ws, &[i.relation])
        .await?
        .is_empty()
    {
        return Err(AppError::field(
            &format!("{prefix}relation_id"),
            "The selected relation is invalid.",
        ));
    }
    let m = minor(&mut *conn, (i.amount, dp), &format!("{prefix}amount")).await?;
    let located = i.location.lock(&mut *conn, ws, currency).await?;
    if located.currency_id != currency {
        return Err(unsupported(FOREIGN));
    }
    if matches!(
        i.location,
        MoneyLocation::CashPlace(crate::modules::cash::dto::Place::Drawer(_))
    ) && located.shift_id.is_none()
    {
        return Err(AppError::field(
            &format!("{prefix}location.cash_place"),
            "Open a shift on this drawer first.",
        ));
    }
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({ "receive": i.relation, "amount": m, "date": i.received_on,
                "location": i.location.columns() })
        .to_string(),
    )
    .into();
    Ok(Receive {
        minor: m,
        currency,
        located,
        review: DepositReview {
            relation_id: i.relation,
            amount: fmt(m, dp),
            accounting_date: i.received_on,
            debit: side_name(located.side),
            credit: "customer_deposits".into(),
            review_fingerprint: hex::encode(fingerprint),
        },
    })
}

/// POST /customer-deposits/review: no writes.
pub async fn receive_review(
    state: &AppState,
    ws: Uuid,
    i: &DepositInput,
) -> Result<DepositReview, AppError> {
    let mut tx = state.db.begin().await?;
    let r = prepare_receive(&mut tx, ws, i, "").await?;
    tx.rollback().await?;
    Ok(r.review)
}

#[derive(sqlx::FromRow)]
struct DepositRow {
    id: Uuid,
    relation_id: Uuid,
    currency_id: Uuid,
    received_on: NaiveDate,
    location_kind: String,
    register_id: Option<Uuid>,
    vault_id: Option<Uuid>,
    bank_account_id: Option<Uuid>,
    reference: Option<String>,
    evidence_ref: Option<String>,
    confirmation_method: Option<String>,
    journal_entry_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    amount: i64,
    residual: i64,
    decimals: i16,
    version: i64,
}

/// One deposit with its open item; 404 outside the workspace.
pub async fn get(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<DepositResource, AppError> {
    let r: DepositRow = sqlx::query_as(
        "SELECT d.id, d.relation_id, d.currency_id, d.received_on, d.location_kind, d.register_id,
                d.vault_id, d.bank_account_id, d.reference, d.evidence_ref, d.confirmation_method,
                d.journal_entry_id, d.created_at,
                d.amount, i.residual, c.decimals, i.version
         FROM customer_deposits d
         JOIN currencies c ON c.workspace_id = d.workspace_id AND c.id = d.currency_id
         JOIN accounting_open_items i ON i.workspace_id = d.workspace_id
              AND i.source_kind = 'customer_deposit' AND i.source_id = d.id
         WHERE d.workspace_id = $1 AND d.id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(DepositResource {
        id: r.id,
        workspace_id: ws,
        relation_id: r.relation_id,
        currency_id: r.currency_id,
        received_on: r.received_on,
        location_kind: r.location_kind,
        register_id: r.register_id,
        vault_id: r.vault_id,
        bank_account_id: r.bank_account_id,
        reference: r.reference,
        evidence_ref: r.evidence_ref,
        confirmation_method: r.confirmation_method,
        journal_entry_id: r.journal_entry_id,
        created_at: r.created_at,
        amount: fmt(r.amount, r.decimals),
        residual: fmt(r.residual, r.decimals),
        version: r.version,
    })
}

/// POST /customer-deposits: receive a reviewed deposit once.
pub async fn receive(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<crate::modules::customer_deposits::dto::DepositPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "customer_deposits_receive";
    let ws = access.workspace_id;
    let by = access.user_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, by, &cmd).await? {
        return Ok(a);
    }
    let (i, fingerprint) = &cmd.input.payload;
    let r = prepare_receive(&mut tx, ws, i, "payload.").await?;
    if &r.review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    let id = Uuid::now_v7();
    let digest: [u8; 32] = Sha256::digest(fingerprint.as_bytes()).into();
    let header = SourceHeader {
        source_kind: "customer_deposit",
        source_id: id,
        effect_kind: "received",
        date: i.received_on,
        reason: "customer deposit",
        fingerprint: &digest,
        by,
    };
    let cols = i.location.columns();
    let lines = [
        SourceLine {
            side: r.located.side,
            debit: r.minor,
            credit: 0,
            relation_id: None,
            bank_account_id: cols.bank_account_id,
        },
        SourceLine {
            side: Side::Role("customer_deposits"),
            debit: 0,
            credit: r.minor,
            relation_id: Some(i.relation),
            bank_account_id: None,
        },
    ];
    let (entry, effect) = post_source_lines(&mut tx, ws, &header, &lines).await?;
    if cols.location_kind == "cash_place" {
        // The money is in the drawer or vault: its place balance moves too.
        sqlx::query(
            "INSERT INTO cash_ledger (workspace_id, register_id, shift_id, vault_id, kind, amount,
                 actor_kind, member_user_id, source_kind, source_id, effect_id)
             VALUES ($1, $2, $3, $4, 'receipt', $5, 'member', $6, 'customer_deposit', $7, $8)",
        )
        .bind(ws)
        .bind(cols.register_id)
        .bind(r.located.shift_id)
        .bind(cols.vault_id)
        .bind(r.minor)
        .bind(by)
        .bind(id)
        .bind(effect)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO customer_deposits (id, workspace_id, relation_id, currency_id, amount,
             received_on, location_kind, register_id, vault_id, bank_account_id, reference,
             evidence_ref, confirmation_method, client_ref, journal_entry_id, effect_id,
             created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
    )
    .bind(id)
    .bind(ws)
    .bind(i.relation)
    .bind(r.currency)
    .bind(r.minor)
    .bind(i.received_on)
    .bind(cols.location_kind)
    .bind(cols.register_id)
    .bind(cols.vault_id)
    .bind(cols.bank_account_id)
    .bind(cols.reference)
    .bind(cols.evidence_ref)
    .bind(cols.confirmation_method)
    .bind(cmd.input.client_ref)
    .bind(entry)
    .bind(effect)
    .bind(by)
    .execute(&mut *tx)
    .await?;
    let account_id = role_account(&mut tx, ws, "customer_deposits").await?;
    settlement::open_item(
        &mut tx,
        ws,
        &NewOpenItem {
            classification: "customer_deposit",
            side: "credit",
            account_role: "customer_deposits",
            account_id,
            source_kind: "customer_deposit",
            source_id: id,
            relation_id: Some(i.relation),
            order_id: None,
            invoice_id: None,
            recognition_id: None,
            currency_id: r.currency,
            issue_date: i.received_on,
            due_date: None,
            origin: "posted",
            amount: r.minor,
            functional_amount: r.minor,
            effect_id: effect,
            by,
        },
    )
    .await?;
    settlement::link_source(
        &mut tx,
        ws,
        "customer_deposit",
        id,
        Some(effect),
        "posted",
        &digest,
    )
    .await?;
    let answer = created(&get(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Facts of an apply, checked: (deposit item, version, recognition item, relation, minor,
/// currency, date, review).
struct Apply {
    deposit_item: Uuid,
    version: i64,
    target: Uuid,
    relation: Option<Uuid>,
    minor: i64,
    currency: Uuid,
    date: NaiveDate,
    review: DepositReview,
}

async fn prepare_apply(
    conn: &mut PgConnection,
    ws: Uuid,
    deposit: Uuid,
    i: &ApplyInput,
    prefix: &str,
) -> Result<Apply, AppError> {
    let (currency, dp) = book(&mut *conn, ws).await?;
    let (deposit_item, version, relation): (Uuid, i64, Option<Uuid>) = sqlx::query_as(
        "SELECT id, version, relation_id FROM accounting_open_items
         WHERE workspace_id = $1 AND source_kind = 'customer_deposit' AND source_id = $2
         FOR UPDATE",
    )
    .bind(ws)
    .bind(deposit)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let target: Uuid = sqlx::query_scalar(
        "SELECT id FROM accounting_open_items
         WHERE workspace_id = $1 AND recognition_id = $2
           AND account_role = 'unbilled_receivables'",
    )
    .bind(ws)
    .bind(i.recognition)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| {
        AppError::field(
            &format!("{prefix}recognition_id"),
            "The selected recognised sale is invalid.",
        )
    })?;
    let m = minor(&mut *conn, (i.amount, dp), &format!("{prefix}amount")).await?;
    let date = crate::modules::shipments::repo::book_today(&mut *conn, ws).await?;
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({ "apply": deposit, "version": version, "recognition": i.recognition, "amount": m,
                "date": date })
        .to_string(),
    )
    .into();
    Ok(Apply {
        deposit_item,
        version,
        target,
        relation,
        minor: m,
        currency,
        date,
        review: DepositReview {
            relation_id: relation.unwrap_or_default(),
            amount: fmt(m, dp),
            accounting_date: date,
            debit: "customer_deposits".into(),
            credit: "unbilled_receivables".into(),
            review_fingerprint: hex::encode(fingerprint),
        },
    })
}

/// POST /customer-deposits/{id}/apply/review: no writes.
pub async fn apply_review(
    state: &AppState,
    ws: Uuid,
    deposit: Uuid,
    i: &ApplyInput,
) -> Result<DepositReview, AppError> {
    let mut tx = state.db.begin().await?;
    let a = prepare_apply(&mut tx, ws, deposit, i, "").await?;
    tx.rollback().await?;
    Ok(a.review)
}

/// POST /customer-deposits/{id}/apply: apply part or all of a deposit to a recognised,
/// unbilled sale of its customer once.
pub async fn apply(
    state: &AppState,
    access: &WorkspaceAccess,
    deposit: Uuid,
    cmd: Command<Action<crate::modules::customer_deposits::dto::ApplyPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "customer_deposits_apply";
    let ws = access.workspace_id;
    let by = access.user_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, by, &cmd).await? {
        return Ok(a);
    }
    let (i, fingerprint) = &cmd.input.payload;
    let a = prepare_apply(&mut tx, ws, deposit, i, "payload.").await?;
    if a.version != cmd.input.expected_version || &a.review.review_fingerprint != fingerprint {
        return Err(stale());
    }
    let application = Uuid::now_v7();
    let digest: [u8; 32] = Sha256::digest(fingerprint.as_bytes()).into();
    let header = SourceHeader {
        source_kind: "deposit_application",
        source_id: application,
        effect_kind: "applied",
        date: a.date,
        reason: "deposit applied",
        fingerprint: &digest,
        by,
    };
    let lines = [
        SourceLine {
            side: Side::Role("customer_deposits"),
            debit: a.minor,
            credit: 0,
            relation_id: a.relation,
            bank_account_id: None,
        },
        SourceLine {
            side: Side::Role("unbilled_receivables"),
            debit: 0,
            credit: a.minor,
            relation_id: a.relation,
            bank_account_id: None,
        },
    ];
    let (_, effect) = post_source_lines(&mut tx, ws, &header, &lines).await?;
    settlement::allocate(
        &mut tx,
        ws,
        &NewAllocation {
            kind: "application",
            source_item_id: Some(a.deposit_item),
            target_item_id: a.target,
            date: a.date,
            currency_id: a.currency,
            amount: a.minor,
            effect_id: effect,
            by,
        },
    )
    .await?;
    let answer = ok(&get(&mut tx, ws, deposit).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[deposit, application]).await?;
    tx.commit().await?;
    Ok(answer)
}
