//! Canonical settlement (Flow ERP M4, masterplan §8): open items, dated allocations, source
//! links, and `MoneyLocation`, the one attribution of where money sits (cash place, bank
//! account, provider clearing). Every fn runs in the caller's transaction. Money math lives in
//! the `accounting_allocation_applied` trigger (residual + functional relief in numeric).
// shortcut: `reverse` and `residual` have no caller until C9/credits land; drop the allow then.
#![allow(dead_code)]

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::Rules,
    modules::cash::dto::{Place, PlaceRequest, place},
};

/// Trigger/CHECK refusal (`23514`) of a settlement row: the caller's facts don't fit.
const MISFIT: &str =
    "The allocation does not fit the open items' party, currency, date or balance.";

/// An item to open; residual starts at `amount`.
pub struct NewOpenItem<'a> {
    pub classification: &'a str,
    /// `debit` (party owes the book) or `credit` (book owes the party).
    pub side: &'a str,
    /// Mapped role of `account_id`, e.g. `receivables` or `unbilled_receivables`.
    pub account_role: &'a str,
    pub account_id: Uuid,
    pub source_kind: &'a str,
    pub source_id: Uuid,
    pub relation_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub invoice_id: Option<Uuid>,
    pub recognition_id: Option<Uuid>,
    pub currency_id: Uuid,
    pub issue_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    /// `posted` or `opening`.
    pub origin: &'a str,
    /// Transaction minor units, > 0.
    pub amount: i64,
    /// Functional carrying basis, > 0.
    pub functional_amount: i64,
    pub effect_id: Uuid,
    pub by: Uuid,
}

/// Open `i`; its id. Same source + classification twice is a database error (identity).
pub async fn open_item(
    conn: &mut PgConnection,
    ws: Uuid,
    i: &NewOpenItem<'_>,
) -> Result<Uuid, AppError> {
    Ok(sqlx::query_scalar(
        "INSERT INTO accounting_open_items (workspace_id, classification, side, account_role,
             account_id, source_kind, source_id, relation_id, order_id, invoice_id,
             recognition_id, currency_id, issue_date, due_date, origin, amount,
             functional_amount, residual, functional_residual, effect_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                 $16, $17, $18, $19)
         RETURNING id",
    )
    .bind(ws)
    .bind(i.classification)
    .bind(i.side)
    .bind(i.account_role)
    .bind(i.account_id)
    .bind(i.source_kind)
    .bind(i.source_id)
    .bind(i.relation_id)
    .bind(i.order_id)
    .bind(i.invoice_id)
    .bind(i.recognition_id)
    .bind(i.currency_id)
    .bind(i.issue_date)
    .bind(i.due_date)
    .bind(i.origin)
    .bind(i.amount)
    .bind(i.functional_amount)
    .bind(i.effect_id)
    .bind(i.by)
    .fetch_one(conn)
    .await?)
}

/// One allocation: `application` (source + target, opposite sides), `reclassification`
/// (same side, target is the item just opened for `amount`) or `rounding` (target only).
pub struct NewAllocation<'a> {
    pub kind: &'a str,
    pub source_item_id: Option<Uuid>,
    pub target_item_id: Uuid,
    pub date: NaiveDate,
    pub currency_id: Uuid,
    /// Transaction minor units, > 0.
    pub amount: i64,
    pub effect_id: Uuid,
    pub by: Uuid,
}

/// Lock the items in id order (two allocations over the same pair never deadlock).
async fn lock_items(conn: &mut PgConnection, ws: Uuid, ids: &[Uuid]) -> Result<(), AppError> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let found: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM accounting_open_items WHERE workspace_id = $1 AND id = ANY($2)
         ORDER BY id FOR NO KEY UPDATE",
    )
    .bind(ws)
    .bind(&ids)
    .fetch_all(conn)
    .await?;
    if found.len() == ids.len() {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

fn misfit(e: sqlx::Error) -> AppError {
    match e.as_database_error().and_then(|d| d.code()) {
        Some(c) if c == "23514" => AppError::Unprocessable(MISFIT.into()),
        _ => e.into(),
    }
}

/// Allocate `a` (partial allowed); its id. 404 for an item outside `ws`; 422 when the items
/// don't share party/currency, the date precedes an item, or the amount exceeds a residual.
pub async fn allocate(
    conn: &mut PgConnection,
    ws: Uuid,
    a: &NewAllocation<'_>,
) -> Result<Uuid, AppError> {
    let items: Vec<Uuid> = a
        .source_item_id
        .into_iter()
        .chain([a.target_item_id])
        .collect();
    lock_items(conn, ws, &items).await?;
    sqlx::query_scalar(
        "INSERT INTO accounting_allocations (workspace_id, kind, source_item_id, target_item_id,
             accounting_date, currency_id, amount, effect_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(ws)
    .bind(a.kind)
    .bind(a.source_item_id)
    .bind(a.target_item_id)
    .bind(a.date)
    .bind(a.currency_id)
    .bind(a.amount)
    .bind(a.effect_id)
    .bind(a.by)
    .fetch_one(conn)
    .await
    .map_err(misfit)
}

/// Reverse allocation `id` on `date` under `effect_id` (restores both residuals); its id.
/// Reclassifications can't be reversed here.
pub async fn reverse(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    date: NaiveDate,
    effect_id: Uuid,
    by: Uuid,
) -> Result<Uuid, AppError> {
    let items: Option<(Option<Uuid>, Uuid)> = sqlx::query_as(
        "SELECT source_item_id, target_item_id FROM accounting_allocations
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let (source, target) = items.ok_or(AppError::NotFound)?;
    let ids: Vec<Uuid> = source.into_iter().chain([target]).collect();
    lock_items(conn, ws, &ids).await?;
    sqlx::query_scalar(
        "INSERT INTO accounting_allocations (workspace_id, kind, source_item_id, target_item_id,
             accounting_date, currency_id, amount, effect_id, reversal_of, created_by)
         SELECT workspace_id, kind, source_item_id, target_item_id, $3, currency_id, amount, $4,
                id, $5
         FROM accounting_allocations WHERE workspace_id = $1 AND id = $2
         RETURNING id",
    )
    .bind(ws)
    .bind(id)
    .bind(date)
    .bind(effect_id)
    .bind(by)
    .fetch_one(conn)
    .await
    .map_err(|e| match e.as_database_error().and_then(|d| d.code()) {
        // UNIQUE(reversal_of): already reversed.
        Some(c) if c == "23505" => {
            AppError::Unprocessable("This allocation is already reversed.".into())
        }
        _ => misfit(e),
    })
}

/// An item's amounts now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct Residual {
    pub amount: i64,
    pub residual: i64,
    pub functional_amount: i64,
    pub functional_residual: i64,
}

/// Item `id`'s residual (read as committed or as this transaction wrote it).
pub async fn residual(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<Residual, AppError> {
    sqlx::query_as(
        "SELECT amount, residual, functional_amount, functional_residual
         FROM accounting_open_items WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Record how source `kind`/`id` reached the book (`posted`, `opening_covered`,
/// `pending_observation`). One link per source.
pub async fn link_source(
    conn: &mut PgConnection,
    ws: Uuid,
    source_kind: &str,
    source_id: Uuid,
    effect_id: Option<Uuid>,
    disposition: &str,
    fingerprint: &[u8; 32],
) -> Result<Uuid, AppError> {
    Ok(sqlx::query_scalar(
        "INSERT INTO accounting_source_links (workspace_id, source_kind, source_id, effect_id,
             disposition, fingerprint)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(ws)
    .bind(source_kind)
    .bind(source_id)
    .bind(effect_id)
    .bind(disposition)
    .bind(fingerprint.as_slice())
    .fetch_one(conn)
    .await?)
}

/// A journal side: a mapped role, or a concrete account (a bank account's GL account).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Role(&'static str),
    Account(Uuid),
}

/// How a noncash amount is proven (masterplan §9 MANUAL NONCASH SOURCES).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof {
    pub reference: String,
    pub evidence_ref: String,
    pub confirmation_method: &'static str,
}

/// Where money actually is: a drawer/vault, a bank account, or the provider clearing account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoneyLocation {
    CashPlace(Place),
    BankAccount { id: Uuid, proof: Proof },
    ProviderClearing(Proof),
}

const METHODS: [&str; 4] = [
    "counted_cash",
    "bank_statement",
    "provider_dashboard",
    "provider_callback",
];

/// `{kind: cash_place|bank_account|provider_clearing, ...}` as requests send it.
#[derive(Deserialize, ToSchema, Default)]
pub struct MoneyLocationRequest {
    /// `cash_place`, `bank_account` or `provider_clearing`.
    #[schema(example = "cash_place")]
    kind: Option<String>,
    /// With `cash_place`: `{kind: drawer|vault, id}`.
    cash_place: Option<PlaceRequest>,
    /// With `bank_account`.
    #[schema(value_type = Option<Uuid>)]
    bank_account_id: Option<String>,
    /// Bank/provider reference, 1–255 (noncash only).
    reference: Option<String>,
    /// Where the proof is kept, 1–255 (noncash only).
    evidence_ref: Option<String>,
    /// `counted_cash`, `bank_statement`, `provider_dashboard` or `provider_callback` (noncash only).
    confirmation_method: Option<String>,
}

impl MoneyLocationRequest {
    /// The validated location; errors under `field.*`.
    pub fn validate(self, r: &mut Rules, field: &str) -> Option<MoneyLocation> {
        let kind = r.required(&format!("{field}.kind"), self.kind)?;
        let proof = |r: &mut Rules| {
            let reference = r.required(&format!("{field}.reference"), self.reference.clone());
            let evidence = r.required(&format!("{field}.evidence_ref"), self.evidence_ref.clone());
            for (f, v) in [("reference", &reference), ("evidence_ref", &evidence)] {
                if let Some(v) = v {
                    r.max_chars(&format!("{field}.{f}"), v, 255);
                }
            }
            let method = r
                .required(
                    &format!("{field}.confirmation_method"),
                    self.confirmation_method.clone(),
                )
                .and_then(|m| {
                    let found = METHODS.iter().find(|x| **x == m).copied();
                    r.check(
                        &format!("{field}.confirmation_method"),
                        found.is_some(),
                        "The confirmation method is invalid.",
                    );
                    found
                });
            Some(Proof {
                reference: reference.filter(|v| v.chars().count() <= 255)?,
                evidence_ref: evidence.filter(|v| v.chars().count() <= 255)?,
                confirmation_method: method?,
            })
        };
        match kind.as_str() {
            "cash_place" => place(r, &format!("{field}.cash_place"), self.cash_place)
                .map(MoneyLocation::CashPlace),
            "bank_account" => {
                let id = r
                    .required(
                        &format!("{field}.bank_account_id"),
                        self.bank_account_id.clone(),
                    )
                    .and_then(|v| Uuid::parse_str(&v).ok());
                r.check(
                    &format!("{field}.bank_account_id"),
                    id.is_some() || self.bank_account_id.is_none(),
                    "The bank account id must be a valid UUID.",
                );
                let proof = proof(r);
                Some(MoneyLocation::BankAccount {
                    id: id?,
                    proof: proof?,
                })
            }
            "provider_clearing" => proof(r).map(MoneyLocation::ProviderClearing),
            _ => {
                r.add(
                    &format!("{field}.kind"),
                    "The kind must be cash_place, bank_account or provider_clearing.",
                );
                None
            }
        }
    }
}

/// A location's persisted columns (same names on every table that stores one).
#[derive(Debug, Serialize, ToSchema)]
pub struct LocationColumns<'a> {
    pub location_kind: &'static str,
    pub register_id: Option<Uuid>,
    pub vault_id: Option<Uuid>,
    pub bank_account_id: Option<Uuid>,
    pub reference: Option<&'a str>,
    pub evidence_ref: Option<&'a str>,
    pub confirmation_method: Option<&'static str>,
}

/// A location's facts under its lock.
#[derive(Debug, Clone, Copy)]
pub struct Located {
    pub currency_id: Uuid,
    /// Journal side money enters on: role `cash`, the bank's GL account, role `provider_clearing`.
    pub side: Side,
    /// Open shift of a drawer (drawer money rows need one).
    pub shift_id: Option<Uuid>,
}

impl MoneyLocation {
    /// Persisted columns.
    pub fn columns(&self) -> LocationColumns<'_> {
        let proof = match self {
            Self::BankAccount { proof, .. } | Self::ProviderClearing(proof) => Some(proof),
            Self::CashPlace(_) => None,
        };
        LocationColumns {
            location_kind: match self {
                Self::CashPlace(_) => "cash_place",
                Self::BankAccount { .. } => "bank_account",
                Self::ProviderClearing(_) => "provider_clearing",
            },
            register_id: match self {
                Self::CashPlace(Place::Drawer(id)) => Some(*id),
                _ => None,
            },
            vault_id: match self {
                Self::CashPlace(Place::Vault(id)) => Some(*id),
                _ => None,
            },
            bank_account_id: match self {
                Self::BankAccount { id, .. } => Some(*id),
                _ => None,
            },
            reference: proof.map(|p| p.reference.as_str()),
            evidence_ref: proof.map(|p| p.evidence_ref.as_str()),
            confirmation_method: proof.map(|p| p.confirmation_method),
        }
    }

    /// Lock the place (register/vault FOR NO KEY UPDATE, bank account FOR SHARE) of `ws`;
    /// 404 when it isn't there, 422 for an inactive bank account. Provider clearing locks
    /// nothing; its currency is `fallback_currency` (the source's).
    pub async fn lock(
        &self,
        conn: &mut PgConnection,
        ws: Uuid,
        fallback_currency: Uuid,
    ) -> Result<Located, AppError> {
        match self {
            Self::CashPlace(Place::Drawer(id)) => {
                let (currency_id, shift_id): (Uuid, Option<Uuid>) = sqlx::query_as(
                    "SELECT r.currency_id, (SELECT s.id FROM drawer_shifts s
                         WHERE s.register_id = r.id AND s.closed_at IS NULL)
                     FROM till_registers r WHERE r.workspace_id = $1 AND r.id = $2
                     FOR NO KEY UPDATE OF r",
                )
                .bind(ws)
                .bind(id)
                .fetch_optional(conn)
                .await?
                .ok_or(AppError::NotFound)?;
                Ok(Located {
                    currency_id,
                    side: Side::Role("cash"),
                    shift_id,
                })
            }
            Self::CashPlace(Place::Vault(id)) => {
                let currency_id: Uuid = sqlx::query_scalar(
                    "SELECT currency_id FROM cash_vaults WHERE workspace_id = $1 AND id = $2
                     FOR NO KEY UPDATE",
                )
                .bind(ws)
                .bind(id)
                .fetch_optional(conn)
                .await?
                .ok_or(AppError::NotFound)?;
                Ok(Located {
                    currency_id,
                    side: Side::Role("cash"),
                    shift_id: None,
                })
            }
            Self::BankAccount { id, .. } => {
                let (currency_id, account, active): (Uuid, Uuid, bool) = sqlx::query_as(
                    "SELECT currency_id, account_id, active FROM bank_accounts
                     WHERE workspace_id = $1 AND id = $2 FOR SHARE",
                )
                .bind(ws)
                .bind(id)
                .fetch_optional(conn)
                .await?
                .ok_or(AppError::NotFound)?;
                if !active {
                    return Err(AppError::field(
                        "attribution.bank_account_id",
                        "The bank account is inactive.",
                    ));
                }
                Ok(Located {
                    currency_id,
                    side: Side::Account(account),
                    shift_id: None,
                })
            }
            Self::ProviderClearing(_) => Ok(Located {
                currency_id: fallback_currency,
                side: Side::Role("provider_clearing"),
                shift_id: None,
            }),
        }
    }
}

/// Account mapped to `role` in the book; 422 `accounting_not_configured` when unmapped.
pub async fn role_account(conn: &mut PgConnection, ws: Uuid, role: &str) -> Result<Uuid, AppError> {
    sqlx::query_scalar(
        "SELECT account_id FROM account_mappings WHERE workspace_id = $1 AND role = $2",
    )
    .bind(ws)
    .bind(role)
    .fetch_optional(conn)
    .await?
    .ok_or_else(crate::modules::accounting::service::not_configured)
}

/// Paid projection of an invoice in an active book (masterplan §14 OPENING PAYMENT CONTRACT):
/// reviewed baseline paid (settled − refunded to cutover) + applications and explicit rounding
/// relieved from its receivable items, net of reversals + dated opening corrections.
/// Reclassification is not payment; a correction's own application counts once, as correction.
pub async fn invoice_paid(
    conn: &mut PgConnection,
    ws: Uuid,
    invoice: Uuid,
) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT (COALESCE((SELECT sum(CASE WHEN a.reversal_of IS NULL THEN a.amount ELSE -a.amount END)
                  FROM accounting_open_items i
                  JOIN accounting_allocations a ON a.workspace_id = i.workspace_id
                       AND (a.target_item_id = i.id OR a.source_item_id = i.id)
                  WHERE i.workspace_id = $1 AND i.invoice_id = $2 AND i.classification = 'receivable'
                    AND a.kind IN ('application', 'rounding')
                    AND NOT EXISTS (SELECT 1 FROM accounting_open_items c
                                    WHERE c.workspace_id = a.workspace_id
                                      AND c.id IN (a.source_item_id, a.target_item_id)
                                      AND c.source_kind = 'opening_correction')), 0)
                + COALESCE((SELECT b.settled - b.refunded
                              + COALESCE((SELECT sum(c.settled_delta) FROM accounting_opening_corrections c
                                          WHERE c.workspace_id = b.workspace_id AND c.baseline_id = b.id), 0)
                            FROM accounting_opening_baselines b
                            WHERE b.workspace_id = $1 AND b.source_kind = 'invoice'
                              AND b.source_id = $2), 0))::bigint",
    )
    .bind(ws)
    .bind(invoice)
    .fetch_one(conn)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::json::Rules;
    use serde_json::json;
    use sqlx::PgPool;

    struct Book {
        ws: Uuid,
        user: Uuid,
        eur: Uuid,
        ar: Uuid,
        deposits: Uuid,
        relation: Uuid,
    }

    async fn book(pool: &PgPool, name: &str) -> Book {
        let user: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, first_name, last_name, password_hash)
             VALUES ($1, 'O', 'W', 'h') RETURNING id",
        )
        .bind(format!("{name}@x.io"))
        .fetch_one(pool)
        .await
        .unwrap();
        let ws: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, created_by_user_id) VALUES ($2, $1) RETURNING id",
        )
        .bind(user)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap();
        let eur = sqlx::query_scalar(
            "INSERT INTO currencies (workspace_id, name, asset_type, decimals, active)
             VALUES ($1, 'Euro', 'EUR', 2, true) RETURNING id",
        )
        .bind(ws)
        .fetch_one(pool)
        .await
        .unwrap();
        let account = |code: &'static str, ty: &'static str| {
            sqlx::query_scalar(
                "INSERT INTO accounts (workspace_id, code, name, type, created_by)
                 VALUES ($1, $2, $2, $3, $4) RETURNING id",
            )
            .bind(ws)
            .bind(code)
            .bind(ty)
            .bind(user)
            .fetch_one(pool)
        };
        let ar = account("1300", "asset").await.unwrap();
        let deposits = account("2300", "liability").await.unwrap();
        let relation = sqlx::query_scalar(
            "WITH t AS (INSERT INTO custom_types (workspace_id, name, domain)
                       VALUES ($1, 'Customer', 'relation') RETURNING id)
             INSERT INTO relations (workspace_id, relation_type_id) SELECT $1, id FROM t
             RETURNING id",
        )
        .bind(ws)
        .fetch_one(pool)
        .await
        .unwrap();
        Book {
            ws,
            user,
            eur,
            ar,
            deposits,
            relation,
        }
    }

    async fn effect(conn: &mut PgConnection, b: &Book) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO accounting_effects (workspace_id, source_kind, source_id, effect_kind,
                 fingerprint, created_by)
             VALUES ($1, 'test', uuidv7(), 'posted', decode(repeat('00', 32), 'hex'), $2)
             RETURNING id",
        )
        .bind(b.ws)
        .bind(b.user)
        .fetch_one(conn)
        .await
        .unwrap()
    }

    fn date(d: &str) -> NaiveDate {
        d.parse().unwrap()
    }

    async fn item(
        conn: &mut PgConnection,
        b: &Book,
        classification: &str,
        role: &str,
        amount: i64,
        relation: Option<Uuid>,
    ) -> Uuid {
        let (side, account) = if classification == "receivable" {
            ("debit", b.ar)
        } else {
            ("credit", b.deposits)
        };
        let effect_id = effect(conn, b).await;
        open_item(
            conn,
            b.ws,
            &NewOpenItem {
                classification,
                side,
                account_role: role,
                account_id: account,
                source_kind: "test",
                source_id: Uuid::now_v7(),
                relation_id: relation,
                order_id: None,
                invoice_id: None,
                recognition_id: None,
                currency_id: b.eur,
                issue_date: date("2027-01-10"),
                due_date: None,
                origin: "posted",
                amount,
                functional_amount: amount,
                effect_id,
                by: b.user,
            },
        )
        .await
        .unwrap()
    }

    async fn alloc(
        conn: &mut PgConnection,
        b: &Book,
        kind: &str,
        source: Option<Uuid>,
        target: Uuid,
        amount: i64,
        on: &str,
    ) -> Result<Uuid, AppError> {
        let effect_id = effect(conn, b).await;
        allocate(
            conn,
            b.ws,
            &NewAllocation {
                kind,
                source_item_id: source,
                target_item_id: target,
                date: date(on),
                currency_id: b.eur,
                amount,
                effect_id,
                by: b.user,
            },
        )
        .await
    }

    #[sqlx::test]
    async fn partial_allocations_move_both_residuals_until_settled(pool: PgPool) {
        let b = book(&pool, "a").await;
        let mut c = pool.acquire().await.unwrap();
        let ar = item(
            &mut c,
            &b,
            "receivable",
            "receivables",
            10_000,
            Some(b.relation),
        )
        .await;
        let dep = item(
            &mut c,
            &b,
            "customer_deposit",
            "customer_deposits",
            12_000,
            Some(b.relation),
        )
        .await;
        alloc(
            &mut c,
            &b,
            "application",
            Some(dep),
            ar,
            4_000,
            "2027-01-11",
        )
        .await
        .unwrap();
        let r = residual(&mut c, b.ws, ar).await.unwrap();
        assert_eq!((r.residual, r.functional_residual), (6_000, 6_000));
        assert_eq!(residual(&mut c, b.ws, dep).await.unwrap().residual, 8_000);
        // More than the receivable has open: refused, nothing moves.
        let over = alloc(
            &mut c,
            &b,
            "application",
            Some(dep),
            ar,
            6_001,
            "2027-01-11",
        )
        .await;
        assert!(matches!(over, Err(AppError::Unprocessable(_))));
        alloc(
            &mut c,
            &b,
            "application",
            Some(dep),
            ar,
            6_000,
            "2027-01-12",
        )
        .await
        .unwrap();
        assert_eq!(residual(&mut c, b.ws, ar).await.unwrap().residual, 0);
        assert_eq!(residual(&mut c, b.ws, dep).await.unwrap().residual, 2_000);
    }

    #[sqlx::test]
    async fn allocations_refuse_other_parties_dates_sides_and_workspaces(pool: PgPool) {
        let b = book(&pool, "a").await;
        let other = book(&pool, "b").await;
        let mut c = pool.acquire().await.unwrap();
        let ar = item(
            &mut c,
            &b,
            "receivable",
            "receivables",
            10_000,
            Some(b.relation),
        )
        .await;
        let anon = item(
            &mut c,
            &b,
            "customer_deposit",
            "customer_deposits",
            1_000,
            None,
        )
        .await;
        let dep = item(
            &mut c,
            &b,
            "customer_deposit",
            "customer_deposits",
            1_000,
            Some(b.relation),
        )
        .await;
        let ar2 = item(
            &mut c,
            &b,
            "receivable",
            "receivables",
            1_000,
            Some(b.relation),
        )
        .await;
        let unprocessable =
            |r: Result<Uuid, AppError>| matches!(r, Err(AppError::Unprocessable(_)));
        // Another party (anonymous vs relation).
        assert!(unprocessable(
            alloc(&mut c, &b, "application", Some(anon), ar, 100, "2027-01-11").await
        ));
        // Dated before the items were issued.
        assert!(unprocessable(
            alloc(&mut c, &b, "application", Some(dep), ar, 100, "2027-01-09").await
        ));
        // Same side.
        assert!(unprocessable(
            alloc(&mut c, &b, "application", Some(ar2), ar, 100, "2027-01-11").await
        ));
        // Another workspace's item: not found.
        let theirs = item(
            &mut c,
            &other,
            "customer_deposit",
            "customer_deposits",
            1_000,
            None,
        )
        .await;
        let r = alloc(
            &mut c,
            &b,
            "application",
            Some(theirs),
            ar,
            100,
            "2027-01-11",
        )
        .await;
        assert!(matches!(r, Err(AppError::NotFound)));
        assert_eq!(residual(&mut c, b.ws, ar).await.unwrap().residual, 10_000);
        assert!(matches!(
            residual(&mut c, other.ws, ar).await,
            Err(AppError::NotFound)
        ));
    }

    #[sqlx::test]
    async fn anonymous_items_settle_and_rounding_relieves_one_item(pool: PgPool) {
        let b = book(&pool, "a").await;
        let mut c = pool.acquire().await.unwrap();
        let ar = item(&mut c, &b, "receivable", "receivables", 10_000, None).await;
        let dep = item(
            &mut c,
            &b,
            "customer_deposit",
            "customer_deposits",
            9_995,
            None,
        )
        .await;
        alloc(
            &mut c,
            &b,
            "application",
            Some(dep),
            ar,
            9_995,
            "2027-01-10",
        )
        .await
        .unwrap();
        alloc(&mut c, &b, "rounding", None, ar, 5, "2027-01-10")
            .await
            .unwrap();
        assert_eq!(residual(&mut c, b.ws, ar).await.unwrap().residual, 0);
    }

    #[sqlx::test]
    async fn reclassification_moves_unbilled_to_billed(pool: PgPool) {
        let b = book(&pool, "a").await;
        let mut c = pool.acquire().await.unwrap();
        let unbilled = item(
            &mut c,
            &b,
            "receivable",
            "unbilled_receivables",
            12_000,
            Some(b.relation),
        )
        .await;
        let billed = item(
            &mut c,
            &b,
            "receivable",
            "receivables",
            12_000,
            Some(b.relation),
        )
        .await;
        alloc(
            &mut c,
            &b,
            "reclassification",
            Some(unbilled),
            billed,
            12_000,
            "2027-01-11",
        )
        .await
        .unwrap();
        assert_eq!(residual(&mut c, b.ws, unbilled).await.unwrap().residual, 0);
        assert_eq!(
            residual(&mut c, b.ws, billed).await.unwrap().residual,
            12_000
        );
        // Same role is no reclassification.
        let again = item(
            &mut c,
            &b,
            "receivable",
            "receivables",
            1_000,
            Some(b.relation),
        )
        .await;
        let src = item(
            &mut c,
            &b,
            "receivable",
            "receivables",
            1_000,
            Some(b.relation),
        )
        .await;
        let r = alloc(
            &mut c,
            &b,
            "reclassification",
            Some(src),
            again,
            1_000,
            "2027-01-11",
        )
        .await;
        assert!(matches!(r, Err(AppError::Unprocessable(_))));
    }

    #[sqlx::test]
    async fn reversal_restores_residuals_once(pool: PgPool) {
        let b = book(&pool, "a").await;
        let mut c = pool.acquire().await.unwrap();
        let ar = item(
            &mut c,
            &b,
            "receivable",
            "receivables",
            10_000,
            Some(b.relation),
        )
        .await;
        let dep = item(
            &mut c,
            &b,
            "customer_deposit",
            "customer_deposits",
            10_000,
            Some(b.relation),
        )
        .await;
        let a = alloc(
            &mut c,
            &b,
            "application",
            Some(dep),
            ar,
            10_000,
            "2027-01-11",
        )
        .await
        .unwrap();
        let e = effect(&mut c, &b).await;
        reverse(&mut c, b.ws, a, date("2027-01-12"), e, b.user)
            .await
            .unwrap();
        assert_eq!(residual(&mut c, b.ws, ar).await.unwrap().residual, 10_000);
        assert_eq!(residual(&mut c, b.ws, dep).await.unwrap().residual, 10_000);
        let e = effect(&mut c, &b).await;
        let twice = reverse(&mut c, b.ws, a, date("2027-01-12"), e, b.user).await;
        assert!(matches!(twice, Err(AppError::Unprocessable(_))));
    }

    fn parse(v: serde_json::Value) -> (Option<MoneyLocation>, Rules) {
        let mut r = Rules::default();
        let req: MoneyLocationRequest = serde_json::from_value(v).unwrap();
        (req.validate(&mut r, "attribution"), r)
    }

    #[test]
    fn money_locations_parse_one_shape() {
        let id = Uuid::now_v7();
        let (cash, _) =
            parse(json!({"kind": "cash_place", "cash_place": {"kind": "vault", "id": id}}));
        assert_eq!(cash, Some(MoneyLocation::CashPlace(Place::Vault(id))));
        let (bank, _) = parse(json!({"kind": "bank_account", "bank_account_id": id,
            "reference": "TX1", "evidence_ref": "stmt-11", "confirmation_method": "bank_statement"}));
        assert_eq!(bank.unwrap().columns().bank_account_id, Some(id));
        let (clearing, _) = parse(json!({"kind": "provider_clearing", "reference": "tr_1",
            "evidence_ref": "dash", "confirmation_method": "provider_dashboard"}));
        assert_eq!(
            clearing.unwrap().columns().location_kind,
            "provider_clearing"
        );
        let (none, r) = parse(json!({"kind": "bank_account", "bank_account_id": id}));
        assert!(none.is_none());
        assert!(r.into_result().is_err());
        let (none, _) = parse(json!({"kind": "provider_clearing", "reference": "x",
            "evidence_ref": "y", "confirmation_method": "guess"}));
        assert!(none.is_none());
        let (none, _) = parse(json!({"kind": "pocket"}));
        assert!(none.is_none());
    }
}
