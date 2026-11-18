//! Moving-average inventory valuation (Flow ERP M2, masterplan §7/§10). Active books only:
//! one cost pool per product/warehouse, locked before its stock rows; every level change gets
//! an immutable value entry and, when value crosses inventory ↔ P&L, one source journal.
//! Provenance the stock layer can't identify is refused; the caller's transaction rolls back.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    error::AppError,
    idempotency::{self, Actor, Answer, Key},
    modules::{
        accounting::{
            gate,
            journals::{self, SourceJournal},
            service::unsupported,
            settlement::Side,
        },
        purchasing,
        stock::repo::{Cause, StockRow},
    },
};

/// How a writer's movement is valued. `Infer` reads `cause.related` and the direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Valued {
    #[default]
    Infer,
    /// Between bins of one warehouse: the pool doesn't change.
    Relocate,
    /// Between warehouses: the in leg takes exactly what the out leg removed.
    Transfer,
    /// Raw kg into packed units (D19): as `Transfer`, across products.
    Pack,
    /// Booking of a purchase receipt line (M3): adds `value` (functional minor units) dated
    /// `date`, DR inventory / CR GRNI once per line, in the pool's eligible receipt order.
    Receipt {
        line: Uuid,
        value: i64,
        date: NaiveDate,
    },
    /// Stocktake difference (M6) dated `date`: out at the current average; in at `value`
    /// (functional minor units) when the caller found the pool empty, else at the average.
    Count { value: Option<i64>, date: NaiveDate },
    /// Customer return restock (M4): in at the original booked issue cost of `sale`
    /// (`order`|`invoice`, id), DR inventory / CR cogs; or at reviewed `legacy` return cost,
    /// DR inventory / CR inventory_adjustment.
    SaleReturn {
        sale: (&'static str, Uuid),
        legacy: Option<Uuid>,
    },
    /// Exact undo of such a restock: out at `value`, DR cogs (legacy: inventory_adjustment)
    /// / CR inventory; what the sale (or legacy record) counts as returned shrinks.
    SaleReturnUndo {
        sale: (&'static str, Uuid),
        legacy: Option<Uuid>,
        value: i64,
    },
}

const NO_OPENING: &str =
    "This stock has no value in the book yet. Its opening value comes with activation.";
const EMPTY_POOL: &str = "There is no cost to add this stock at. A reviewed value is needed for stock into an empty pool.";
const UNKNOWN: &str = "This stock change isn't supported while the accounting book is active.";

fn reason(code: &'static str, message: &str) -> AppError {
    AppError::Reason {
        status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        code,
        message: message.into(),
    }
}

/// 422 `stocktake_value_required`: counted stock into an empty pool needs a reviewed value.
pub fn value_required() -> AppError {
    reason(
        "stocktake_value_required",
        "This product has no cost here yet. Enter the value of the stock found.",
    )
}

/// 422 `insufficient_stock`: an issue larger than the pool.
pub fn insufficient() -> AppError {
    AppError::insufficient_stock("There isn't enough stock for this.")
}

/// Shared book gate, then whether the book is active. Every stock writer comes here through
/// [`crate::modules::stock::repo::locked`]; the gate is re-entrant within a transaction.
pub async fn active(conn: &mut PgConnection, ws: Uuid) -> Result<bool, AppError> {
    gate::shared(&mut *conn, &[ws]).await?;
    // After the gate, in its own statement: activation commits under the exclusive gate.
    let state = sqlx::query_scalar!(
        "SELECT state FROM accounting_settings WHERE workspace_id = $1",
        ws
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(state.as_deref() == Some("active"))
}

/// 422 `unsupported_active_erp_operation` when the book is active (masterplan §16 guards).
pub async fn refuse_active(
    conn: &mut PgConnection,
    ws: Uuid,
    message: &str,
) -> Result<(), AppError> {
    if active(conn, ws).await? {
        return Err(unsupported(message));
    }
    Ok(())
}

/// How an economic stock write replays: today's daily Idempotency-Key replay in a setup
/// book; permanent and key-required in an active one (masterplan §5/§6).
pub struct Replay {
    active: bool,
    operation: &'static str,
    user_id: Uuid,
}

/// Start `operation`'s replay in the write's transaction (gate first); the stored answer when
/// this request already ran.
pub async fn replay(
    conn: &mut PgConnection,
    ws: Uuid,
    operation: &'static str,
    user_id: Uuid,
    key: Option<&Key>,
) -> Result<(Replay, Option<Answer>), AppError> {
    let active = active(conn, ws).await?;
    let answer = if active {
        idempotency::permanent_replay(conn, ws, operation, user_id, key).await?
    } else {
        idempotency::replay(conn, ws, Actor::User(user_id), key).await?
    };
    let r = Replay {
        active,
        operation,
        user_id,
    };
    Ok((r, answer))
}

impl Replay {
    /// Whether the book was active when the write started (it can't change before commit).
    pub fn active(&self) -> bool {
        self.active
    }

    /// Store this write's answer, in its transaction.
    pub async fn finish(
        &self,
        conn: &mut PgConnection,
        ws: Uuid,
        key: Option<&Key>,
        answer: &Answer,
    ) -> Result<(), AppError> {
        match key {
            Some(k) if self.active => {
                idempotency::permanent_finish(conn, ws, self.operation, self.user_id, k, answer)
                    .await?
            }
            _ => idempotency::finish(conn, ws, key, answer).await?,
        }
        Ok(())
    }
}

const PACKING: &str =
    "Packing in an active book takes kg of one product into pieces (pc or single) of another.";

/// D19: same-book packing only from base unit kg into pc/single of a different product.
pub async fn check_packing(
    conn: &mut PgConnection,
    ws: Uuid,
    input: Uuid,
    output: Uuid,
) -> Result<(), AppError> {
    let units = sqlx::query!(
        "SELECT id, unit FROM products WHERE workspace_id = $1 AND id = ANY($2)",
        ws,
        &[input, output]
    )
    .fetch_all(&mut *conn)
    .await?;
    let unit = |id| {
        units
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.unit.as_deref())
    };
    let ok = input != output
        && unit(input) == Some("kg")
        && matches!(unit(output), Some("pc" | "single"));
    if ok {
        Ok(())
    } else {
        Err(unsupported(PACKING))
    }
}

struct Pool {
    id: Uuid,
    quantity: Decimal,
    value: i64,
    last_date: Option<NaiveDate>,
}

async fn lock_pool(
    conn: &mut PgConnection,
    ws: Uuid,
    product: Uuid,
    warehouse: Uuid,
) -> Result<Option<Pool>, sqlx::Error> {
    sqlx::query_as!(
        Pool,
        r#"SELECT id, quantity, value, last_accounting_date AS last_date
           FROM inventory_cost_pools
           WHERE workspace_id = $1 AND product_id = $2 AND warehouse_id = $3
           FOR UPDATE"#,
        ws,
        product,
        warehouse
    )
    .fetch_optional(conn)
    .await
}

/// Lock the product/warehouse pool of an active book, created when missing; its last valued
/// date. Callers hold the gate ([`active`]) and run it before the stock row lock, so pools lock
/// in slot-key order.
pub async fn pool(
    conn: &mut PgConnection,
    ws: Uuid,
    product: Uuid,
    warehouse: Uuid,
) -> Result<Option<NaiveDate>, AppError> {
    if let Some(p) = lock_pool(conn, ws, product, warehouse).await? {
        return Ok(p.last_date);
    }
    let on_hand = sqlx::query_scalar!(
        r#"SELECT coalesce(sum(stock_lvl), 0) AS "q!" FROM stocks
           WHERE workspace_id = $1 AND product_id = $2 AND warehouse_id = $3
             AND deleted_at IS NULL"#,
        ws,
        product,
        warehouse
    )
    .fetch_one(&mut *conn)
    .await?;
    // Stock from before activation has no cost here: never value it at zero.
    if !on_hand.is_zero() {
        return Err(unsupported(NO_OPENING));
    }
    sqlx::query!(
        "INSERT INTO inventory_cost_pools (workspace_id, product_id, warehouse_id)
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        ws,
        product,
        warehouse
    )
    .execute(&mut *conn)
    .await?;
    lock_pool(conn, ws, product, warehouse).await?;
    Ok(None)
}

/// round(v × q / of) in minor units, half away from zero, computed in SQL.
async fn share(conn: &mut PgConnection, v: i64, q: Decimal, of: Decimal) -> Result<i64, AppError> {
    let value = sqlx::query_scalar!(
        r#"SELECT money_round_div($1::bigint * $2::numeric, $3::numeric)::bigint AS "v!""#,
        v,
        q,
        of
    )
    .fetch_one(conn)
    .await?;
    Ok(value)
}

/// Value an issue of `q` takes from `pool`: all of its value when it empties the pool.
async fn issue_value(conn: &mut PgConnection, pool: &Pool, q: Decimal) -> Result<i64, AppError> {
    if q > pool.quantity {
        return Err(insufficient());
    }
    if q == pool.quantity {
        return Ok(pool.value);
    }
    share(conn, pool.value, q, pool.quantity).await
}

/// What a movement is and what it does to the pool.
struct Plan {
    class: &'static str,
    value: i64,
    /// (debit role, credit role) of the journal; None = inventory → inventory.
    roles: Option<(&'static str, &'static str)>,
}

const RECEIPT_BOOKING: &str = "receipt_booking_required";
const BOOK_FIRST: &str =
    "Goods received here are waiting to be booked. Book that receipt before this stock change.";
const BOOK_EARLIER: &str = "An earlier receipt of this stock is waiting. Book it first.";

const LEGACY_RETURN: &str =
    "This sale has no recorded cost in the book. Its return needs a reviewed cost.";

/// Original booked cost of returning `q` of `row` from sale `kind`/`id` (its cost
/// allocation): cumulative rounding, the return reaching the issued quantity takes the exact
/// rest. 422 `legacy_return_cost_required` without an allocation or beyond what was issued.
async fn restore(
    conn: &mut PgConnection,
    ws: Uuid,
    (kind, id): (&str, Uuid),
    row: &StockRow,
    q: Decimal,
) -> Result<i64, AppError> {
    let a = sqlx::query!(
        r#"SELECT id,
                  CASE WHEN returned_quantity + $6 > issued_quantity THEN NULL
                       WHEN returned_quantity + $6 = issued_quantity
                       THEN issued_value - returned_value
                       ELSE money_round_div(issued_value * (returned_quantity + $6),
                                            issued_quantity)::bigint - returned_value
                  END AS value
           FROM inventory_cost_allocations
           WHERE workspace_id = $1 AND source_kind = $2 AND source_id = $3
             AND product_id = $4 AND warehouse_id = $5
           FOR UPDATE"#,
        ws,
        kind,
        id,
        row.product_id,
        row.warehouse_id,
        q
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| reason("legacy_return_cost_required", LEGACY_RETURN))?;
    let value = a
        .value
        .ok_or_else(|| reason("legacy_return_cost_required", LEGACY_RETURN))?;
    sqlx::query!(
        "UPDATE inventory_cost_allocations
         SET returned_quantity = returned_quantity + $2, returned_value = returned_value + $3,
             version = version + 1, updated_at = now()
         WHERE id = $1",
        a.id,
        q,
        value
    )
    .execute(&mut *conn)
    .await?;
    Ok(value)
}

/// Reviewed legacy cost of restocking `q` of `row` (record `id`, same product): cumulative
/// share of its restoration amount, exact rest at its full quantity; 422 beyond it.
async fn legacy_value(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    row: &StockRow,
    q: Decimal,
) -> Result<i64, AppError> {
    let value: Option<Option<i64>> = sqlx::query_scalar(
        "SELECT CASE WHEN c.consumed_quantity + $4 > c.quantity
                       OR coalesce(c.target_product_id, l.product_id) <> $3 THEN NULL
                     WHEN c.consumed_quantity + $4 = c.quantity
                     THEN c.restoration_amount - c.consumed_value
                     ELSE money_round_div(c.restoration_amount * (c.consumed_quantity + $4),
                                          c.quantity)::bigint - c.consumed_value END
         FROM legacy_return_costs c
         JOIN order_lines l ON l.workspace_id = c.workspace_id AND l.id = c.order_line_id
         WHERE c.workspace_id = $1 AND c.id = $2
         FOR UPDATE OF c",
    )
    .bind(ws)
    .bind(id)
    .bind(row.product_id)
    .bind(q)
    .fetch_optional(&mut *conn)
    .await?;
    let value = value
        .flatten()
        .ok_or_else(|| reason("legacy_return_cost_required", LEGACY_RETURN))?;
    sqlx::query(
        "UPDATE legacy_return_costs SET consumed_quantity = consumed_quantity + $3,
                consumed_value = consumed_value + $4, version = version + 1
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .bind(q)
    .bind(value)
    .execute(conn)
    .await?;
    Ok(value)
}

/// Undo `value` of a restock of `q` of `row`: shrink what the sale's cost allocation (or
/// the legacy record) counts as returned.
async fn unrestore(
    conn: &mut PgConnection,
    ws: Uuid,
    (kind, id): (&str, Uuid),
    legacy: Option<Uuid>,
    (row, q, value): (&StockRow, Decimal, i64),
) -> Result<(), AppError> {
    let done = match legacy {
        Some(l) => sqlx::query(
            "UPDATE legacy_return_costs SET consumed_quantity = consumed_quantity - $3,
                    consumed_value = consumed_value - $4, version = version + 1
             WHERE workspace_id = $1 AND id = $2",
        )
        .bind(ws)
        .bind(l)
        .bind(q)
        .bind(value),
        None => sqlx::query(
            "UPDATE inventory_cost_allocations
             SET returned_quantity = returned_quantity - $5, returned_value = returned_value - $6,
                 version = version + 1, updated_at = now()
             WHERE workspace_id = $1 AND source_kind = $2 AND source_id = $3 AND product_id = $4
               AND warehouse_id = $7",
        )
        .bind(ws)
        .bind(kind)
        .bind(id)
        .bind(row.product_id)
        .bind(q)
        .bind(value)
        .bind(row.warehouse_id),
    }
    .execute(conn)
    .await?;
    if done.rows_affected() == 1 {
        Ok(())
    } else {
        Err(AppError::Internal(
            "restock undo without its cost record".into(),
        ))
    }
}

async fn plan(
    conn: &mut PgConnection,
    ws: Uuid,
    row: &StockRow,
    pool: &Pool,
    delta: Decimal,
    cause: &Cause<'_>,
) -> Result<Plan, AppError> {
    let q = delta.abs();
    let out = delta < Decimal::ZERO;
    let pair = match cause.value {
        Valued::SaleReturn { sale, legacy } if !out => {
            return Ok(match legacy {
                None => Plan {
                    class: "customer_return",
                    value: restore(&mut *conn, ws, sale, row, q).await?,
                    roles: Some(("inventory", "cogs")),
                },
                Some(l) => Plan {
                    class: "legacy_return",
                    value: legacy_value(&mut *conn, ws, l, row, q).await?,
                    roles: Some(("inventory", "inventory_adjustment")),
                },
            });
        }
        Valued::SaleReturnUndo {
            sale,
            legacy,
            value,
        } if out => {
            unrestore(&mut *conn, ws, sale, legacy, (row, q, value)).await?;
            let debit = if legacy.is_some() {
                "inventory_adjustment"
            } else {
                "cogs"
            };
            return Ok(Plan {
                class: "customer_return_reversal",
                value: -value,
                roles: Some((debit, "inventory")),
            });
        }
        Valued::SaleReturn { .. } | Valued::SaleReturnUndo { .. } => {
            return Err(unsupported(UNKNOWN));
        }
        Valued::Transfer => Some(("transfer_out", "transfer_in")),
        Valued::Pack => Some(("pack_out", "pack_in")),
        Valued::Receipt { value, .. } if !out => {
            return Ok(Plan {
                class: "purchase_receipt",
                value,
                roles: Some(("inventory", "grni")),
            });
        }
        Valued::Receipt { .. } => return Err(unsupported(UNKNOWN)),
        Valued::Count { value, .. } => {
            let value = match value {
                _ if out => -issue_value(&mut *conn, pool, q).await?,
                Some(v) => v,
                None if pool.quantity.is_zero() => return Err(value_required()),
                None => share(&mut *conn, pool.value, q, pool.quantity).await?,
            };
            let roles = if out {
                ("inventory_adjustment", "inventory")
            } else {
                ("inventory", "inventory_adjustment")
            };
            return Ok(Plan {
                class: "adjustment",
                value,
                roles: Some(roles),
            });
        }
        Valued::Relocate | Valued::Infer => None,
    };
    if let Some((out_class, in_class)) = pair {
        let (kind, id) = cause.related.ok_or_else(|| unsupported(UNKNOWN))?;
        if out {
            let value = issue_value(&mut *conn, pool, q).await?;
            return Ok(Plan {
                class: out_class,
                value: -value,
                roles: None,
            });
        }
        // What the out legs removed and the in legs haven't taken yet.
        let left = sqlx::query_scalar!(
            r#"SELECT coalesce(-sum(value_delta), 0)::bigint AS "left!"
               FROM inventory_value_entries
               WHERE workspace_id = $1 AND source_kind = $2 AND source_id = $3
                 AND classification = ANY($4)"#,
            ws,
            kind,
            id,
            &[out_class.to_owned(), in_class.to_owned()]
        )
        .fetch_one(&mut *conn)
        .await?;
        return Ok(Plan {
            class: in_class,
            value: left,
            roles: None,
        });
    }
    match cause.related {
        None if out => Ok(Plan {
            class: "adjustment",
            value: -issue_value(&mut *conn, pool, q).await?,
            roles: Some(("inventory_adjustment", "inventory")),
        }),
        None => {
            if pool.quantity.is_zero() {
                return Err(unsupported(EMPTY_POOL));
            }
            let value = share(&mut *conn, pool.value, q, pool.quantity).await?;
            Ok(Plan {
                class: "adjustment",
                value,
                roles: Some(("inventory", "inventory_adjustment")),
            })
        }
        Some((kind @ ("order" | "invoice"), id)) if out => {
            let value = issue_value(&mut *conn, pool, q).await?;
            sqlx::query!(
                "INSERT INTO inventory_cost_allocations (workspace_id, source_kind, source_id,
                     product_id, warehouse_id, issued_quantity, issued_value)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (workspace_id, source_kind, source_id, product_id, warehouse_id)
                 DO UPDATE SET issued_quantity = inventory_cost_allocations.issued_quantity + $6,
                               issued_value = inventory_cost_allocations.issued_value + $7,
                               version = inventory_cost_allocations.version + 1, updated_at = now()",
                ws,
                kind,
                id,
                row.product_id,
                row.warehouse_id,
                q,
                value
            )
            .execute(&mut *conn)
            .await?;
            Ok(Plan {
                class: "sale_issue",
                value: -value,
                roles: Some(("cogs", "inventory")),
            })
        }
        Some((kind @ ("order" | "invoice"), id)) => Ok(Plan {
            class: "customer_return",
            value: restore(&mut *conn, ws, (kind, id), row, q).await?,
            roles: Some(("inventory", "cogs")),
        }),
        Some(_) => Err(unsupported(UNKNOWN)),
    }
}

/// Value one ledger movement of `row` (`before` → `after`) in an active book: pool, value
/// entry, journal. The pool was locked by [`pool`] in this transaction. No-op in setup.
pub async fn on_movement(
    conn: &mut PgConnection,
    ws: Uuid,
    row: &StockRow,
    movement_id: Uuid,
    (before, after): (Decimal, Decimal),
    cause: &Cause<'_>,
) -> Result<(), AppError> {
    let delta = after - before;
    if delta.is_zero() || cause.value == Valued::Relocate || !active(conn, ws).await? {
        return Ok(());
    }
    let pool = lock_pool(conn, ws, row.product_id, row.warehouse_id)
        .await?
        .ok_or_else(|| AppError::Internal("stock valued without its pool".into()))?;
    // Clock time under the pool lock: serialized writers get non-decreasing dates.
    let (date, timezone_ok) = sqlx::query!(
        r#"SELECT (clock_timestamp() AT TIME ZONE coalesce(timezone, 'UTC'))::date AS "date!",
                  timezone IS NOT NULL AS "tz!"
           FROM accounting_settings WHERE workspace_id = $1"#,
        ws
    )
    .fetch_one(&mut *conn)
    .await
    .map(|r| (r.date, r.tz))?;
    if !timezone_ok {
        return Err(crate::modules::accounting::service::not_configured());
    }
    let date = match cause.value {
        Valued::Count { date, .. } => date,
        Valued::Receipt { line, date, .. } => {
            // Earliest eligible receipt first (masterplan §7): no older pending line left.
            if purchasing::repo::pending_before(&mut *conn, ws, line).await? {
                return Err(reason(RECEIPT_BOOKING, BOOK_EARLIER));
            }
            date
        }
        _ if purchasing::repo::pending(&mut *conn, ws, row.warehouse_id, Some(row.product_id))
            .await? =>
        {
            return Err(reason(RECEIPT_BOOKING, BOOK_FIRST));
        }
        _ => date,
    };
    if pool.last_date.is_some_and(|last| date < last) {
        return Err(reason(
            "backdated_valuation",
            "This stock was already valued on a later date.",
        ));
    }
    let plan = plan(conn, ws, row, &pool, delta, cause).await?;
    let quantity_after = pool.quantity + delta;
    if quantity_after < Decimal::ZERO {
        return Err(insufficient());
    }
    // §6 permanent identity: receipt_line/booked for bookings, else the movement.
    let (source_kind, source_id, effect_kind) = match cause.value {
        Valued::Receipt { line, .. } => ("purchase_receipt_line", line, "booked"),
        _ => ("stock_movement", movement_id, "valued"),
    };
    let journal = match plan.roles {
        Some((debit, credit)) if plan.value != 0 => {
            let fingerprint: [u8; 32] = Sha256::digest(
                json!({ "movement": movement_id, "class": plan.class, "value": plan.value })
                    .to_string(),
            )
            .into();
            Some(
                journals::post_source(
                    conn,
                    ws,
                    &SourceJournal {
                        source_kind,
                        source_id,
                        effect_kind,
                        date,
                        reason: plan.class,
                        debit: Side::Role(debit),
                        credit: Side::Role(credit),
                        relation_id: None,
                        bank_account_id: None,
                        amount: plan.value.abs(),
                        fingerprint: &fingerprint,
                        // shortcut: nil = system actor (till, jobs); actor_kind comes with M4.
                        by: cause.user_id.unwrap_or(Uuid::nil()),
                    },
                )
                .await?,
            )
        }
        _ => None,
    };
    // New pool value in SQL; the table CHECKs refuse a negative value or value without stock.
    let (sequence, value_after) = sqlx::query!(
        "UPDATE inventory_cost_pools
         SET quantity = $2, value = value + $3, last_accounting_date = $4,
             valuation_sequence = valuation_sequence + 1, version = version + 1, updated_at = now()
         WHERE id = $1 RETURNING valuation_sequence, value",
        pool.id,
        quantity_after,
        plan.value,
        date
    )
    .fetch_one(&mut *conn)
    .await
    .map(|r| (r.valuation_sequence, r.value))?;
    let (source_kind, source_id) = cause.related.unzip();
    sqlx::query!(
        "INSERT INTO inventory_value_entries (workspace_id, pool_id, sequence, movement_id,
             accounting_date, classification, source_kind, source_id, quantity_before,
             quantity_delta, quantity_after, value_before, value_delta, value_after,
             journal_entry_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        ws,
        pool.id,
        sequence,
        movement_id,
        date,
        plan.class,
        source_kind,
        source_id,
        pool.quantity,
        delta,
        quantity_after,
        pool.value,
        plan.value,
        value_after,
        journal
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}
