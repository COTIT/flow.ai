//! Moving-average inventory valuation (Flow ERP M2, masterplan §7/§10). Active books only:
//! one cost pool per product/warehouse, locked before its stock rows; every level change gets
//! an immutable value entry and, when value crosses inventory ↔ P&L, one source journal.
//! Provenance the stock layer can't identify is refused; the caller's transaction rolls back.

use chrono::NaiveDate;
use rust_decimal::{Decimal, prelude::ToPrimitive};
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
        },
        stock::repo::{Cause, StockRow},
    },
    money::round,
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

/// Lock the product/warehouse pool of an active book, created when missing. Callers hold the
/// gate ([`active`]) and run it before the stock row lock, so pools lock in slot-key order.
pub async fn pool(
    conn: &mut PgConnection,
    ws: Uuid,
    product: Uuid,
    warehouse: Uuid,
) -> Result<(), AppError> {
    if lock_pool(conn, ws, product, warehouse).await?.is_some() {
        return Ok(());
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
    Ok(())
}

/// round(v × q / of) in minor units, half away from zero.
fn share(v: i64, q: Decimal, of: Decimal) -> Option<i64> {
    round(Decimal::from(v).checked_mul(q)?.checked_div(of)?, 0).to_i64()
}

/// Value an issue of `q` takes from pool (Q, V): all of V when it empties the pool.
fn issue_value(quantity: Decimal, value: i64, q: Decimal) -> Result<i64, AppError> {
    if q > quantity {
        return Err(insufficient());
    }
    if q == quantity {
        return Ok(value);
    }
    share(value, q, quantity).ok_or_else(|| AppError::Internal("issue value overflow".into()))
}

/// Value a return of `q` restores from an allocation (issued Iq/Iv, returned Rq/Rv):
/// cumulative rounding, the final unit takes the exact remainder.
fn restore_value((iq, iv): (Decimal, i64), (rq, rv): (Decimal, i64), q: Decimal) -> Option<i64> {
    let total = rq + q;
    if total > iq {
        return None;
    }
    if total == iq {
        return Some(iv - rv);
    }
    Some(share(iv, total, iq)? - rv)
}

/// What a movement is and what it does to the pool.
struct Plan {
    class: &'static str,
    value: i64,
    /// (debit role, credit role) of the journal; None = inventory → inventory.
    roles: Option<(&'static str, &'static str)>,
}

const LEGACY_RETURN: &str =
    "This sale has no recorded cost in the book. Its return needs a reviewed cost.";

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
        Valued::Transfer => Some(("transfer_out", "transfer_in")),
        Valued::Pack => Some(("pack_out", "pack_in")),
        Valued::Relocate | Valued::Infer => None,
    };
    if let Some((out_class, in_class)) = pair {
        let (kind, id) = cause.related.ok_or_else(|| unsupported(UNKNOWN))?;
        if out {
            let value = issue_value(pool.quantity, pool.value, q)?;
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
            value: -issue_value(pool.quantity, pool.value, q)?,
            roles: Some(("inventory_adjustment", "inventory")),
        }),
        None => {
            if pool.quantity.is_zero() {
                return Err(unsupported(EMPTY_POOL));
            }
            let value = share(pool.value, q, pool.quantity)
                .ok_or_else(|| AppError::Internal("adjustment value overflow".into()))?;
            Ok(Plan {
                class: "adjustment",
                value,
                roles: Some(("inventory", "inventory_adjustment")),
            })
        }
        Some((kind @ ("order" | "invoice"), id)) if out => {
            let value = issue_value(pool.quantity, pool.value, q)?;
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
        Some((kind @ ("order" | "invoice"), id)) => {
            let a = sqlx::query!(
                "SELECT id, issued_quantity, issued_value, returned_quantity, returned_value
                 FROM inventory_cost_allocations
                 WHERE workspace_id = $1 AND source_kind = $2 AND source_id = $3
                   AND product_id = $4 AND warehouse_id = $5
                 FOR UPDATE",
                ws,
                kind,
                id,
                row.product_id,
                row.warehouse_id
            )
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| reason("legacy_return_cost_required", LEGACY_RETURN))?;
            let value = restore_value(
                (a.issued_quantity, a.issued_value),
                (a.returned_quantity, a.returned_value),
                q,
            )
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
            Ok(Plan {
                class: "customer_return",
                value,
                roles: Some(("inventory", "cogs")),
            })
        }
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
    if pool.last_date.is_some_and(|last| date < last) {
        return Err(reason(
            "backdated_valuation",
            "This stock was already valued on a later date.",
        ));
    }
    let plan = plan(conn, ws, row, &pool, delta, cause).await?;
    let quantity_after = pool.quantity + delta;
    let value_after = pool.value + plan.value;
    if quantity_after < Decimal::ZERO {
        return Err(insufficient());
    }
    if value_after < 0 || (quantity_after.is_zero() && value_after != 0) {
        return Err(AppError::Internal("inconsistent inventory pool".into()));
    }
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
                        source_kind: "stock_movement",
                        source_id: movement_id,
                        effect_kind: "valued",
                        date,
                        reason: plan.class,
                        debit,
                        credit,
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
    let sequence = sqlx::query_scalar!(
        "UPDATE inventory_cost_pools
         SET quantity = $2, value = $3, last_accounting_date = $4,
             valuation_sequence = valuation_sequence + 1, version = version + 1, updated_at = now()
         WHERE id = $1 RETURNING valuation_sequence",
        pool.id,
        quantity_after,
        value_after,
        date
    )
    .fetch_one(&mut *conn)
    .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn issue_takes_the_average_and_empties_exactly() {
        assert_eq!(issue_value(d("20"), 24000, d("5")).unwrap(), 6000);
        // Q3/V1 three times: 0, 1 (half away from zero), 0.
        assert_eq!(issue_value(d("3"), 1, d("1")).unwrap(), 0);
        assert_eq!(issue_value(d("2"), 1, d("1")).unwrap(), 1);
        assert_eq!(issue_value(d("1"), 0, d("1")).unwrap(), 0);
        assert_eq!(issue_value(d("7.5"), 7500, d("7.5")).unwrap(), 7500);
        assert!(issue_value(d("1"), 100, d("2")).is_err());
    }

    #[test]
    fn returns_restore_original_cost_with_an_exact_final_part() {
        assert_eq!(
            restore_value((d("5"), 6000), (d("0"), 0), d("2")),
            Some(2400)
        );
        // 3 units for 100: 33, 33, then the rest 34.
        assert_eq!(restore_value((d("3"), 100), (d("0"), 0), d("1")), Some(33));
        assert_eq!(restore_value((d("3"), 100), (d("1"), 33), d("1")), Some(34));
        assert_eq!(restore_value((d("3"), 100), (d("2"), 67), d("1")), Some(33));
        assert_eq!(
            restore_value((d("3"), 100), (d("2"), 67), d("2")),
            None,
            "cap"
        );
    }
}
