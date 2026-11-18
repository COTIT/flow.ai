//! Bank statements and statement-line reconciliation (Flow ERP M5-C, masterplan §13 BANKING):
//! the `bank_statement` import profile (header mapping, row checks, one-transaction apply),
//! matching a statement line to posted bank journal lines under shared capacity, typed
//! unmatch, and reads with residuals and states computed in SQL. Active books only.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Row};
use std::collections::HashMap;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{
    dto::{Acted, Action, Create, Created, date, uuid},
    service::{created, ok, stale, unsupported},
};
use crate::{
    error::AppError,
    http::{
        json::{Rules, Validate},
        list::{Page, Paginated},
        workspace::WorkspaceAccess,
    },
    idempotency::{self, Answer, Command},
    modules::stock::value,
};
use axum::http::StatusCode;

/// Import profile name.
pub const PROFILE: &str = "bank_statement";
/// Columns a statement row may carry; the bank transaction id travels as `external_key`.
pub const ROW_FIELDS: &[&str] = &["booking_date", "amount", "description"];
/// Native permission of statement imports and matching.
pub const PERMISSION: &str = "accounting.reconcile";

const SETUP: &str = "Bank statements are reconciled here once the accounting book is active.";

fn reason(status: StatusCode, code: &'static str, message: &str) -> AppError {
    AppError::Reason {
        status,
        code,
        message: message.into(),
    }
}

fn refused(code: &'static str, message: &str) -> AppError {
    reason(StatusCode::UNPROCESSABLE_ENTITY, code, message)
}

/// 422 `unsupported_active_erp_operation` in a setup book (takes the shared gate).
async fn require_active(conn: &mut PgConnection, ws: Uuid) -> Result<(), AppError> {
    if value::active(&mut *conn, ws).await? {
        Ok(())
    } else {
        Err(unsupported(SETUP))
    }
}

fn signed(r: &mut Rules, field: &str, v: &Value) -> Option<Decimal> {
    r.decimal(field, v, -crate::money::MAX, crate::money::MAX)
}

/// Minor units of `d` at `dp`, the refusal named `field`.
fn minor(d: Decimal, dp: i16, field: &str) -> Result<i64, AppError> {
    crate::money::minor(d, u32::try_from(dp).unwrap_or(0)).map_err(|e| match e {
        AppError::Validation(m) => {
            AppError::field(field, m.into_values().flatten().next().unwrap_or_default())
        }
        e => e,
    })
}

// --- import profile ----------------------------------------------------------------------

/// Normalized `bank_statement` mapping, shape only (the bank account is checked at create).
pub fn mapping(r: &mut Rules, m: &Value) -> Option<Value> {
    let s = |k: &str| m[k].as_str().map(str::to_owned);
    let bank = uuid(r, "mapping.bank_account_id", s("bank_account_id"));
    let reference = r.text("mapping.reference", s("reference"), 255);
    let start = date(r, "mapping.start_date", s("start_date"));
    let end = date(r, "mapping.end_date", s("end_date"));
    if let (Some(a), Some(b)) = (start, end) {
        r.check(
            "mapping.end_date",
            a <= b,
            "The end date must be on or after the start date.",
        );
    }
    let opening = signed(r, "mapping.opening_balance", &m["opening_balance"]);
    let closing = signed(r, "mapping.closing_balance", &m["closing_balance"]);
    Some(json!({
        "bank_account_id": bank?, "reference": reference, "start_date": start?, "end_date": end?,
        "opening_balance": opening?.to_string(), "closing_balance": closing?.to_string(),
    }))
}

/// A statement header resolved against the book.
pub struct Header {
    bank: Uuid,
    reference: String,
    start: NaiveDate,
    end: NaiveDate,
    opening: i64,
    closing: i64,
    dp: i16,
}

fn parse<T: std::str::FromStr>(m: &Value, k: &str) -> Result<T, AppError> {
    m[k].as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AppError::field(&format!("payload.mapping.{k}"), "This value is invalid."))
}

/// Resolve `mapping` in an active book: 422 setup book, unknown bank account (field),
/// `currency_mismatch` for a bank account outside the functional currency, amount precision.
pub async fn header(conn: &mut PgConnection, ws: Uuid, m: &Value) -> Result<Header, AppError> {
    require_active(&mut *conn, ws).await?;
    let bank: Uuid = parse(m, "bank_account_id")?;
    let found: Option<(i16, bool)> = sqlx::query_as(
        "SELECT c.decimals, b.currency_id = s.functional_currency_id
         FROM bank_accounts b
         JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
         JOIN accounting_settings s ON s.workspace_id = b.workspace_id
         WHERE b.workspace_id = $1 AND b.id = $2",
    )
    .bind(ws)
    .bind(bank)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((dp, functional)) = found else {
        return Err(AppError::field(
            "payload.mapping.bank_account_id",
            "The selected bank account is invalid.",
        ));
    };
    if !functional {
        // shortcut: source journals hold functional amounts only, upgrade when they store bank-currency amounts.
        return Err(refused(
            "currency_mismatch",
            "Only bank accounts in the book's functional currency can be reconciled yet.",
        ));
    }
    Ok(Header {
        bank,
        reference: m["reference"].as_str().unwrap_or_default().to_owned(),
        start: parse(m, "start_date")?,
        end: parse(m, "end_date")?,
        opening: minor(
            parse(m, "opening_balance")?,
            dp,
            "payload.mapping.opening_balance",
        )?,
        closing: minor(
            parse(m, "closing_balance")?,
            dp,
            "payload.mapping.closing_balance",
        )?,
        dp,
    })
}

/// One staged statement row parsed for storage.
struct Parsed {
    date: NaiveDate,
    amount: i64,
    description: Option<String>,
    transaction: Option<String>,
}

fn parse_row(h: &Header, fields: &Value, key: Option<&str>) -> Result<Parsed, AppError> {
    let mut r = Rules::default();
    let d = date(
        &mut r,
        "booking_date",
        fields["booking_date"].as_str().map(str::to_owned),
    );
    if let Some(d) = d {
        r.check(
            "booking_date",
            (h.start..=h.end).contains(&d),
            "The booking date must fall inside the statement dates.",
        );
    }
    let amount = match &fields["amount"] {
        Value::Null => {
            r.missing("amount");
            None
        }
        v => signed(&mut r, "amount", v),
    };
    r.into_result()?;
    let amount = minor(amount.unwrap_or_default(), h.dp, "amount")?;
    if amount == 0 {
        return Err(AppError::field("amount", "The amount must not be zero."));
    }
    Ok(Parsed {
        date: d.unwrap_or_default(),
        amount,
        description: fields["description"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        transaction: key
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    })
}

/// Check every staged row `(fields, external_key)` of one statement: per-row rules, bank ids
/// once (file and stored), reference once per account, opening + Σ = closing (SQL). One issue
/// per row, `None` when valid.
pub async fn check_rows(
    conn: &mut PgConnection,
    ws: Uuid,
    h: &Header,
    rows: &[(Value, Option<String>)],
) -> Result<(Vec<Option<Value>>, Vec<Option<ParsedRow>>), AppError> {
    let issue = crate::modules::imports::profiles::issue;
    let mut issues: Vec<Option<Value>> = Vec::with_capacity(rows.len());
    let mut parsed: Vec<Option<Parsed>> = Vec::with_capacity(rows.len());
    for (fields, key) in rows {
        match parse_row(h, fields, key.as_deref()) {
            Ok(p) => {
                issues.push(None);
                parsed.push(Some(p));
            }
            Err(e) => {
                issues.push(Some(issue(&e)));
                parsed.push(None);
            }
        }
    }
    let ids: Vec<String> = parsed
        .iter()
        .flatten()
        .filter_map(|p| p.transaction.clone())
        .collect();
    let stored: Vec<String> = sqlx::query_scalar(
        "SELECT transaction_id FROM bank_statement_lines
         WHERE workspace_id = $1 AND bank_account_id = $2 AND transaction_id = ANY($3)",
    )
    .bind(ws)
    .bind(h.bank)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for id in &ids {
        *seen.entry(id).or_default() += 1;
    }
    let duplicate = json!({"code": "duplicate_key",
        "message": "This bank transaction id is already on a statement or twice in this file."});
    for (i, p) in parsed.iter().enumerate() {
        if let Some(t) = p.as_ref().and_then(|p| p.transaction.as_deref())
            && (seen[t] > 1 || stored.iter().any(|s| s == t))
        {
            issues[i] = Some(duplicate.clone());
        }
    }
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM bank_statements
                        WHERE workspace_id = $1 AND bank_account_id = $2 AND reference = $3)",
    )
    .bind(ws)
    .bind(h.bank)
    .bind(&h.reference)
    .fetch_one(&mut *conn)
    .await?;
    let amounts: Vec<i64> = parsed.iter().flatten().map(|p| p.amount).collect();
    let balanced: bool = sqlx::query_scalar(
        "SELECT $1::numeric + coalesce((SELECT sum(a::numeric) FROM unnest($2::bigint[]) a), 0)
                = $3::numeric",
    )
    .bind(h.opening)
    .bind(&amounts)
    .bind(h.closing)
    .fetch_one(&mut *conn)
    .await?;
    let whole = if taken {
        Some(json!({"code": "duplicate_statement",
            "message": "This bank account already has a statement with this reference."}))
    } else if !balanced && issues.iter().all(Option::is_none) {
        Some(json!({"code": "statement_unbalanced",
            "message": "Opening balance plus the statement lines must equal the closing balance."}))
    } else {
        None
    };
    if let Some(w) = whole {
        issues.iter_mut().for_each(|i| *i = Some(w.clone()));
    }
    let parsed = parsed
        .into_iter()
        .zip(&issues)
        .map(|(p, i)| p.filter(|_| i.is_none()).map(ParsedRow))
        .collect();
    Ok((issues, parsed))
}

/// A valid statement row, ready to store.
pub struct ParsedRow(Parsed);

/// Store the statement of batch `batch` (current revision, every row valid) in the caller's
/// transaction; marks the rows applied. All or nothing (masterplan: no partial statement).
pub async fn apply(
    conn: &mut PgConnection,
    ws: Uuid,
    actor: Uuid,
    batch: Uuid,
    mapping: &Value,
) -> Result<Uuid, AppError> {
    let h = header(&mut *conn, ws, mapping).await?;
    let (file_hash, revision): (String, i32) = sqlx::query_as(
        "SELECT file_hash, revision FROM import_batches WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(batch)
    .fetch_one(&mut *conn)
    .await?;
    let staged = sqlx::query(
        "SELECT id, row_number, fields, external_key, status FROM import_rows
         WHERE batch_id = $1 AND revision = $2 ORDER BY row_number FOR UPDATE",
    )
    .bind(batch)
    .bind(revision)
    .fetch_all(&mut *conn)
    .await?;
    if staged.iter().any(|r| r.get::<String, _>(4) != "valid") {
        return Err(AppError::Unprocessable(
            "Every statement row must be valid. Correct the file and start a new import.".into(),
        ));
    }
    let rows: Vec<(Value, Option<String>)> = staged.iter().map(|r| (r.get(2), r.get(3))).collect();
    // Recheck under the gate: another statement may have taken a bank id since validation.
    let (issues, parsed) = check_rows(&mut *conn, ws, &h, &rows).await?;
    if let Some(i) = issues.into_iter().flatten().next() {
        return Err(AppError::Unprocessable(
            i["message"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    let parsed: Vec<Parsed> = parsed.into_iter().flatten().map(|p| p.0).collect();
    let statement: Uuid = sqlx::query_scalar(
        "INSERT INTO bank_statements (workspace_id, bank_account_id, import_batch_id, file_hash,
             reference, start_date, end_date, opening_balance, closing_balance, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(ws)
    .bind(h.bank)
    .bind(batch)
    .bind(&file_hash)
    .bind(&h.reference)
    .bind(h.start)
    .bind(h.end)
    .bind(h.opening)
    .bind(h.closing)
    .bind(actor)
    .fetch_one(&mut *conn)
    .await?;
    let row_ids: Vec<Uuid> = staged.iter().map(|r| r.get(0)).collect();
    let numbers: Vec<i32> = staged.iter().map(|r| r.get(1)).collect();
    sqlx::query(
        "WITH l AS (
             INSERT INTO bank_statement_lines (workspace_id, statement_id, bank_account_id,
                 row_number, import_row_id, transaction_id, booking_date, description, amount)
             SELECT $1, $2, $3, u.n, u.row_id, u.tx, u.d, u.descr, u.amount
             FROM unnest($4::uuid[], $5::int[], $6::text[], $7::date[], $8::text[], $9::bigint[])
                  AS u(row_id, n, tx, d, descr, amount)
             RETURNING id, import_row_id)
         UPDATE import_rows r SET status = 'applied', result_id = l.id, applied_at = now()
         FROM l WHERE r.id = l.import_row_id",
    )
    .bind(ws)
    .bind(statement)
    .bind(h.bank)
    .bind(&row_ids)
    .bind(&numbers)
    .bind(
        parsed
            .iter()
            .map(|p| p.transaction.clone())
            .collect::<Vec<_>>(),
    )
    .bind(parsed.iter().map(|p| p.date).collect::<Vec<_>>())
    .bind(
        parsed
            .iter()
            .map(|p| p.description.clone())
            .collect::<Vec<_>>(),
    )
    .bind(parsed.iter().map(|p| p.amount).collect::<Vec<_>>())
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        AppError::unique(
            e,
            "bank_statement_lines_transaction_key",
            "external_key",
            "This bank transaction id is already on a statement.",
        )
    })?;
    Ok(statement)
}

// --- reconciliation ----------------------------------------------------------------------

/// One allocation of a match request.
#[derive(Deserialize, ToSchema)]
pub struct AllocationPayload {
    #[schema(value_type = Uuid)]
    pub journal_line_id: Option<String>,
    /// Exact decimal string in the bank currency, signed like the statement line.
    #[schema(value_type = String)]
    pub amount: Option<Value>,
}

/// POST /accounting/bank-reconciliations payload.
#[derive(Deserialize, ToSchema)]
pub struct ReconcilePayload {
    #[schema(value_type = Uuid)]
    pub statement_line_id: Option<String>,
    pub allocations: Option<Vec<AllocationPayload>>,
}

/// A validated match request; amounts still decimal (bank decimals checked in the service).
pub struct Reconcile {
    line: Uuid,
    allocations: Vec<(Uuid, Decimal)>,
}

impl Validate for ReconcilePayload {
    type Valid = Reconcile;

    fn validate(self, r: &mut Rules) -> Option<Reconcile> {
        let line = uuid(r, "statement_line_id", self.statement_line_id);
        let items = self.allocations.unwrap_or_default();
        r.check(
            "allocations",
            (1..=50).contains(&items.len()),
            "Give 1 to 50 journal lines.",
        );
        let mut out = Vec::with_capacity(items.len());
        for (i, a) in items.into_iter().enumerate() {
            let id = uuid(
                r,
                &format!("allocations.{i}.journal_line_id"),
                a.journal_line_id,
            );
            let f = format!("allocations.{i}.amount");
            let amount = match a.amount {
                Some(v) => signed(r, &f, &v),
                None => {
                    r.missing(&f);
                    None
                }
            };
            if let Some(id) = id {
                r.check(
                    &format!("allocations.{i}.journal_line_id"),
                    !out.iter().any(|(o, _)| *o == id),
                    "Each journal line appears once.",
                );
                out.push((id, amount.unwrap_or_default()));
            }
        }
        Some(Reconcile {
            line: line?,
            allocations: out,
        })
    }
}

/// POST /accounting/bank-reconciliations/{id}/reverse payload.
#[derive(Deserialize, ToSchema)]
pub struct UnmatchPayload {
    pub reason: Option<String>,
}

impl Validate for UnmatchPayload {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        Some(r.text("reason", self.reason, 1000))
    }
}

/// Reconciliation resource: allocations in exact bank-currency strings.
async fn resource(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<Value, AppError> {
    sqlx::query_scalar(
        "SELECT jsonb_build_object('id', r.id, 'statement_line_id', r.statement_line_id,
                    'statement_id', l.statement_id, 'version', r.version,
                    'created_by', r.created_by, 'created_at', r.created_at,
                    'reversed_at', r.reversed_at, 'reversed_by', r.reversed_by,
                    'reverse_reason', r.reverse_reason,
                    'allocations', (SELECT jsonb_agg(jsonb_build_object(
                            'journal_line_id', a.journal_line_id,
                            'amount', bank_amount_text(a.amount, c.decimals)) ORDER BY a.journal_line_id)
                        FROM bank_reconciliation_allocations a WHERE a.reconciliation_id = r.id))
         FROM bank_reconciliations r
         JOIN bank_statement_lines l ON l.id = r.statement_line_id
         JOIN bank_accounts b ON b.workspace_id = l.workspace_id AND b.id = l.bank_account_id
         JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
         WHERE r.workspace_id = $1 AND r.id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// POST /accounting/bank-reconciliations: match one statement line to posted bank journal
/// lines (on the bank's GL account and tagged with the bank account). Σ allocations = line
/// amount exactly; each within the journal line's remaining capacity; same currency; never a
/// reversed or reversal entry. Posts no cash.
pub async fn reconcile(
    db: &PgPool,
    access: &WorkspaceAccess,
    cmd: Command<Create<ReconcilePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "bank_reconciliations_create";
    let ws = access.workspace_id;
    let mut tx = db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    require_active(&mut tx, ws).await?;
    let Created {
        payload: Reconcile { line, allocations },
        ..
    } = &cmd.input;
    // Lock order: statement line, then journal lines by id.
    let (bank, line_amount, dp, live): (Uuid, i64, i16, bool) = sqlx::query_as(
        "SELECT l.bank_account_id, l.amount, c.decimals,
                EXISTS (SELECT 1 FROM bank_reconciliations r
                        WHERE r.statement_line_id = l.id AND r.reversed_at IS NULL)
         FROM bank_statement_lines l
         JOIN bank_accounts b ON b.workspace_id = l.workspace_id AND b.id = l.bank_account_id
         JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
         WHERE l.workspace_id = $1 AND l.id = $2 FOR UPDATE OF l",
    )
    .bind(ws)
    .bind(line)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    if live {
        return Err(reason(
            StatusCode::CONFLICT,
            "already_reconciled",
            "This statement line is already matched. Reverse that match first.",
        ));
    }
    let mut ids = Vec::with_capacity(allocations.len());
    let mut amounts = Vec::with_capacity(allocations.len());
    for (i, (id, amount)) in allocations.iter().enumerate() {
        ids.push(*id);
        amounts.push(minor(
            *amount,
            dp,
            &format!("payload.allocations.{i}.amount"),
        )?);
    }
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    let locked: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM journal_lines WHERE workspace_id = $1 AND id = ANY($2)
         ORDER BY id FOR NO KEY UPDATE",
    )
    .bind(ws)
    .bind(&sorted)
    .fetch_all(&mut *tx)
    .await?;
    if locked.len() != sorted.len() {
        return Err(AppError::NotFound);
    }
    let checks = sqlx::query(
        "SELECT e.state = 'posted' AND j.bank_account_id IS NOT DISTINCT FROM $3
                    AND j.account_id = b.account_id,
                e.currency_id = b.currency_id,
                bank_journal_reversed($1, e.id),
                sign(u.amount) = sign(rem.left) AND abs(u.amount::numeric) <= abs(rem.left),
                sum(u.amount::numeric) OVER () = $5::numeric
         FROM unnest($2::uuid[], $4::bigint[]) WITH ORDINALITY AS u(id, amount, ord)
         JOIN journal_lines j ON j.workspace_id = $1 AND j.id = u.id
         JOIN journal_entries e ON e.workspace_id = $1 AND e.id = j.entry_id
         JOIN bank_accounts b ON b.workspace_id = $1 AND b.id = $3
         CROSS JOIN LATERAL (
             SELECT (j.debit - j.credit)::numeric - coalesce(sum(a.amount::numeric), 0) AS left
             FROM bank_reconciliation_allocations a
             JOIN bank_reconciliations r ON r.id = a.reconciliation_id AND r.reversed_at IS NULL
             WHERE a.workspace_id = $1 AND a.journal_line_id = j.id) rem
         ORDER BY u.ord",
    )
    .bind(ws)
    .bind(&ids)
    .bind(bank)
    .bind(&amounts)
    .bind(line_amount)
    .fetch_all(&mut *tx)
    .await?;
    let all = |col: usize| checks.iter().all(|c| c.get::<bool, _>(col));
    if !all(0) {
        return Err(refused(
            "not_this_bank_account",
            "Match only posted journal lines on this statement's bank account.",
        ));
    }
    if !all(1) {
        return Err(refused(
            "currency_mismatch",
            "The journal line is in another currency than the bank account.",
        ));
    }
    if checks.iter().any(|c| c.get::<bool, _>(2)) {
        return Err(refused(
            "journal_line_reversed",
            "This journal line was reversed or is a reversal; it can't be matched.",
        ));
    }
    if !all(4) {
        return Err(refused(
            "amount_mismatch",
            "The matched amounts must add up to the statement line exactly.",
        ));
    }
    if !all(3) {
        return Err(refused(
            "capacity_exceeded",
            "A journal line has less left to match than this amount.",
        ));
    }
    let id: Uuid = sqlx::query_scalar(
        "WITH r AS (INSERT INTO bank_reconciliations (workspace_id, statement_line_id, created_by)
                    VALUES ($1, $2, $3) RETURNING id),
              a AS (INSERT INTO bank_reconciliation_allocations
                        (workspace_id, reconciliation_id, journal_line_id, amount)
                    SELECT $1, r.id, u.id, u.amount FROM r, unnest($4::uuid[], $5::bigint[]) u(id, amount))
         SELECT id FROM r",
    )
    .bind(ws)
    .bind(line)
    .bind(access.user_id)
    .bind(&ids)
    .bind(&amounts)
    .fetch_one(&mut *tx)
    .await?;
    let answer = created(&resource(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id, *line]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST /accounting/bank-reconciliations/{id}/reverse: typed unmatch; the row stays, its
/// capacity returns. Already reversed: 422.
pub async fn unmatch(
    db: &PgPool,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<UnmatchPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "bank_reconciliations_reverse";
    let ws = access.workspace_id;
    let mut tx = db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    require_active(&mut tx, ws).await?;
    let Acted {
        expected_version,
        payload: why,
        ..
    } = &cmd.input;
    let (version, reversed): (i64, bool) = sqlx::query_as(
        "SELECT version, reversed_at IS NOT NULL FROM bank_reconciliations
         WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    if reversed {
        return Err(refused(
            "already_reversed",
            "This match was already reversed.",
        ));
    }
    if version != *expected_version {
        return Err(stale());
    }
    sqlx::query(
        "UPDATE bank_reconciliations SET reversed_at = now(), reversed_by = $3,
             reverse_reason = $4, version = version + 1
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(access.user_id)
    .bind(why)
    .execute(&mut *tx)
    .await?;
    let answer = ok(&resource(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

// --- reads -------------------------------------------------------------------------------

const STATEMENTS: &str = "
    SELECT jsonb_build_object('id', s.id, 'bank_account_id', s.bank_account_id,
               'currency_id', b.currency_id, 'reference', s.reference,
               'start_date', s.start_date, 'end_date', s.end_date,
               'opening_balance', bank_amount_text(s.opening_balance, c.decimals),
               'closing_balance', bank_amount_text(s.closing_balance, c.decimals),
               'line_count', t.lines, 'reconciled_lines', t.done,
               'unreconciled_lines', t.lines - t.done,
               'unreconciled_amount', bank_amount_text(t.open_amount, c.decimals),
               'state', CASE WHEN t.done = t.lines THEN 'reconciled' ELSE 'open' END,
               'import_batch_id', s.import_batch_id, 'file_hash', s.file_hash,
               'created_by', s.created_by, 'created_at', s.created_at),
           count(*) OVER ()
    FROM bank_statements s
    JOIN bank_accounts b ON b.workspace_id = s.workspace_id AND b.id = s.bank_account_id
    JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
    CROSS JOIN LATERAL (
        SELECT count(*) AS lines, count(r.id) AS done,
               coalesce(sum(l.amount::numeric) FILTER (WHERE r.id IS NULL), 0) AS open_amount
        FROM bank_statement_lines l
        LEFT JOIN bank_reconciliations r ON r.statement_line_id = l.id AND r.reversed_at IS NULL
        WHERE l.statement_id = s.id) t
    WHERE s.workspace_id = $1 AND ($2::uuid IS NULL OR s.id = $2)
    ORDER BY s.end_date DESC, s.id DESC LIMIT $3 OFFSET $4";

/// GET /accounting/bank-statements: newest first.
pub async fn statements(db: &PgPool, ws: Uuid, page: Page) -> Result<Paginated<Value>, AppError> {
    let rows: Vec<(Value, i64)> = sqlx::query_as(STATEMENTS)
        .bind(ws)
        .bind(None::<Uuid>)
        .bind(page.size)
        .bind(page.offset())
        .fetch_all(db)
        .await?;
    let total = match rows.first() {
        Some((_, t)) => *t,
        None => {
            sqlx::query_scalar("SELECT count(*) FROM bank_statements WHERE workspace_id = $1")
                .bind(ws)
                .fetch_one(db)
                .await?
        }
    };
    Ok(Paginated::new(
        rows.into_iter().map(|(v, _)| v).collect(),
        total,
        page,
    ))
}

/// GET /accounting/bank-statements/{id}.
pub async fn statement(db: &PgPool, ws: Uuid, id: Uuid) -> Result<Value, AppError> {
    let row: Option<(Value, i64)> = sqlx::query_as(STATEMENTS)
        .bind(ws)
        .bind(id)
        .bind(1_i64)
        .bind(0_i64)
        .fetch_optional(db)
        .await?;
    row.map(|(v, _)| v).ok_or(AppError::NotFound)
}

/// GET /accounting/bank-statements/{id}/lines: file order, live match and residual per line.
pub async fn lines(
    db: &PgPool,
    ws: Uuid,
    id: Uuid,
    page: Page,
) -> Result<Paginated<Value>, AppError> {
    let total: i64 = sqlx::query_scalar(
        "SELECT count(l.id) FROM bank_statements s
         LEFT JOIN bank_statement_lines l ON l.statement_id = s.id
         WHERE s.workspace_id = $1 AND s.id = $2 GROUP BY s.id",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)?;
    let data: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', l.id, 'statement_id', l.statement_id,
                    'row_number', l.row_number, 'transaction_id', l.transaction_id,
                    'booking_date', l.booking_date, 'description', l.description,
                    'amount', bank_amount_text(l.amount, c.decimals),
                    'matched_amount', bank_amount_text(coalesce(m.total, 0), c.decimals),
                    'residual', bank_amount_text(l.amount - coalesce(m.total, 0), c.decimals),
                    'state', CASE WHEN r.id IS NULL THEN 'unreconciled' ELSE 'reconciled' END,
                    'reconciliation_id', r.id, 'reconciliation_version', r.version,
                    'allocations', coalesce(m.items, '[]'::jsonb))
         FROM bank_statement_lines l
         JOIN bank_accounts b ON b.workspace_id = l.workspace_id AND b.id = l.bank_account_id
         JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
         LEFT JOIN bank_reconciliations r ON r.statement_line_id = l.id AND r.reversed_at IS NULL
         LEFT JOIN LATERAL (
             SELECT sum(a.amount::numeric) AS total,
                    jsonb_agg(jsonb_build_object('journal_line_id', a.journal_line_id,
                        'journal_entry_id', e.id, 'entry_number', e.number,
                        'accounting_date', e.accounting_date,
                        'amount', bank_amount_text(a.amount, c.decimals),
                        'journal_reversed', bank_journal_reversed($1, e.id))
                        ORDER BY a.journal_line_id) AS items
             FROM bank_reconciliation_allocations a
             JOIN journal_lines j ON j.id = a.journal_line_id
             JOIN journal_entries e ON e.id = j.entry_id
             WHERE a.reconciliation_id = r.id) m ON true
         WHERE l.workspace_id = $1 AND l.statement_id = $2
         ORDER BY l.row_number LIMIT $3 OFFSET $4",
    )
    .bind(ws)
    .bind(id)
    .bind(page.size)
    .bind(page.offset())
    .fetch_all(db)
    .await?;
    Ok(Paginated::new(data, total, page))
}
