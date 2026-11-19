//! Opening conversion (Flow ERP M6-B, masterplan §14): the four opening import profiles stage
//! typed `accounting_opening_lines` only (D11); `review` recomputes schedules, control and
//! coverage checks and the opening fingerprint without writing. Whole-batch profiles like
//! `bank_statement`: every row valid or nothing stages. Book must be in setup.

use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use utoipa::ToSchema;
use uuid::Uuid;

use super::{
    repo,
    service::{not_configured, stale, unsupported},
};
use crate::{
    AppState,
    error::AppError,
    http::json::{Rules, Validate},
    modules::stock::value,
};

/// Opening import profiles.
pub const PROFILES: [&str; 4] = [
    "opening_inventory",
    "opening_receivables",
    "opening_payables",
    "opening_trial_balance",
];
/// Native permission an opening batch needs besides `import.apply`.
pub const PERMISSION: &str = "accounting.configure";
const ACTIVE: &str = "Opening balances can only be staged before the books are activated.";
const AMOUNTS: [&str; 6] = [
    "face",
    "settled",
    "credited",
    "refunded",
    "outstanding",
    "carrying",
];

/// Whether `profile` is an opening profile.
pub fn is_profile(profile: &str) -> bool {
    PROFILES.contains(&profile)
}

/// Whether opening `profile` accepts column `name`.
pub fn allows(profile: &str, name: &str) -> bool {
    match profile {
        "opening_inventory" => ["warehouse_id", "product_id", "quantity", "value"].contains(&name),
        "opening_receivables" => name == "invoice_number" || AMOUNTS.contains(&name),
        "opening_payables" => name == "bill_number" || AMOUNTS.contains(&name),
        "opening_trial_balance" => [
            "account_code",
            "debit",
            "credit",
            "register_id",
            "vault_id",
            "bank_account_id",
        ]
        .contains(&name),
        _ => false,
    }
}

/// Normalized mapping, shape only: inventory takes `mode` existing|empty, the others nothing.
pub fn mapping(r: &mut Rules, profile: &str, m: &Value) -> Option<Value> {
    if profile == "opening_inventory" {
        let mode = m["mode"].as_str().unwrap_or_default();
        r.check(
            "mapping.mode",
            ["existing", "empty"].contains(&mode),
            "The mode must be existing or empty.",
        );
        return Some(json!({ "mode": mode }));
    }
    r.check(
        "mapping",
        m.as_object().is_some_and(|m| m.is_empty()),
        "Opening imports take no mapping options.",
    );
    Some(json!({}))
}

/// Setup book with a functional currency; its decimals. 422 active / not configured.
pub async fn header(conn: &mut PgConnection, ws: Uuid) -> Result<i16, AppError> {
    if value::active(&mut *conn, ws).await? {
        return Err(unsupported(ACTIVE));
    }
    sqlx::query_scalar(
        "SELECT c.decimals FROM accounting_settings s
         JOIN currencies c ON c.workspace_id = s.workspace_id AND c.id = s.functional_currency_id
         WHERE s.workspace_id = $1",
    )
    .bind(ws)
    .fetch_optional(conn)
    .await?
    .ok_or_else(not_configured)
}

/// One staged line, typed like `accounting_opening_lines`.
#[derive(Default, Clone)]
pub struct Line {
    kind: &'static str,
    mode: Option<String>,
    warehouse: Option<Uuid>,
    product: Option<Uuid>,
    quantity: Option<Decimal>,
    value: Option<i64>,
    invoice: Option<Uuid>,
    bill: Option<Uuid>,
    currency: Option<Uuid>,
    amounts: [i64; 6],
    account: Option<Uuid>,
    debit: Option<i64>,
    credit: Option<i64>,
    register: Option<Uuid>,
    vault: Option<Uuid>,
    bank: Option<Uuid>,
}

impl Line {
    /// The source a row stages once per file.
    fn key(&self) -> Option<String> {
        match self.kind {
            "inventory" => Some(format!("{:?}/{:?}", self.warehouse, self.product)),
            "receivable" => self.invoice.map(|i| i.to_string()),
            "payable" => self.bill.map(|b| b.to_string()),
            _ => None,
        }
    }
}

fn text<'a>(f: &'a Value, k: &str) -> Option<&'a str> {
    f[k].as_str().map(str::trim).filter(|s| !s.is_empty())
}

fn id(f: &Value, k: &str) -> Result<Option<Uuid>, AppError> {
    text(f, k)
        .map(|s| {
            s.parse()
                .map_err(|_| AppError::field(k, "This value is invalid."))
        })
        .transpose()
}

/// Nonnegative amount `k` of `f` in minor units at `dp`; missing reads as zero when `zero`.
fn amount(f: &Value, k: &str, dp: i16, zero: bool) -> Result<i64, AppError> {
    let mut r = Rules::default();
    let d = match text(f, k) {
        None if zero => return Ok(0),
        None => None,
        Some(_) => r.decimal(k, &f[k], Decimal::ZERO, crate::money::MAX),
    };
    let Some(d) = d else {
        return Err(AppError::field(
            k,
            "This amount is required and can't be negative.",
        ));
    };
    crate::money::minor(d, u32::try_from(dp).unwrap_or(0)).map_err(|e| match e {
        AppError::Validation(m) => {
            AppError::field(k, m.into_values().flatten().next().unwrap_or_default())
        }
        e => e,
    })
}

fn invalid(k: &str, what: &str) -> AppError {
    AppError::field(k, format!("No {what} matches this value exactly."))
}

// shortcut: one lookup per row (≤500 rows per batch), batch the lookups if imports grow.
async fn parse_row(
    conn: &mut PgConnection,
    ws: Uuid,
    profile: &str,
    mapping: &Value,
    fdp: i16,
    f: &Value,
) -> Result<Line, AppError> {
    match profile {
        "opening_inventory" => {
            let (warehouse, product) = (id(f, "warehouse_id")?, id(f, "product_id")?);
            let found: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM warehouses WHERE workspace_id = $1 AND id = $2)
                    AND EXISTS (SELECT 1 FROM products WHERE workspace_id = $1 AND id = $3
                                  AND deleted_at IS NULL)",
            )
            .bind(ws)
            .bind(warehouse)
            .bind(product)
            .fetch_one(&mut *conn)
            .await?;
            if !found {
                return Err(invalid("product_id", "warehouse and product"));
            }
            let mut r = Rules::default();
            let quantity = text(f, "quantity")
                .and_then(|_| {
                    r.decimal("quantity", &f["quantity"], Decimal::ZERO, crate::money::MAX)
                })
                .filter(|q| q.normalize().scale() <= 2)
                .ok_or_else(|| {
                    AppError::field(
                        "quantity",
                        "The quantity must be zero or more, with at most 2 decimals.",
                    )
                })?;
            let value = amount(f, "value", fdp, false)?;
            if quantity.is_zero() && value != 0 {
                return Err(AppError::field("value", "No quantity can't carry a value."));
            }
            Ok(Line {
                kind: "inventory",
                mode: mapping["mode"].as_str().map(str::to_owned),
                warehouse,
                product,
                quantity: Some(quantity),
                value: Some(value),
                ..Line::default()
            })
        }
        "opening_receivables" | "opening_payables" => {
            let receivable = profile == "opening_receivables";
            let (k, number) = if receivable {
                ("invoice_number", text(f, "invoice_number"))
            } else {
                ("bill_number", text(f, "bill_number"))
            };
            let number: i64 = number.and_then(|n| n.parse().ok()).ok_or_else(|| {
                invalid(
                    k,
                    if receivable {
                        "invoice"
                    } else {
                        "supplier bill"
                    },
                )
            })?;
            let found: Option<(Uuid, Uuid, i16)> = sqlx::query_as(if receivable {
                "SELECT i.id, i.currency_id, c.decimals FROM invoices i
                 JOIN currencies c ON c.workspace_id = i.workspace_id AND c.id = i.currency_id
                 WHERE i.workspace_id = $1 AND i.invoice_number = $2 AND i.deleted_at IS NULL
                   AND NOT i.is_credit"
            } else {
                "SELECT b.id, b.currency_id, c.decimals FROM supplier_bills b
                 JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
                 WHERE b.workspace_id = $1 AND b.number = $2 AND b.state <> 'reversed'"
            })
            .bind(ws)
            .bind(number)
            .fetch_optional(&mut *conn)
            .await?;
            let (source, currency, dp) = found.ok_or_else(|| {
                invalid(
                    k,
                    if receivable {
                        "invoice"
                    } else {
                        "supplier bill"
                    },
                )
            })?;
            let mut amounts = [0; 6];
            for (i, k) in AMOUNTS.iter().enumerate() {
                let dp = if *k == "carrying" { fdp } else { dp };
                amounts[i] = amount(f, k, dp, matches!(*k, "credited" | "refunded"))?;
            }
            let [face, settled, credited, refunded, outstanding, carrying] = amounts;
            let adds: bool = sqlx::query_scalar(
                "SELECT $1::numeric - $2 - $3 + $4 = $5 AND ($5 = 0) = ($6 = 0)",
            )
            .bind(face)
            .bind(settled)
            .bind(credited)
            .bind(refunded)
            .bind(outstanding)
            .bind(carrying)
            .fetch_one(&mut *conn)
            .await?;
            if !adds {
                return Err(AppError::field(
                    "outstanding",
                    "Face minus settled and credited plus refunded must equal the outstanding amount, and only an outstanding amount carries a value.",
                ));
            }
            Ok(Line {
                kind: if receivable { "receivable" } else { "payable" },
                invoice: receivable.then_some(source),
                bill: (!receivable).then_some(source),
                currency: Some(currency),
                amounts,
                ..Line::default()
            })
        }
        _ => {
            let code = text(f, "account_code").unwrap_or_default();
            let (register, vault, bank) = (
                id(f, "register_id")?,
                id(f, "vault_id")?,
                id(f, "bank_account_id")?,
            );
            // The account, and whether the place fits it: cash places on the cash role,
            // bank accounts on their own GL account.
            let found: Option<(Uuid, bool)> = sqlx::query_as(
                "SELECT a.id,
                        CASE WHEN $3::uuid IS NOT NULL OR $4::uuid IS NOT NULL THEN
                               EXISTS (SELECT 1 FROM account_mappings m WHERE m.workspace_id = $1
                                         AND m.role = 'cash' AND m.account_id = a.id)
                               AND ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM till_registers
                                         WHERE workspace_id = $1 AND id = $3))
                               AND ($4::uuid IS NULL OR EXISTS (SELECT 1 FROM cash_vaults
                                         WHERE workspace_id = $1 AND id = $4))
                             WHEN $5::uuid IS NOT NULL THEN
                               EXISTS (SELECT 1 FROM bank_accounts WHERE workspace_id = $1
                                         AND id = $5 AND account_id = a.id)
                             ELSE true END
                 FROM accounts a WHERE a.workspace_id = $1 AND a.code = $2",
            )
            .bind(ws)
            .bind(code)
            .bind(register)
            .bind(vault)
            .bind(bank)
            .fetch_optional(&mut *conn)
            .await?;
            let (account, place_fits) = found.ok_or_else(|| invalid("account_code", "account"))?;
            if [register, vault, bank].iter().flatten().count() > 1 || !place_fits {
                return Err(AppError::field(
                    "register_id",
                    "Name at most one cash place or bank account, on its own ledger account.",
                ));
            }
            let (debit, credit) = (
                amount(f, "debit", fdp, true)?,
                amount(f, "credit", fdp, true)?,
            );
            if (debit == 0) == (credit == 0) {
                return Err(AppError::field(
                    "debit",
                    "Fill exactly one of debit and credit.",
                ));
            }
            Ok(Line {
                kind: "balance",
                account: Some(account),
                debit: Some(debit),
                credit: Some(credit),
                register,
                vault,
                bank,
                ..Line::default()
            })
        }
    }
}

/// Check every staged row of one opening batch: per-row rules and one row per source. One
/// issue per row (`None` when valid) and the parsed lines.
pub async fn check_rows(
    conn: &mut PgConnection,
    ws: Uuid,
    profile: &str,
    mapping: &Value,
    rows: &[Value],
) -> Result<(Vec<Option<Value>>, Vec<Option<Line>>), AppError> {
    let issue = crate::modules::imports::profiles::issue;
    let fdp = header(&mut *conn, ws).await?;
    let (mut issues, mut lines) = (
        Vec::with_capacity(rows.len()),
        Vec::with_capacity(rows.len()),
    );
    for fields in rows {
        match parse_row(&mut *conn, ws, profile, mapping, fdp, fields).await {
            Ok(l) => {
                issues.push(None);
                lines.push(Some(l));
            }
            Err(e) => {
                issues.push(Some(issue(&e)));
                lines.push(None);
            }
        }
    }
    let keys: Vec<Option<String>> = lines
        .iter()
        .map(|l| l.as_ref().and_then(Line::key))
        .collect();
    for (i, k) in keys.iter().enumerate() {
        if k.is_some() && keys.iter().filter(|o| *o == k).count() > 1 {
            issues[i] = Some(json!({"code": "duplicate_key",
                "message": "Another row in this file stages the same source. Keep one row per source."}));
            lines[i] = None;
        }
    }
    Ok((issues, lines))
}

/// Stage batch `batch` (current revision, every row valid) in the caller's transaction; marks
/// the rows applied. All or nothing. Writes no stock, value, journal or open item.
pub async fn stage(
    conn: &mut PgConnection,
    ws: Uuid,
    actor: Uuid,
    batch: Uuid,
    profile: &str,
    mapping: &Value,
) -> Result<(), AppError> {
    let staged = sqlx::query(
        "SELECT r.id, r.row_number, r.fields FROM import_rows r
         JOIN import_batches b ON b.id = r.batch_id AND b.revision = r.revision
         WHERE b.workspace_id = $1 AND b.id = $2 ORDER BY r.row_number FOR UPDATE OF r",
    )
    .bind(ws)
    .bind(batch)
    .fetch_all(&mut *conn)
    .await?;
    let fields: Vec<Value> = staged.iter().map(|r| r.get(2)).collect();
    let (issues, lines) = check_rows(&mut *conn, ws, profile, mapping, &fields).await?;
    if let Some(issue) = issues.into_iter().flatten().next() {
        return Err(AppError::Unprocessable(format!(
            "The opening file no longer validates: {}",
            issue["message"].as_str().unwrap_or_default()
        )));
    }
    for (row, line) in staged.iter().zip(lines.into_iter().flatten()) {
        let [face, settled, credited, refunded, outstanding, carrying] = line.amounts;
        let money = matches!(line.kind, "receivable" | "payable");
        sqlx::query(
            "INSERT INTO accounting_opening_lines (workspace_id, batch_id, row_number, kind, mode,
                 warehouse_id, product_id, quantity, value, invoice_id, supplier_bill_id,
                 currency_id, face, settled, credited, refunded, outstanding, carrying,
                 account_id, debit, credit, register_id, vault_id, bank_account_id, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                     $18, $19, $20, $21, $22, $23, $24, $25)",
        )
        .bind(ws)
        .bind(batch)
        .bind(row.get::<i32, _>(1))
        .bind(line.kind)
        .bind(&line.mode)
        .bind(line.warehouse)
        .bind(line.product)
        .bind(line.quantity)
        .bind(line.value)
        .bind(line.invoice)
        .bind(line.bill)
        .bind(line.currency)
        .bind(money.then_some(face))
        .bind(money.then_some(settled))
        .bind(money.then_some(credited))
        .bind(money.then_some(refunded))
        .bind(money.then_some(outstanding))
        .bind(money.then_some(carrying))
        .bind(line.account)
        .bind(line.debit)
        .bind(line.credit)
        .bind(line.register)
        .bind(line.vault)
        .bind(line.bank)
        .bind(actor)
        .execute(&mut *conn)
        .await?;
    }
    let ids: Vec<Uuid> = staged.iter().map(|r| r.get(0)).collect();
    sqlx::query("UPDATE import_rows SET status = 'applied' WHERE id = ANY($1)")
        .bind(&ids)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

// --- review ----------------------------------------------------------------------------

/// POST /accounting/opening/validate body: the staged opening batches to review.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReviewBody {
    /// Completed opening import batches, at most one per profile.
    pub batch_ids: Option<Vec<String>>,
    /// The book setup version the caller loaded.
    pub expected_setup_version: Option<i64>,
}

/// A validated review request.
pub struct Review {
    batches: Vec<Uuid>,
    version: i64,
}

impl Validate for ReviewBody {
    type Valid = Review;

    fn validate(self, r: &mut Rules) -> Option<Review> {
        let ids = self.batch_ids.unwrap_or_default();
        let batches: Vec<Uuid> = ids.iter().filter_map(|s| s.parse().ok()).collect();
        r.check(
            "batch_ids",
            !ids.is_empty() && ids.len() <= PROFILES.len() && batches.len() == ids.len(),
            "Name one to four opening import batches.",
        );
        if self.expected_setup_version.is_none() {
            r.missing("expected_setup_version");
        }
        Some(Review {
            batches,
            version: self.expected_setup_version?,
        })
    }
}

/// Schedules, issues and fingerprint of `batches` as of now (no writes). `None` batches field
/// error when a batch is unknown, unfinished, not an opening batch, or a profile repeats.
pub async fn review_in(
    conn: &mut PgConnection,
    ws: Uuid,
    batches: &[Uuid],
) -> Result<Value, AppError> {
    let s = repo::settings(&mut *conn, ws, false).await?;
    let fdp = header(&mut *conn, ws).await?;
    let found: Vec<(String, String)> = sqlx::query_as(
        "SELECT profile, id::text || ':' || revision || ':' || coalesce(validation_fingerprint, '')
         FROM import_batches
         WHERE workspace_id = $1 AND id = ANY($2) AND state = 'completed'
           AND profile LIKE 'opening\\_%' ORDER BY id",
    )
    .bind(ws)
    .bind(batches)
    .fetch_all(&mut *conn)
    .await?;
    let mut profiles: Vec<&str> = found.iter().map(|(p, _)| p.as_str()).collect();
    profiles.sort_unstable();
    profiles.dedup();
    if found.len() != batches.len() || profiles.len() != found.len() {
        return Err(AppError::field(
            "batch_ids",
            "Name completed opening imports of this workspace, at most one per profile.",
        ));
    }
    let has = |p: &str| profiles.contains(&p);
    // Every figure and its text in SQL: functional decimals for value, carrying and trial
    // balance; face/settled/outstanding are nominal sums (one currency per schedule in practice).
    let r = sqlx::query(
        "WITH l AS (SELECT * FROM accounting_opening_lines WHERE workspace_id = $1 AND batch_id = ANY($2)),
         roles AS (SELECT role, account_id FROM account_mappings WHERE workspace_id = $1),
         tb AS (SELECT l.account_id, sum(l.debit - l.credit) AS net FROM l WHERE kind = 'balance'
                GROUP BY l.account_id),
         net AS (SELECT r.role, coalesce(sum(tb.net), 0) AS tb FROM roles r
                 LEFT JOIN tb ON tb.account_id = r.account_id GROUP BY r.role),
         stock AS (SELECT product_id, warehouse_id, sum(stock_lvl) AS q FROM stocks
                   WHERE workspace_id = $1 AND deleted_at IS NULL
                   GROUP BY product_id, warehouse_id HAVING sum(stock_lvl) <> 0),
         staged AS (SELECT product_id, warehouse_id, quantity AS q FROM l
                    WHERE kind = 'inventory' AND quantity > 0),
         paid AS (SELECT l.id, l.settled - l.refunded AS reviewed,
                         coalesce((SELECT sum(p.amount) FROM invoice_payments p
                                   WHERE p.workspace_id = $1 AND p.invoice_id = l.invoice_id
                                     AND p.is_paid AND p.voided_at IS NULL), 0) AS ledger
                  FROM l WHERE kind = 'receivable'),
         n AS (SELECT
           (SELECT min(mode) FROM l WHERE kind = 'inventory') AS mode,
           (SELECT coalesce(sum(quantity), 0)::numeric(19,2)::text FROM l WHERE kind = 'inventory') AS quantity,
           (SELECT coalesce(sum(value), 0)::bigint FROM l WHERE kind = 'inventory') AS value,
           (SELECT count(*) FROM l WHERE kind = 'receivable') AS ar_count,
           (SELECT coalesce(sum(face), 0)::bigint FROM l WHERE kind = 'receivable') AS ar_face,
           (SELECT coalesce(sum(settled), 0)::bigint FROM l WHERE kind = 'receivable') AS ar_settled,
           (SELECT coalesce(sum(outstanding), 0)::bigint FROM l WHERE kind = 'receivable') AS ar_out,
           (SELECT coalesce(sum(carrying), 0)::bigint FROM l WHERE kind = 'receivable') AS ar_carrying,
           (SELECT count(*) FROM l WHERE kind = 'payable') AS ap_count,
           (SELECT coalesce(sum(face), 0)::bigint FROM l WHERE kind = 'payable') AS ap_face,
           (SELECT coalesce(sum(settled), 0)::bigint FROM l WHERE kind = 'payable') AS ap_settled,
           (SELECT coalesce(sum(outstanding), 0)::bigint FROM l WHERE kind = 'payable') AS ap_out,
           (SELECT coalesce(sum(carrying), 0)::bigint FROM l WHERE kind = 'payable') AS ap_carrying,
           (SELECT coalesce(sum(debit), 0)::bigint FROM l WHERE kind = 'balance') AS debit,
           (SELECT coalesce(sum(credit), 0)::bigint FROM l WHERE kind = 'balance') AS credit,
           -- control accounts: trial balance net = detail schedule
           (SELECT count(*) FROM net WHERE
              (role = 'receivables' AND tb <> (SELECT coalesce(sum(carrying), 0) FROM l WHERE kind = 'receivable'))
              OR (role = 'payables' AND -tb <> (SELECT coalesce(sum(carrying), 0) FROM l WHERE kind = 'payable'))
              OR (role = 'inventory' AND tb <> (SELECT coalesce(sum(value), 0) FROM l WHERE kind = 'inventory'))) AS control,
           -- cash and bank lines name their place
           (SELECT count(*) FROM l WHERE kind = 'balance' AND register_id IS NULL AND vault_id IS NULL
              AND bank_account_id IS NULL
              AND (account_id IN (SELECT account_id FROM roles WHERE role = 'cash')
                   OR account_id IN (SELECT account_id FROM bank_accounts WHERE workspace_id = $1))) AS unplaced,
           (SELECT count(*) FROM (SELECT * FROM stock EXCEPT SELECT * FROM staged) a) +
           (SELECT count(*) FROM (SELECT * FROM staged EXCEPT SELECT * FROM stock) b) AS quantity_diff,
           (SELECT count(*) FROM stock) AS stocked,
           (SELECT count(*) FROM paid WHERE reviewed <> ledger) AS unpaid,
           (SELECT count(*) FROM drawer_shifts WHERE workspace_id = $1 AND closed_at IS NULL) AS shifts,
           -- coverage: the historical payments the receivables cover, as of now
           (SELECT coalesce(string_agg(p.id::text || '=' || p.amount, ',' ORDER BY p.id), '')
              FROM invoice_payments p JOIN l ON l.kind = 'receivable' AND l.invoice_id = p.invoice_id
              WHERE p.workspace_id = $1 AND p.is_paid AND p.voided_at IS NULL) AS coverage,
           (SELECT coalesce(string_agg(x.place || '=' || x.amount, ',' ORDER BY x.place), '') FROM (
              SELECT coalesce(register_id, vault_id, bank_account_id)::text AS place,
                     sum(debit - credit) AS amount
              FROM l WHERE kind = 'balance' AND num_nonnulls(register_id, vault_id, bank_account_id) = 1
              GROUP BY 1) x) AS places)
         SELECT n.*, f.*
         FROM n, LATERAL (SELECT
             round(n.value / 10::numeric ^ $3::int, $3::int)::text AS value_t,
             round(n.ar_face / 10::numeric ^ $3::int, $3::int)::text AS ar_face_t,
             round(n.ar_settled / 10::numeric ^ $3::int, $3::int)::text AS ar_settled_t,
             round(n.ar_out / 10::numeric ^ $3::int, $3::int)::text AS ar_out_t,
             round(n.ar_carrying / 10::numeric ^ $3::int, $3::int)::text AS ar_carrying_t,
             round(n.ap_face / 10::numeric ^ $3::int, $3::int)::text AS ap_face_t,
             round(n.ap_settled / 10::numeric ^ $3::int, $3::int)::text AS ap_settled_t,
             round(n.ap_out / 10::numeric ^ $3::int, $3::int)::text AS ap_out_t,
             round(n.ap_carrying / 10::numeric ^ $3::int, $3::int)::text AS ap_carrying_t,
             round(n.debit / 10::numeric ^ $3::int, $3::int)::text AS debit_t,
             round(n.credit / 10::numeric ^ $3::int, $3::int)::text AS credit_t) f",
    )
    .bind(ws)
    .bind(batches)
    .bind(i32::from(fdp))
    .fetch_one(&mut *conn)
    .await?;
    let n = |k: &str| r.get::<i64, _>(k);
    let t = |k: &str| r.get::<String, _>(format!("{k}_t").as_str());
    let mode: Option<String> = r.get("mode");
    let mut issues = vec![];
    let mut issue = |on: bool, code: &str, message: &str| {
        if on {
            issues.push(json!({"code": code, "message": message}));
        }
    };
    issue(
        !has("opening_trial_balance"),
        "trial_balance_required",
        "Stage an opening trial balance: its control accounts must match the detail schedules.",
    );
    issue(
        has("opening_trial_balance") && n("debit") != n("credit"),
        "trial_balance_unbalanced",
        "The opening trial balance debits and credits must be equal.",
    );
    issue(
        has("opening_trial_balance") && n("control") > 0,
        "control_difference",
        "A receivables, payables or inventory control account differs from its detail schedule.",
    );
    issue(
        n("unplaced") > 0,
        "place_required",
        "Opening cash and bank lines must name their drawer, vault or bank account.",
    );
    issue(
        n("unpaid") > 0,
        "opening_payment_mismatch",
        "A receivable's reviewed paid amount differs from the payments recorded on its invoice.",
    );
    issue(
        mode.as_deref() == Some("existing") && n("quantity_diff") > 0,
        "stock_quantity_mismatch",
        "Existing workspace: opening quantities must equal the stock on hand, product by product.",
    );
    issue(
        mode.as_deref() == Some("empty") && n("stocked") > 0,
        "stock_not_empty",
        "Empty workspace mode needs a workspace without stock on hand.",
    );
    issue(
        n("shifts") > 0,
        "open_shifts",
        "Close every open till shift before the opening.",
    );
    let schedules = json!({
        "inventory": {"mode": mode, "quantity": r.get::<String, _>("quantity"),
                      "value": t("value")},
        "receivables": {"count": n("ar_count"), "face": t("ar_face"),
                        "settled": t("ar_settled"), "outstanding": t("ar_out"),
                        "carrying": t("ar_carrying")},
        "payables": {"count": n("ap_count"), "face": t("ap_face"),
                     "settled": t("ap_settled"), "outstanding": t("ap_out"),
                     "carrying": t("ap_carrying")},
        "trial_balance": {"debit": t("debit"), "credit": t("credit")},
    });
    let batches: Vec<&String> = found.iter().map(|(_, b)| b).collect();
    let fingerprint = hex::encode(Sha256::digest(
        json!({"setup_version": s.version, "batches": batches, "schedules": schedules,
               "coverage": r.get::<String, _>("coverage"), "places": r.get::<String, _>("places"),
               "issues": issues})
        .to_string(),
    ));
    let mut out = schedules;
    out["setup_version"] = json!(s.version);
    out["ready"] = json!(issues.is_empty());
    out["issues"] = json!(issues);
    out["opening_fingerprint"] = json!(fingerprint);
    Ok(out)
}

/// POST /accounting/opening/validate: review only, nothing is written. 409 stale setup.
pub async fn review(state: &AppState, ws: Uuid, input: Review) -> Result<Value, AppError> {
    let mut conn = state.db.acquire().await?;
    let s = repo::settings(&mut conn, ws, false).await?;
    if s.version != input.version {
        return Err(stale());
    }
    review_in(&mut conn, ws, &input.batches).await
}

// --- activation core -------------------------------------------------------------------

/// Reason code of an ordinary void/reverse/callback against a historical opening-covered source.
pub const COVERED: &str = "opening_covered_source";

/// 422 `opening_covered_source`.
pub fn covered_error() -> AppError {
    AppError::Reason {
        status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        code: COVERED,
        message: "This historical payment is part of the reviewed opening balance. Use an opening correction instead.".into(),
    }
}

/// Whether source `kind`/`id` is already covered by the opening.
pub async fn covered(
    conn: &mut PgConnection,
    ws: Uuid,
    kind: &str,
    id: Uuid,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM accounting_source_links WHERE workspace_id = $1
                          AND source_kind = $2 AND source_id = $3 AND disposition = 'opening_covered')",
    )
    .bind(ws)
    .bind(kind)
    .bind(id)
    .fetch_one(conn)
    .await?)
}

/// Fingerprint of a covered provider fact: exact economic kind, amount and currency.
pub fn provider_fact(kind: &str, amount: i64, currency: &str) -> [u8; 32] {
    Sha256::digest(format!("{kind}|{amount}|{currency}")).into()
}

/// Whether provider operation `op` is covered with exactly this kind/amount/currency.
pub async fn covered_exactly(
    conn: &mut PgConnection,
    ws: Uuid,
    op: Uuid,
    fact: &[u8; 32],
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM accounting_source_links WHERE workspace_id = $1
                          AND source_kind = 'order_payment' AND source_id = $2
                          AND disposition = 'opening_covered' AND fingerprint = $3)",
    )
    .bind(ws)
    .bind(op)
    .bind(fact.as_slice())
    .fetch_one(conn)
    .await?)
}

/// Activation core (masterplan §14 ACTIVATION steps 7-9), crate-internal: M8 wraps it with the
/// route, the setup→active flip and the audit. Under the exclusive book gate: re-review
/// `batches`, refuse a changed `fingerprint` (409) or an unready opening (422), then once per
/// book post the opening journal, open baselines and items, mark historical payments
/// `opening_covered`, seed or create opening stock and record place openings. `false` when
/// the opening already ran (retry = no-op). Book state is left as is.
pub async fn apply(
    conn: &mut PgConnection,
    ws: Uuid,
    batches: &[Uuid],
    fingerprint: &str,
    by: Uuid,
) -> Result<bool, AppError> {
    use super::{gate, journals, settlement};
    use crate::modules::stock::repo as stock;
    gate::exclusive(&mut *conn, ws).await?;
    let done: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM accounting_effects WHERE workspace_id = $1
                          AND source_kind = 'opening' AND source_id = $1 AND effect_kind = 'activated')",
    )
    .bind(ws)
    .fetch_one(&mut *conn)
    .await?;
    if done {
        return Ok(false);
    }
    let review = review_in(&mut *conn, ws, batches).await?;
    if review["opening_fingerprint"].as_str() != Some(fingerprint) {
        return Err(stale());
    }
    if review["ready"] != true {
        let codes: Vec<&str> = review["issues"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| i["code"].as_str())
            .collect();
        return Err(AppError::Unprocessable(format!(
            "The opening is not ready: {}.",
            codes.join(", ")
        )));
    }
    let date: chrono::NaiveDate = sqlx::query_scalar(
        "SELECT (cutover_at AT TIME ZONE timezone)::date FROM accounting_settings
         WHERE workspace_id = $1 AND cutover_at IS NOT NULL AND timezone IS NOT NULL",
    )
    .bind(ws)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(not_configured)?;
    let fp: [u8; 32] = Sha256::digest(fingerprint).into();
    let effect = repo::insert_effect(&mut *conn, ws, "opening", ws, "activated", &fp, by).await?;

    // Step 8: the reviewed trial balance, once (its controls equal the detail schedules).
    let tb: Vec<(Uuid, i64, i64, Option<Uuid>)> = sqlx::query_as(
        "SELECT account_id, debit, credit, bank_account_id FROM accounting_opening_lines
         WHERE workspace_id = $1 AND batch_id = ANY($2) AND kind = 'balance'
         ORDER BY batch_id, row_number",
    )
    .bind(ws)
    .bind(batches)
    .fetch_all(&mut *conn)
    .await?;
    let lines: Vec<journals::SourceLine> = tb
        .iter()
        .map(|(account, debit, credit, bank)| journals::SourceLine {
            side: settlement::Side::Account(*account),
            debit: *debit,
            credit: *credit,
            relation_id: None,
            bank_account_id: *bank,
        })
        .collect();
    let entry = journals::post_lines(
        &mut *conn,
        ws,
        "opening",
        (date, "Opening balances", by),
        &lines,
        effect,
    )
    .await?;

    // Step 9: baselines, opening items and the historical sources they cover.
    let sources = sqlx::query(
        "SELECT l.id, l.kind, coalesce(l.invoice_id, l.supplier_bill_id) AS source, l.currency_id,
                l.face, l.settled, l.credited, l.refunded, l.outstanding, l.carrying,
                coalesce(i.relation_id, b.supplier_id) AS relation,
                coalesce((i.due_date AT TIME ZONE s.timezone)::date, b.due_date) AS due
         FROM accounting_opening_lines l
         JOIN accounting_settings s ON s.workspace_id = l.workspace_id
         LEFT JOIN invoices i ON i.workspace_id = l.workspace_id AND i.id = l.invoice_id
         LEFT JOIN supplier_bills b ON b.workspace_id = l.workspace_id AND b.id = l.supplier_bill_id
         WHERE l.workspace_id = $1 AND l.batch_id = ANY($2) AND l.kind IN ('receivable', 'payable')
         ORDER BY l.batch_id, l.row_number",
    )
    .bind(ws)
    .bind(batches)
    .fetch_all(&mut *conn)
    .await?;
    for r in sources {
        let (line, kind, source, currency): (Uuid, String, Uuid, Uuid) = (
            r.get("id"),
            r.get("kind"),
            r.get("source"),
            r.get("currency_id"),
        );
        let a: [i64; 6] = AMOUNTS.map(|k| r.get(k));
        let (relation, due): (Option<Uuid>, Option<chrono::NaiveDate>) =
            (r.get("relation"), r.get("due"));
        let receivable = kind == "receivable";
        let (role, source_kind) = if receivable {
            ("receivables", "invoice")
        } else {
            ("payables", "supplier_bill")
        };
        let account_id = settlement::role_account(&mut *conn, ws, role).await?;
        let item = if a[4] > 0 {
            Some(
                settlement::open_item(
                    &mut *conn,
                    ws,
                    &settlement::NewOpenItem {
                        classification: if receivable { "receivable" } else { "payable" },
                        side: if receivable { "debit" } else { "credit" },
                        account_role: role,
                        account_id,
                        source_kind,
                        source_id: source,
                        relation_id: relation,
                        order_id: None,
                        invoice_id: receivable.then_some(source),
                        recognition_id: None,
                        currency_id: currency,
                        issue_date: date,
                        due_date: due.filter(|d| *d >= date),
                        origin: "opening",
                        amount: a[4],
                        functional_amount: a[5],
                        effect_id: effect,
                        by,
                    },
                )
                .await?,
            )
        } else {
            None
        };
        // Historical ledger payments and provider operations of the source stay as they are;
        // a sidecar link marks each covered (no journal, no cash).
        let ledger: Vec<(Uuid, i64)> = sqlx::query_as(
            "SELECT id, amount FROM invoice_payments WHERE workspace_id = $1 AND invoice_id = $2
               AND is_paid AND voided_at IS NULL ORDER BY id",
        )
        .bind(ws)
        .bind(source)
        .fetch_all(&mut *conn)
        .await?;
        let provider: Vec<(Uuid, String, i64, String)> = sqlx::query_as(
            "SELECT id, CASE kind WHEN 'payment' THEN 'collection' ELSE 'refund' END, amount, currency
             FROM order_payments WHERE workspace_id = $1 AND invoice_id = $2
               AND ((kind = 'payment' AND status = 'paid') OR (kind = 'refund' AND status = 'refunded'))
             ORDER BY id",
        )
        .bind(ws)
        .bind(source)
        .fetch_all(&mut *conn)
        .await?;
        let mut coverage = Sha256::new();
        for (id, amount) in ledger.iter().filter(|_| receivable) {
            coverage.update(format!("{id}={amount};"));
            let fp: [u8; 32] = Sha256::digest(format!("{id}={amount}")).into();
            settlement::link_source(
                &mut *conn,
                ws,
                "invoice_payment",
                *id,
                None,
                "opening_covered",
                &fp,
            )
            .await?;
        }
        for (id, fact, amount, currency) in provider.iter().filter(|_| receivable) {
            coverage.update(format!("{id}:{fact}={amount};"));
            let fp = provider_fact(fact, *amount, currency);
            settlement::link_source(
                &mut *conn,
                ws,
                "order_payment",
                *id,
                None,
                "opening_covered",
                &fp,
            )
            .await?;
        }
        let coverage: [u8; 32] = coverage.finalize().into();
        sqlx::query(
            "INSERT INTO accounting_opening_baselines (workspace_id, line_id, source_kind, source_id,
                 currency_id, face, settled, credited, refunded, outstanding, carrying,
                 open_item_id, effect_id, coverage)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(ws)
        .bind(line)
        .bind(source_kind)
        .bind(source)
        .bind(currency)
        .bind(a[0])
        .bind(a[1])
        .bind(a[2])
        .bind(a[3])
        .bind(a[4])
        .bind(a[5])
        .bind(item)
        .bind(effect)
        .bind(coverage.as_slice())
        .execute(&mut *conn)
        .await?;
    }

    // Step 7: existing workspace seeds value on the quantity already on hand; an empty
    // workspace gets its opening stock now, once.
    let stock_lines: Vec<(Uuid, Uuid, Decimal, i64, String)> = sqlx::query_as(
        "SELECT product_id, warehouse_id, quantity, value, mode FROM accounting_opening_lines
         WHERE workspace_id = $1 AND batch_id = ANY($2) AND kind = 'inventory' AND quantity > 0
         ORDER BY batch_id, row_number",
    )
    .bind(ws)
    .bind(batches)
    .fetch_all(&mut *conn)
    .await?;
    for (product, warehouse, quantity, value, mode) in stock_lines {
        let movement: Option<Uuid> = if mode == "empty" {
            let cause = stock::Cause {
                reason: Some("Opening stock"),
                is_correction: false,
                user_id: Some(by),
                related: Some(("opening", effect)),
                meta: None,
                value: Default::default(),
            };
            let slot = stock::Slot {
                product_id: product,
                warehouse_id: warehouse,
                bin: None,
                lot_id: None,
            };
            let stock::Mutated::Done { stock_id, .. } =
                stock::mutate(&mut *conn, ws, slot, quantity, stock::Floor::None, &cause).await?
            else {
                return Err(AppError::Unprocessable(
                    "The opening stock is out of range.".into(),
                ));
            };
            sqlx::query_scalar(
                "SELECT id FROM stock_movements WHERE workspace_id = $1 AND stock_id = $2
                   AND related_domain = 'opening' AND related_id = $3",
            )
            .bind(ws)
            .bind(stock_id)
            .bind(effect)
            .fetch_one(&mut *conn)
            .await?
        } else {
            None
        };
        sqlx::query(
            "WITH p AS (INSERT INTO inventory_cost_pools (workspace_id, product_id, warehouse_id,
                            quantity, value, last_accounting_date, valuation_sequence)
                        VALUES ($1, $2, $3, $4, $5, $6, 1) RETURNING id)
             INSERT INTO inventory_value_entries (workspace_id, pool_id, sequence, movement_id,
                 accounting_date, classification, source_kind, source_id, quantity_before,
                 quantity_delta, quantity_after, value_before, value_delta, value_after,
                 journal_entry_id)
             SELECT $1, p.id, 1, $7, $6, 'opening', 'opening', $8, 0, $4, $4, 0, $5, $5, $9 FROM p",
        )
        .bind(ws)
        .bind(product)
        .bind(warehouse)
        .bind(quantity)
        .bind(value)
        .bind(date)
        .bind(movement)
        .bind(effect)
        .bind(entry)
        .execute(&mut *conn)
        .await?;
    }

    // Reviewed opening cash per drawer/vault; historical place rows stay before the cutover.
    sqlx::query(
        "INSERT INTO cash_place_openings (workspace_id, register_id, vault_id, amount, effect_id)
         SELECT $1, register_id, vault_id, sum(debit - credit), $3 FROM accounting_opening_lines
         WHERE workspace_id = $1 AND batch_id = ANY($2) AND kind = 'balance'
           AND (register_id IS NOT NULL OR vault_id IS NOT NULL)
         GROUP BY register_id, vault_id",
    )
    .bind(ws)
    .bind(batches)
    .bind(effect)
    .execute(&mut *conn)
    .await?;
    Ok(true)
}

// --- opening correction ----------------------------------------------------------------

/// Opening correction body (review: plain body; create: inside the C envelope).
#[derive(Debug, Deserialize, ToSchema)]
pub struct CorrectionPayload {
    #[schema(value_type = Uuid)]
    pub baseline_id: Option<String>,
    /// Current open period date (`YYYY-MM-DD`).
    pub accounting_date: Option<String>,
    /// Corrected opening outstanding, source currency (decimal string).
    #[schema(value_type = String)]
    pub corrected_nominal_basis: Option<Value>,
    /// Corrected functional carrying (decimal string).
    #[schema(value_type = String)]
    pub corrected_functional_carrying: Option<Value>,
    pub reason: Option<String>,
    pub evidence_ref: Option<String>,
    /// From the review; required on create.
    pub review_fingerprint: Option<String>,
}

/// A validated correction request.
#[derive(Clone)]
pub struct Correction {
    baseline: Uuid,
    date: chrono::NaiveDate,
    nominal: Decimal,
    carrying: Decimal,
    reason: String,
    evidence: String,
    fingerprint: Option<String>,
}

impl Validate for CorrectionPayload {
    type Valid = Correction;

    fn validate(self, r: &mut Rules) -> Option<Correction> {
        let baseline = super::dto::uuid(r, "baseline_id", self.baseline_id);
        let date = super::dto::date(r, "accounting_date", self.accounting_date);
        let nominal = match &self.corrected_nominal_basis {
            Some(v) => r.decimal(
                "corrected_nominal_basis",
                v,
                Decimal::ZERO,
                crate::money::MAX,
            ),
            None => {
                r.missing("corrected_nominal_basis");
                None
            }
        };
        let carrying = match &self.corrected_functional_carrying {
            Some(v) => r.decimal(
                "corrected_functional_carrying",
                v,
                Decimal::ZERO,
                crate::money::MAX,
            ),
            None => {
                r.missing("corrected_functional_carrying");
                None
            }
        };
        let reason = r.text("reason", self.reason, 1000);
        let evidence = r.text("evidence_ref", self.evidence_ref, 255);
        Some(Correction {
            baseline: baseline?,
            date: date?,
            nominal: nominal?,
            carrying: carrying?,
            reason,
            evidence,
            fingerprint: self.review_fingerprint,
        })
    }
}

/// Facts of a correction under the caller's locks.
struct Plan {
    receivable: bool,
    source: Uuid,
    currency: Uuid,
    relation: Option<Uuid>,
    delta: i64,
    review: Value,
}

/// Review a correction (no writes): active book, functional-currency baseline, corrected
/// outstanding within 0 ..= face − credited + refunded − confirmed provider cash, a change.
async fn plan(conn: &mut PgConnection, ws: Uuid, c: &Correction) -> Result<Plan, AppError> {
    if !value::active(&mut *conn, ws).await? {
        return Err(not_configured());
    }
    let found = sqlx::query(
        "SELECT b.source_kind, b.source_id, b.currency_id, c.decimals,
                b.currency_id = s.functional_currency_id AS functional,
                coalesce(i.relation_id, sb.supplier_id) AS relation,
                (b.outstanding + coalesce((SELECT sum(x.outstanding_delta) FROM accounting_opening_corrections x
                                          WHERE x.workspace_id = b.workspace_id AND x.baseline_id = b.id), 0)
                    )::bigint AS current,
                (b.face - b.credited + b.refunded
                  - coalesce((SELECT sum(CASE p.kind WHEN 'payment' THEN p.amount ELSE -p.amount END)
                              FROM order_payments p
                              WHERE p.workspace_id = b.workspace_id AND b.source_kind = 'invoice'
                                AND p.invoice_id = b.source_id
                                AND ((p.kind = 'payment' AND p.status = 'paid')
                                     OR (p.kind = 'refund' AND p.status = 'refunded'))), 0))::bigint
                    AS ceiling,
                (SELECT count(*) FROM accounting_opening_corrections x
                 WHERE x.workspace_id = b.workspace_id AND x.baseline_id = b.id) AS corrections
         FROM accounting_opening_baselines b
         JOIN accounting_settings s ON s.workspace_id = b.workspace_id
         JOIN currencies c ON c.workspace_id = b.workspace_id AND c.id = b.currency_id
         LEFT JOIN invoices i ON i.workspace_id = b.workspace_id AND i.id = b.source_id
         LEFT JOIN supplier_bills sb ON sb.workspace_id = b.workspace_id AND sb.id = b.source_id
         WHERE b.workspace_id = $1 AND b.id = $2",
    )
    .bind(ws)
    .bind(c.baseline)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    if !found.get::<bool, _>("functional") {
        // shortcut: foreign-currency baselines need carrying-only corrections, add with them.
        return Err(unsupported(
            "Opening corrections in a foreign currency are not supported yet.",
        ));
    }
    let dp = u32::try_from(found.get::<i16, _>("decimals")).unwrap_or(0);
    let field = |k: &'static str| {
        move |e: AppError| match e {
            AppError::Validation(m) => {
                AppError::field(k, m.into_values().flatten().next().unwrap_or_default())
            }
            e => e,
        }
    };
    let nominal = crate::money::minor(c.nominal, dp).map_err(field("corrected_nominal_basis"))?;
    let carrying =
        crate::money::minor(c.carrying, dp).map_err(field("corrected_functional_carrying"))?;
    if carrying != nominal {
        return Err(AppError::field(
            "corrected_functional_carrying",
            "In the functional currency the carrying equals the outstanding amount.",
        ));
    }
    let (current, ceiling): (i64, i64) = (found.get("current"), found.get("ceiling"));
    let figures: (i64, String, String, String) = sqlx::query_as(
        "SELECT $2::bigint - $1, round($1 / 10::numeric ^ $3::int, $3::int)::text,
                round($2 / 10::numeric ^ $3::int, $3::int)::text,
                round(($2::bigint - $1) / 10::numeric ^ $3::int, $3::int)::text",
    )
    .bind(current)
    .bind(nominal)
    .bind(i32::try_from(dp).unwrap_or(0))
    .fetch_one(&mut *conn)
    .await?;
    let delta = figures.0;
    if delta == 0 {
        return Err(AppError::field(
            "corrected_nominal_basis",
            "This is the current opening amount; there is nothing to correct.",
        ));
    }
    if nominal > ceiling {
        return Err(AppError::field(
            "corrected_nominal_basis",
            "This would undo confirmed customer or provider money. Record a refund or correction of that money instead.",
        ));
    }
    let receivable = found.get::<String, _>("source_kind") == "invoice";
    let control = if receivable {
        "receivables"
    } else {
        "payables"
    };
    // Receivable up / payable down debits the control; the other side is opening equity.
    let control_debit = (delta > 0) == receivable;
    let amount = &figures.3.trim_start_matches('-').to_owned();
    let journal = if control_debit {
        json!([{"role": control, "debit": amount}, {"role": "opening_equity", "credit": amount}])
    } else {
        json!([{"role": "opening_equity", "debit": amount}, {"role": control, "credit": amount}])
    };
    let mut review = json!({
        "baseline_id": c.baseline, "accounting_date": c.date,
        "current_outstanding": figures.1, "corrected_outstanding": figures.2,
        "delta": figures.3, "journal": journal,
    });
    let fp = hex::encode(Sha256::digest(
        json!({"review": review, "corrections": found.get::<i64, _>("corrections"),
               "reason": c.reason, "evidence": c.evidence})
        .to_string(),
    ));
    review["review_fingerprint"] = json!(fp);
    Ok(Plan {
        receivable,
        source: found.get("source_id"),
        currency: found.get("currency_id"),
        relation: found.get("relation"),
        delta,
        review,
    })
}

/// POST /accounting/opening-corrections/review.
pub async fn correction_review(
    state: &AppState,
    ws: Uuid,
    c: Correction,
) -> Result<Value, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(plan(&mut conn, ws, &c).await?.review)
}

/// POST /accounting/opening-corrections: append the reviewed correction once (current open
/// period): effect + journal vs opening equity, an opening item for an increase or a credit
/// item applied to the opening item for a decrease, the correction row. The baseline stays.
pub async fn correction_create(
    state: &AppState,
    access: &crate::http::workspace::WorkspaceAccess,
    cmd: crate::idempotency::Command<super::dto::Create<CorrectionPayload>>,
) -> Result<crate::idempotency::Answer, AppError> {
    use super::{gate, journals, settlement};
    use crate::idempotency;
    const OP: &str = "accounting_opening_corrections_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    gate::shared(&mut tx, &[ws]).await?;
    let c = &cmd.input.payload;
    let Some(given) = c.fingerprint.as_deref() else {
        return Err(AppError::field(
            "review_fingerprint",
            "Review the correction first.",
        ));
    };
    let source: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT source_kind, source_id FROM accounting_opening_baselines WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(c.baseline)
    .fetch_optional(&mut *tx)
    .await?;
    // Lock order: invoice (as payments do), then the plan's reads, then open items.
    if let Some(("invoice", invoice)) = source.as_ref().map(|(k, i)| (k.as_str(), *i)) {
        crate::modules::invoice_payments::service::refresh(&mut tx, ws, invoice).await?;
    }
    let p = plan(&mut tx, ws, c).await?;
    if p.review["review_fingerprint"].as_str() != Some(given) {
        return Err(stale());
    }
    let id = Uuid::now_v7();
    let fp: [u8; 32] = Sha256::digest(given).into();
    let control = if p.receivable {
        "receivables"
    } else {
        "payables"
    };
    let amount = p.delta.abs();
    let control_debit = (p.delta > 0) == p.receivable;
    let line = |side, debit: bool, relation| journals::SourceLine {
        side,
        debit: if debit { amount } else { 0 },
        credit: if debit { 0 } else { amount },
        relation_id: relation,
        bank_account_id: None,
    };
    let lines = [
        line(settlement::Side::Role(control), control_debit, p.relation),
        line(
            settlement::Side::Role("opening_equity"),
            !control_debit,
            None,
        ),
    ];
    let (_, effect) = journals::post_source_lines(
        &mut tx,
        ws,
        &journals::SourceHeader {
            source_kind: "opening_correction",
            source_id: id,
            effect_kind: "posted",
            date: c.date,
            reason: &c.reason,
            fingerprint: &fp,
            by: access.user_id,
        },
        &lines,
    )
    .await?;
    let account_id = settlement::role_account(&mut tx, ws, control).await?;
    let item = |classification, side| settlement::NewOpenItem {
        classification,
        side,
        account_role: control,
        account_id,
        source_kind: "opening_correction",
        source_id: id,
        relation_id: p.relation,
        order_id: None,
        invoice_id: p.receivable.then_some(p.source),
        recognition_id: None,
        currency_id: p.currency,
        issue_date: c.date,
        due_date: None,
        origin: "opening",
        amount,
        functional_amount: amount,
        effect_id: effect,
        by: access.user_id,
    };
    let (open, credit) = if p.receivable {
        (("receivable", "debit"), ("customer_credit", "credit"))
    } else {
        (("payable", "credit"), ("supplier_credit", "debit"))
    };
    if p.delta > 0 {
        settlement::open_item(&mut tx, ws, &item(open.0, open.1)).await?;
    } else {
        // Relieve an opening item of this source that still holds the amount.
        let target: Uuid = sqlx::query_scalar(
            "SELECT i.id FROM accounting_open_items i
             WHERE i.workspace_id = $1 AND i.classification = $3 AND i.residual >= $4
               AND (i.id = (SELECT open_item_id FROM accounting_opening_baselines
                            WHERE workspace_id = $1 AND id = $2)
                    OR (i.source_kind = 'opening_correction' AND i.source_id IN
                        (SELECT x.id FROM accounting_opening_corrections x
                         WHERE x.workspace_id = $1 AND x.baseline_id = $2)))
             ORDER BY i.id LIMIT 1",
        )
        .bind(ws)
        .bind(c.baseline)
        .bind(open.0)
        .bind(amount)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            AppError::field(
                "corrected_nominal_basis",
                "Less than this is still open on the opening balance; it was settled after the cutover.",
            )
        })?;
        let src = settlement::open_item(&mut tx, ws, &item(credit.0, credit.1)).await?;
        settlement::allocate(
            &mut tx,
            ws,
            &settlement::NewAllocation {
                kind: "application",
                source_item_id: Some(src),
                target_item_id: target,
                date: c.date,
                currency_id: p.currency,
                amount,
                effect_id: effect,
                by: access.user_id,
            },
        )
        .await?;
    }
    sqlx::query(
        "INSERT INTO accounting_opening_corrections (id, workspace_id, baseline_id, accounting_date,
             outstanding_delta, carrying_delta, settled_delta, effect_id, reason, evidence_ref,
             created_by)
         VALUES ($1, $2, $3, $4, $5, $5, -$5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(ws)
    .bind(c.baseline)
    .bind(c.date)
    .bind(p.delta)
    .bind(effect)
    .bind(&c.reason)
    .bind(&c.evidence)
    .bind(access.user_id)
    .execute(&mut *tx)
    .await?;
    if p.receivable {
        crate::modules::invoice_payments::service::refresh(&mut tx, ws, p.source).await?;
    }
    let mut data = p.review;
    data["id"] = json!(id);
    data["effect_id"] = json!(effect);
    let answer = super::service::created(&data);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
