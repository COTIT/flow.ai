//! SQL for stocks and the stock_movements ledger. Every write runs in the caller's
//! transaction: lock the slot's row (or create it), change it, append one movement.

use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::money::round;

/// Where a stock row sits: product + warehouse, plus a bin (with its shelf) or none for the
/// warehouse-level row.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub bin: Option<Bin>,
}

/// A bin and the shelf it sits on.
#[derive(Clone, Copy, Debug)]
pub struct Bin {
    pub shelf_id: Uuid,
    pub bin_id: Uuid,
}

/// A stock row.
#[derive(Clone, Debug)]
pub struct StockRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub shelf_id: Option<Uuid>,
    pub bin_id: Option<Uuid>,
    pub stock_lvl: Decimal,
    pub reserved: Decimal,
    pub minimal_stock_level: i32,
    pub maximum_stock_level: i32,
}

/// Who and why, copied onto the movement.
#[derive(Default)]
pub struct Cause<'a> {
    pub reason: Option<&'a str>,
    pub is_correction: bool,
    pub user_id: Option<Uuid>,
    /// The record that caused it, e.g. `("order", id)`.
    pub related: Option<(&'a str, Uuid)>,
    pub meta: Option<&'a Value>,
}

/// How far a decrease may take the level.
// shortcut: None/Zero are for order flows (final batch F12); drop the allow when they land.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Floor {
    /// No floor: store warehouses (Laravel MutateStock).
    None,
    /// Level stays ≥ 0: other warehouses (Laravel MutateStock).
    Zero,
    /// Level − reserved stays ≥ 0 (Laravel EnoughStockRule on the HTTP route).
    Available,
}

/// Result of [`mutate`].
// shortcut: the HTTP route reads only the variant; order flows (F12) read the values.
#[allow(dead_code)]
#[derive(Debug)]
pub enum Mutated {
    Done {
        stock_id: Uuid,
        before: Decimal,
        after: Decimal,
    },
    /// Refused by the floor; nothing written. `level`/`reserved` are 0 when the slot has no row.
    Short { level: Decimal, reserved: Decimal },
}

/// One ledger row.
pub struct Movement<'a> {
    pub kind: &'static str,
    pub change: Option<Decimal>,
    pub before: Decimal,
    pub after: Decimal,
    pub min: (Option<i32>, Option<i32>),
    pub max: (Option<i32>, Option<i32>),
    pub reason: Option<&'a str>,
    pub cause: &'a Cause<'a>,
}

/// THE stock writer (HTTP routes and order flows): add `change` (rounded to the column's 2
/// places) to the slot's row under a row lock and append an increase/decrease movement.
/// Creates the row on first use; refuses past `floor` without writing anything.
pub async fn mutate(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    slot: Slot,
    change: Decimal,
    floor: Floor,
    cause: &Cause<'_>,
) -> Result<Mutated, sqlx::Error> {
    let change = round(change, 2);
    let existing = locked(conn, workspace_id, slot).await?;
    let (level, reserved) = existing
        .as_ref()
        .map_or((Decimal::ZERO, Decimal::ZERO), |r| {
            (r.stock_lvl, r.reserved)
        });
    let after = level + change;
    let refused = match floor {
        Floor::None => false,
        Floor::Zero => after < Decimal::ZERO,
        Floor::Available => after - reserved < Decimal::ZERO,
    };
    if refused && change < Decimal::ZERO {
        return Ok(Mutated::Short { level, reserved });
    }
    let row = match existing {
        Some(row) => row,
        None => lock_or_create(conn, workspace_id, slot).await?,
    };
    set_quantities(
        conn,
        row.id,
        after,
        row.minimal_stock_level,
        row.maximum_stock_level,
    )
    .await?;
    let kind = if change > Decimal::ZERO {
        "increase"
    } else {
        "decrease"
    };
    let movement = Movement {
        kind,
        change: Some(change),
        before: row.stock_lvl,
        after,
        min: (None, None),
        max: (None, None),
        reason: cause.reason,
        cause,
    };
    record(conn, workspace_id, &row, &movement).await?;
    Ok(Mutated::Done {
        stock_id: row.id,
        before: row.stock_lvl,
        after,
    })
}

/// The slot's live row under `FOR NO KEY UPDATE`; None = no row yet.
pub async fn locked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    slot: Slot,
) -> Result<Option<StockRow>, sqlx::Error> {
    // A waiter holds a pooled connection; give up instead of starving the pool.
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await?;
    sqlx::query_as!(
        StockRow,
        "SELECT id, product_id, warehouse_id, shelf_id, bin_id, stock_lvl, reserved,
                minimal_stock_level, maximum_stock_level
         FROM stocks
         WHERE workspace_id = $1 AND product_id = $2 AND warehouse_id = $3
           AND bin_id IS NOT DISTINCT FROM $4 AND deleted_at IS NULL
         FOR NO KEY UPDATE",
        workspace_id,
        slot.product_id,
        slot.warehouse_id,
        slot.bin.map(|b| b.bin_id),
    )
    .fetch_optional(&mut *conn)
    .await
}

/// The slot's row under a lock, created with zeros when missing. Two creators meet on
/// `stocks_slot_unique`: the loser's insert does nothing and its second read waits for and
/// locks the winner's row.
pub async fn lock_or_create(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    slot: Slot,
) -> Result<StockRow, sqlx::Error> {
    if let Some(row) = locked(conn, workspace_id, slot).await? {
        return Ok(row);
    }
    sqlx::query!(
        "INSERT INTO stocks (workspace_id, product_id, warehouse_id, shelf_id, bin_id)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (product_id, warehouse_id, bin_id) WHERE deleted_at IS NULL DO NOTHING",
        workspace_id,
        slot.product_id,
        slot.warehouse_id,
        slot.bin.map(|b| b.shelf_id),
        slot.bin.map(|b| b.bin_id),
    )
    .execute(&mut *conn)
    .await?;
    locked(conn, workspace_id, slot)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// Write level and requirements of a row the caller holds locked.
pub async fn set_quantities(
    conn: &mut PgConnection,
    id: Uuid,
    stock_lvl: Decimal,
    minimal: i32,
    maximum: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE stocks SET stock_lvl = $2, minimal_stock_level = $3, maximum_stock_level = $4,
                           updated_at = now()
         WHERE id = $1",
        id,
        stock_lvl,
        minimal,
        maximum,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Append one ledger row for `row` (its slot) in the caller's transaction.
pub async fn record(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    row: &StockRow,
    m: &Movement<'_>,
) -> Result<(), sqlx::Error> {
    let (related_domain, related_id) = m.cause.related.unzip();
    sqlx::query!(
        "INSERT INTO stock_movements (workspace_id, stock_id, product_id, warehouse_id, shelf_id,
             bin_id, movement_type, quantity_change, quantity_before, quantity_after,
             minimal_stock_level_before, minimal_stock_level_after, maximum_stock_level_before,
             maximum_stock_level_after, reason, is_correction, user_id, related_domain,
             related_id, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                 $18, $19, $20)",
        workspace_id,
        row.id,
        row.product_id,
        row.warehouse_id,
        row.shelf_id,
        row.bin_id,
        m.kind,
        m.change,
        m.before,
        m.after,
        m.min.0,
        m.min.1,
        m.max.0,
        m.max.1,
        m.reason,
        m.cause.is_correction,
        m.cause.user_id,
        related_domain,
        related_id,
        m.cause.meta,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// The live bin of this warehouse (its section, row and shelf live too), with its shelf.
pub async fn bin(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    bin_id: Uuid,
) -> Result<Option<Bin>, sqlx::Error> {
    sqlx::query_as!(
        Bin,
        "SELECT b.shelf_id, b.id AS bin_id
         FROM warehouse_section_row_bins b
         JOIN warehouse_section_row_shelves sh ON sh.id = b.shelf_id AND sh.deleted_at IS NULL
         JOIN warehouse_section_rows r ON r.id = b.row_id AND r.deleted_at IS NULL
         JOIN warehouse_sections s ON s.id = b.section_id AND s.deleted_at IS NULL
         WHERE b.id = $1 AND b.workspace_id = $2 AND b.warehouse_id = $3 AND b.deleted_at IS NULL",
        bin_id,
        workspace_id,
        warehouse_id,
    )
    .fetch_optional(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    /// A workspace with one product, a store and a plain warehouse; returns their ids.
    async fn fixture(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid) {
        let row: (Uuid, Uuid, Uuid, Uuid) = sqlx::query_as(
            "WITH u AS (INSERT INTO users (email, first_name, last_name, password_hash)
                        VALUES ('ada@example.com', 'Ada', 'L', 'x') RETURNING id),
                  w AS (INSERT INTO workspaces (name, created_by_user_id)
                        SELECT 'acme', id FROM u RETURNING id),
                  c AS (INSERT INTO currencies (workspace_id, name, asset_type)
                        SELECT id, 'Euro', 'EUR' FROM w RETURNING id, workspace_id),
                  p AS (INSERT INTO products (workspace_id, product_number, product_appendix,
                                              product_nr_formatted, name, currency_id)
                        SELECT workspace_id, 1, '{}', 'PRO-00001', 'A', id FROM c
                        RETURNING id, workspace_id),
                  s AS (INSERT INTO warehouses (workspace_id, name, slug, is_store)
                        SELECT id, 'Shop', 'shop', true FROM w RETURNING id),
                  m AS (INSERT INTO warehouses (workspace_id, name, slug)
                        SELECT id, 'Main', 'main' FROM w RETURNING id)
             SELECT p.workspace_id, p.id, s.id, m.id FROM p, s, m",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        row
    }

    #[sqlx::test]
    async fn floors_decide_how_far_a_decrease_goes(pool: PgPool) {
        let (ws, product, shop, main) = fixture(&pool).await;
        let slot = |warehouse_id| Slot {
            product_id: product,
            warehouse_id,
            bin: None,
        };
        let cause = Cause {
            reason: Some("order 7"),
            ..Cause::default()
        };
        let three = Decimal::from(3);
        let mut tx = pool.begin().await.unwrap();

        let r = mutate(&mut tx, ws, slot(shop), -three, Floor::None, &cause)
            .await
            .unwrap();
        assert!(
            matches!(r, Mutated::Done { after, .. } if after == -three),
            "{r:?}"
        );
        let r = mutate(&mut tx, ws, slot(main), -Decimal::ONE, Floor::Zero, &cause)
            .await
            .unwrap();
        assert!(
            matches!(r, Mutated::Short { level, .. } if level.is_zero()),
            "{r:?}"
        );
        mutate(&mut tx, ws, slot(main), three, Floor::Zero, &cause)
            .await
            .unwrap();
        let r = mutate(&mut tx, ws, slot(main), -three, Floor::Zero, &cause)
            .await
            .unwrap();
        assert!(
            matches!(r, Mutated::Done { before, after, .. }
            if before == three && after.is_zero()),
            "{r:?}"
        );
        tx.commit().await.unwrap();

        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT movement_type, quantity_before::text, quantity_after::text
             FROM stock_movements ORDER BY created_at",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let rows: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("decrease", "0.00", "-3.00"),
                ("increase", "0.00", "3.00"),
                ("decrease", "3.00", "0.00"),
            ],
            "a refused decrease writes nothing"
        );
    }
}
