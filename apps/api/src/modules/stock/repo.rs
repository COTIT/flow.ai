//! SQL for stocks and the stock_movements ledger. Every write runs in the caller's
//! transaction: lock the slot's row (or create it), change it, append one movement.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{modules::webhooks, money::round};

/// Where a stock row sits: product + warehouse, plus a bin (with its shelf) or none for the
/// warehouse-level row, plus a lot or none for the product's no-lot stock.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub bin: Option<Bin>,
    pub lot_id: Option<Uuid>,
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
    pub lot_id: Option<Uuid>,
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
    /// The new level would leave ±[`LEVEL_MAX`]; nothing written.
    OutOfRange,
}

/// Largest level a row may reach: 1e15 (the `numeric(19,2)` column holds 17 integer digits).
pub const LEVEL_MAX: Decimal = Decimal::from_parts(2_764_472_320, 232_830, 0, false, 0);

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
/// Creates the row on first use; refuses past `floor` or [`LEVEL_MAX`] without writing.
/// Callers skip a change that rounds to 0 (it would log an empty movement). A caller locking
/// several slots takes them through [`lock_all`] first.
pub async fn mutate(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    slot: Slot,
    change: Decimal,
    floor: Floor,
    cause: &Cause<'_>,
) -> Result<Mutated, sqlx::Error> {
    let change = round(change, 2);
    let row = match locked(conn, workspace_id, slot).await? {
        Some(row) => row,
        // Nothing to take from a slot without a row.
        None if change < Decimal::ZERO && floor != Floor::None => {
            return Ok(Mutated::Short {
                level: Decimal::ZERO,
                reserved: Decimal::ZERO,
            });
        }
        None => lock_or_create(conn, workspace_id, slot).await?.0,
    };
    // Always from the locked row: a concurrent first write may have created it meanwhile.
    let after = row.stock_lvl + change;
    if after.abs() > LEVEL_MAX {
        return Ok(Mutated::OutOfRange);
    }
    let refused = match floor {
        Floor::None => false,
        Floor::Zero => after < Decimal::ZERO,
        Floor::Available => after - row.reserved < Decimal::ZERO,
    };
    if refused && change < Decimal::ZERO {
        return Ok(Mutated::Short {
            level: row.stock_lvl,
            reserved: row.reserved,
        });
    }
    set_quantities(
        conn,
        workspace_id,
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

/// A stock row with its warehouse, shelf and bin (Laravel StockWithoutRelationsResource).
/// Each relation is None when soft-deleted, as Laravel's BelongsTo gave null.
pub struct StockLine {
    pub id: Uuid,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub stock_lvl: Decimal,
    pub reserved: Decimal,
    pub minimal_stock_level: i32,
    pub maximum_stock_level: i32,
    pub meta: Option<Value>,
    pub updated_at: DateTime<Utc>,
    pub w_id: Option<Uuid>,
    pub w_name: Option<String>,
    pub w_alias: Option<String>,
    pub w_is_store: Option<bool>,
    pub w_pickup: Option<bool>,
    pub w_location: Option<Value>,
    pub shelf_id: Option<Uuid>,
    pub shelf_code: Option<String>,
    pub shelf_name: Option<String>,
    pub bin_id: Option<Uuid>,
    pub bin_code: Option<String>,
    pub bin_name: Option<String>,
    pub lot_id: Option<Uuid>,
    pub lot_code: Option<String>,
    pub lot_expires_on: Option<NaiveDate>,
}

/// Live stock rows of `product_ids` with their relations, oldest first.
pub async fn lines(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    product_ids: &[Uuid],
) -> Result<Vec<StockLine>, sqlx::Error> {
    sqlx::query_as!(
        StockLine,
        r#"SELECT s.id, s.product_id, s.warehouse_id, s.stock_lvl, s.reserved,
                  s.minimal_stock_level, s.maximum_stock_level, s.meta, s.updated_at,
                  w.id AS "w_id?", w.name AS "w_name?", w.alias AS "w_alias?",
                  w.is_store AS "w_is_store?", w.pickup_enabled AS "w_pickup?",
                  w.location AS "w_location?",
                  sh.id AS "shelf_id?", sh.shelf_code AS "shelf_code?", sh.shelf_name AS "shelf_name?",
                  b.id AS "bin_id?", b.bin_code AS "bin_code?", b.bin_name AS "bin_name?",
                  l.id AS "lot_id?", l.code AS "lot_code?", l.expires_on AS "lot_expires_on?"
           FROM stocks s
           LEFT JOIN warehouses w ON w.id = s.warehouse_id AND w.deleted_at IS NULL
           LEFT JOIN warehouse_section_row_shelves sh ON sh.id = s.shelf_id AND sh.deleted_at IS NULL
           LEFT JOIN warehouse_section_row_bins b ON b.id = s.bin_id AND b.deleted_at IS NULL
           LEFT JOIN lots l ON l.id = s.lot_id
           WHERE s.workspace_id = $1 AND s.product_id = ANY($2) AND s.deleted_at IS NULL
           ORDER BY s.id"#,
        workspace_id,
        product_ids,
    )
    .fetch_all(db)
    .await
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
        "SELECT id, product_id, warehouse_id, shelf_id, bin_id, lot_id, stock_lvl, reserved,
                minimal_stock_level, maximum_stock_level
         FROM stocks
         WHERE workspace_id = $1 AND product_id = $2 AND warehouse_id = $3
           AND bin_id IS NOT DISTINCT FROM $4 AND lot_id IS NOT DISTINCT FROM $5
           AND deleted_at IS NULL
         FOR NO KEY UPDATE",
        workspace_id,
        slot.product_id,
        slot.warehouse_id,
        slot.bin.map(|b| b.bin_id),
        slot.lot_id,
    )
    .fetch_optional(&mut *conn)
    .await
}

/// The slot's row under a lock, created with zeros when missing; `true` = this call created
/// it. Two creators meet on `stocks_slot_unique`: the loser's insert does nothing and its
/// second read waits for and locks the winner's row.
pub async fn lock_or_create(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    slot: Slot,
) -> Result<(StockRow, bool), sqlx::Error> {
    if let Some(row) = locked(conn, workspace_id, slot).await? {
        return Ok((row, false));
    }
    let created = sqlx::query_scalar!(
        "INSERT INTO stocks (workspace_id, product_id, warehouse_id, shelf_id, bin_id, lot_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (product_id, warehouse_id, bin_id, lot_id) WHERE deleted_at IS NULL
         DO NOTHING
         RETURNING id",
        workspace_id,
        slot.product_id,
        slot.warehouse_id,
        slot.bin.map(|b| b.shelf_id),
        slot.bin.map(|b| b.bin_id),
        slot.lot_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .is_some();
    match locked(conn, workspace_id, slot).await? {
        Some(row) => Ok((row, created)),
        // The row it met was soft-deleted and committed in between: try once more.
        None => Box::pin(lock_or_create(conn, workspace_id, slot)).await,
    }
}

/// Laravel EnsureProductWarehouseStockEntries for a new product: zero warehouse-level rows in
/// the main (default) warehouse and every store, only when a main warehouse exists. No
/// movements, as in Laravel. Returns the main warehouse.
pub async fn seed(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"WITH targets AS (
               SELECT id, default_warehouse FROM warehouses
               WHERE workspace_id = $1 AND deleted_at IS NULL AND (default_warehouse OR is_store)
           ),
           seeded AS (
               INSERT INTO stocks (workspace_id, product_id, warehouse_id)
               SELECT $1, $2, id FROM targets
               WHERE EXISTS (SELECT 1 FROM targets WHERE default_warehouse)
               ON CONFLICT DO NOTHING
           )
           SELECT id AS "id!" FROM targets WHERE default_warehouse"#,
        workspace_id,
        product_id,
    )
    .fetch_optional(&mut *conn)
    .await
}

/// Most stock rows one sweep chunk inserts: with many stores a chunk takes fewer products.
const SEED_ROWS: i64 = 50_000;

/// One chunk of the warehouse sweep (Laravel EnsureProductWarehouseStockEntries): zero
/// warehouse-level rows in the main warehouse and every store for up to `limit` live
/// stock-tracked products after `after` (id order, fewer when there are many stores), and the
/// main warehouse as the default of those that have none (a chosen default stays, as in
/// `seed`). Returns the chunk's last product and size; None when none is left or there is no
/// main warehouse (Laravel skipped the workspace). Lock order: products by id, then slots.
pub async fn seed_chunk(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<Option<(Uuid, i64)>, sqlx::Error> {
    let t = sqlx::query!(
        r#"SELECT count(*) FILTER (WHERE default_warehouse) AS "mains!", count(*) AS "targets!"
           FROM warehouses
           WHERE workspace_id = $1 AND deleted_at IS NULL AND (default_warehouse OR is_store)"#,
        workspace_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    if t.mains == 0 {
        return Ok(None);
    }
    let limit = limit.min((SEED_ROWS / t.targets).max(1));
    // coalesce, not `$2 IS NULL OR`: a cached generic plan keeps the index bound.
    let ids = sqlx::query_scalar!(
        "SELECT id FROM products
         WHERE workspace_id = $1 AND has_stock AND deleted_at IS NULL
           AND id > coalesce($2, '00000000-0000-0000-0000-000000000000'::uuid)
         ORDER BY id LIMIT $3",
        workspace_id,
        after,
        limit,
    )
    .fetch_all(&mut *conn)
    .await?;
    let Some(&last) = ids.last() else {
        return Ok(None);
    };
    // Main read in the same statement, so a default moving meanwhile can't be half applied.
    sqlx::query!(
        "UPDATE products SET updated_at = now(),
             default_warehouse_id = (SELECT id FROM warehouses
                                     WHERE workspace_id = $1 AND default_warehouse
                                       AND deleted_at IS NULL)
         WHERE id IN (SELECT id FROM products
                      WHERE workspace_id = $1 AND id = ANY($2) AND default_warehouse_id IS NULL
                      ORDER BY id
                      FOR NO KEY UPDATE)",
        workspace_id,
        &ids,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "INSERT INTO stocks (workspace_id, product_id, warehouse_id)
         SELECT $1, p.id, w.id
         FROM unnest($2::uuid[]) AS p(id)
         CROSS JOIN (SELECT id FROM warehouses
                     WHERE workspace_id = $1 AND deleted_at IS NULL
                       AND (default_warehouse OR is_store)) AS w
         ORDER BY p.id, w.id
         ON CONFLICT DO NOTHING",
        workspace_id,
        &ids,
    )
    .execute(&mut *conn)
    .await?;
    Ok(Some((last, i64::try_from(ids.len()).unwrap_or(i64::MAX))))
}

/// Lock (creating when missing) every slot in [`SlotKey`] order, so writers touching the same
/// slots queue instead of deadlocking. Returns the slots this call created.
pub async fn lock_all(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    slots: &[Slot],
) -> Result<Vec<SlotKey>, sqlx::Error> {
    let mut keyed: Vec<(SlotKey, Slot)> = slots.iter().map(|s| (key(s), *s)).collect();
    keyed.sort_by_key(|k| k.0);
    keyed.dedup_by_key(|k| k.0);
    let mut created = Vec::new();
    for (k, slot) in keyed {
        if lock_or_create(conn, workspace_id, slot).await?.1 {
            created.push(k);
        }
        // The lot rows sort right after the no-lot warehouse row: still key order.
        if slot.bin.is_none() && slot.lot_id.is_none() {
            lot_rows(conn, workspace_id, slot.product_id, slot.warehouse_id).await?;
        }
    }
    Ok(created)
}

/// The product's live warehouse-level lot rows (no bin) at the warehouse, locked in
/// [`SlotKey`] order (by lot).
pub async fn lot_rows(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
    warehouse_id: Uuid,
) -> Result<Vec<StockRow>, sqlx::Error> {
    sqlx::query_as!(
        StockRow,
        "SELECT id, product_id, warehouse_id, shelf_id, bin_id, lot_id, stock_lvl, reserved,
                minimal_stock_level, maximum_stock_level
         FROM stocks
         WHERE workspace_id = $1 AND product_id = $2 AND warehouse_id = $3
           AND bin_id IS NULL AND lot_id IS NOT NULL AND deleted_at IS NULL
         ORDER BY lot_id
         FOR NO KEY UPDATE",
        workspace_id,
        product_id,
        warehouse_id,
    )
    .fetch_all(conn)
    .await
}

/// A slot's identity and lock order: product, warehouse, bin (the warehouse row first), then
/// lot (the no-lot row first). Rust's `Option` and Postgres' uuid order agree (bytewise).
pub type SlotKey = (Uuid, Uuid, Option<Uuid>, Option<Uuid>);

/// The [`SlotKey`] of `slot`.
pub fn key(slot: &Slot) -> SlotKey {
    (
        slot.product_id,
        slot.warehouse_id,
        slot.bin.map(|b| b.bin_id),
        slot.lot_id,
    )
}

/// Soft-delete a row the caller holds locked.
pub async fn soft_delete(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE stocks SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2",
        id,
        workspace_id,
    )
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

/// Write level and requirements of a row the caller holds locked.
pub async fn set_quantities(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    stock_lvl: Decimal,
    minimal: i32,
    maximum: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE stocks SET stock_lvl = $3, minimal_stock_level = $4, maximum_stock_level = $5,
                           updated_at = now()
         WHERE id = $1 AND workspace_id = $2",
        id,
        workspace_id,
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
             related_id, meta, lot_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                 $18, $19, $20, $21)",
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
        row.lot_id,
    )
    .execute(&mut *conn)
    .await?;
    // Requirements-only movements leave the level alone: no event.
    if m.before == m.after {
        return Ok(());
    }
    let payload = serde_json::json!({
        "stock_id": row.id,
        "product_id": row.product_id,
        "warehouse_id": row.warehouse_id,
        "lot_id": row.lot_id,
        "reason": m.kind,
        "quantity_change": format!("{:.2}", m.after - m.before),
        "quantity_after": format!("{:.2}", m.after),
    });
    webhooks::events::emit(conn, workspace_id, webhooks::events::STOCK_CHANGED, payload).await
}

/// The live warehouses of the workspace among `ids`.
pub async fn live_warehouses(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM warehouses WHERE id = ANY($1) AND workspace_id = $2 AND deleted_at IS NULL",
        ids,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// A live bin (live section, row and shelf) with its warehouse and shelf.
pub struct LiveBin {
    pub bin_id: Uuid,
    pub warehouse_id: Uuid,
    pub shelf_id: Uuid,
}

/// The live bins of the workspace among `ids`.
pub async fn live_bins(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<LiveBin>, sqlx::Error> {
    sqlx::query_as!(
        LiveBin,
        "SELECT b.id AS bin_id, b.warehouse_id, b.shelf_id
         FROM warehouse_section_row_bins b
         JOIN warehouse_section_row_shelves sh ON sh.id = b.shelf_id AND sh.deleted_at IS NULL
         JOIN warehouse_section_rows r ON r.id = b.row_id AND r.deleted_at IS NULL
         JOIN warehouse_sections s ON s.id = b.section_id AND s.deleted_at IS NULL
         WHERE b.id = ANY($1) AND b.workspace_id = $2 AND b.deleted_at IS NULL",
        ids,
        workspace_id,
    )
    .fetch_all(db)
    .await
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

/// A stock row with its warehouse, shelf (with row and section) and bin, for
/// `stock-with-bins`. Each relation is None when soft-deleted.
pub struct BinsRow {
    pub id: Uuid,
    pub stock_lvl: Decimal,
    pub reserved: Decimal,
    pub minimal_stock_level: i32,
    pub maximum_stock_level: i32,
    pub w_id: Option<Uuid>,
    pub w_name: Option<String>,
    pub sh_id: Option<Uuid>,
    pub sh_code: Option<String>,
    pub sh_name: Option<String>,
    pub r_id: Option<Uuid>,
    pub r_code: Option<String>,
    pub r_name: Option<String>,
    pub sec_id: Option<Uuid>,
    pub sec_code: Option<String>,
    pub sec_name: Option<String>,
    pub b_id: Option<Uuid>,
    pub b_code: Option<String>,
    pub b_name: Option<String>,
    pub lot_id: Option<Uuid>,
    pub lot_code: Option<String>,
    pub lot_expires_on: Option<NaiveDate>,
}

/// Live stock rows of the product with their place, oldest first.
pub async fn with_bins(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<Vec<BinsRow>, sqlx::Error> {
    sqlx::query_as!(
        BinsRow,
        r#"SELECT s.id, s.stock_lvl, s.reserved, s.minimal_stock_level, s.maximum_stock_level,
                  w.id AS "w_id?", w.name AS "w_name?",
                  sh.id AS "sh_id?", sh.shelf_code AS "sh_code?", sh.shelf_name AS "sh_name?",
                  r.id AS "r_id?", r.row_code AS "r_code?", r.row_name AS "r_name?",
                  sec.id AS "sec_id?", sec.section_code AS "sec_code?",
                  sec.section_name AS "sec_name?",
                  b.id AS "b_id?", b.bin_code AS "b_code?", b.bin_name AS "b_name?",
                  l.id AS "lot_id?", l.code AS "lot_code?", l.expires_on AS "lot_expires_on?"
           FROM stocks s
           LEFT JOIN warehouses w ON w.id = s.warehouse_id AND w.deleted_at IS NULL
           LEFT JOIN warehouse_section_row_shelves sh ON sh.id = s.shelf_id AND sh.deleted_at IS NULL
           LEFT JOIN warehouse_section_rows r ON r.id = sh.row_id AND r.deleted_at IS NULL
           LEFT JOIN warehouse_sections sec ON sec.id = r.section_id AND sec.deleted_at IS NULL
           LEFT JOIN warehouse_section_row_bins b ON b.id = s.bin_id AND b.deleted_at IS NULL
           LEFT JOIN lots l ON l.id = s.lot_id
           WHERE s.workspace_id = $1 AND s.product_id = $2 AND s.deleted_at IS NULL
           ORDER BY s.id"#,
        workspace_id,
        product_id,
    )
    .fetch_all(db)
    .await
}

/// A live bin holding a live stock row of the product, with its warehouse and place.
pub struct ConfiguredBin {
    pub id: Uuid,
    pub bin_code: String,
    pub bin_name: Option<String>,
    pub warehouse_id: Uuid,
    pub w_id: Option<Uuid>,
    pub sec_id: Option<Uuid>,
    pub sec_code: Option<String>,
    pub sec_name: Option<String>,
    pub r_id: Option<Uuid>,
    pub r_code: Option<String>,
    pub r_name: Option<String>,
    pub sh_id: Option<Uuid>,
    pub sh_code: Option<String>,
    pub sh_name: Option<String>,
}

/// The product's configured bins (Laravel ListProductConfiguredBins), oldest bin first.
pub async fn configured_bins(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<Vec<ConfiguredBin>, sqlx::Error> {
    sqlx::query_as!(
        ConfiguredBin,
        r#"SELECT b.id, b.bin_code, b.bin_name, b.warehouse_id, w.id AS "w_id?",
                  sec.id AS "sec_id?", sec.section_code AS "sec_code?",
                  sec.section_name AS "sec_name?",
                  r.id AS "r_id?", r.row_code AS "r_code?", r.row_name AS "r_name?",
                  sh.id AS "sh_id?", sh.shelf_code AS "sh_code?", sh.shelf_name AS "sh_name?"
           FROM warehouse_section_row_bins b
           LEFT JOIN warehouses w ON w.id = b.warehouse_id AND w.deleted_at IS NULL
           LEFT JOIN warehouse_sections sec ON sec.id = b.section_id AND sec.deleted_at IS NULL
           LEFT JOIN warehouse_section_rows r ON r.id = b.row_id AND r.deleted_at IS NULL
           LEFT JOIN warehouse_section_row_shelves sh ON sh.id = b.shelf_id AND sh.deleted_at IS NULL
           WHERE b.workspace_id = $1 AND b.deleted_at IS NULL
             AND b.id IN (SELECT bin_id FROM stocks
                          WHERE workspace_id = $1 AND product_id = $2 AND deleted_at IS NULL
                            AND bin_id IS NOT NULL)
           ORDER BY b.id"#,
        workspace_id,
        product_id,
    )
    .fetch_all(db)
    .await
}

/// Movement list filter: each id narrows independently.
pub struct MovementFilter {
    pub warehouse_id: Option<Uuid>,
    pub shelf_id: Option<Uuid>,
    pub bin_id: Option<Uuid>,
    /// Movement types to keep; None = all.
    pub types: Option<Vec<String>>,
    pub limit: i64,
    pub offset: i64,
}

/// A ledger row with its place, user and stock row (Laravel StockMovementResource).
pub struct MovementRow {
    pub id: Uuid,
    pub movement_type: String,
    pub quantity_change: Option<Decimal>,
    pub quantity_before: Option<Decimal>,
    pub quantity_after: Option<Decimal>,
    pub minimal_stock_level_before: Option<i32>,
    pub minimal_stock_level_after: Option<i32>,
    pub maximum_stock_level_before: Option<i32>,
    pub maximum_stock_level_after: Option<i32>,
    pub reason: Option<String>,
    pub is_correction: bool,
    pub related_domain: Option<String>,
    pub related_id: Option<Uuid>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub w_id: Option<Uuid>,
    pub w_name: Option<String>,
    pub sh_id: Option<Uuid>,
    pub sh_code: Option<String>,
    pub sh_name: Option<String>,
    pub r_code: Option<String>,
    pub r_name: Option<String>,
    pub sec_code: Option<String>,
    pub sec_name: Option<String>,
    pub b_id: Option<Uuid>,
    pub b_code: Option<String>,
    pub b_name: Option<String>,
    pub u_id: Option<Uuid>,
    pub u_first: Option<String>,
    pub u_middle: Option<String>,
    pub u_last: Option<String>,
    pub u_email: Option<String>,
    pub st_id: Option<Uuid>,
    pub st_product: Option<Uuid>,
    pub st_warehouse: Option<Uuid>,
    pub st_shelf: Option<Uuid>,
    pub st_bin: Option<Uuid>,
    pub st_lvl: Option<Decimal>,
    pub st_reserved: Option<Decimal>,
    pub st_max: Option<i32>,
    pub st_min: Option<i32>,
    pub st_meta: Option<Value>,
    pub st_created: Option<DateTime<Utc>>,
    pub st_updated: Option<DateTime<Utc>>,
}

/// One page of the product's movements, newest first. The page is cut before the joins.
pub async fn movements(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
    f: &MovementFilter,
) -> Result<Vec<MovementRow>, sqlx::Error> {
    sqlx::query_as!(
        MovementRow,
        r#"SELECT m.id, m.movement_type, m.quantity_change, m.quantity_before, m.quantity_after,
                  m.minimal_stock_level_before, m.minimal_stock_level_after,
                  m.maximum_stock_level_before, m.maximum_stock_level_after, m.reason,
                  m.is_correction, m.related_domain, m.related_id, m.meta, m.created_at,
                  w.id AS "w_id?", w.name AS "w_name?",
                  sh.id AS "sh_id?", sh.shelf_code AS "sh_code?", sh.shelf_name AS "sh_name?",
                  r.row_code AS "r_code?", r.row_name AS "r_name?",
                  sec.section_code AS "sec_code?", sec.section_name AS "sec_name?",
                  b.id AS "b_id?", b.bin_code AS "b_code?", b.bin_name AS "b_name?",
                  u.id AS "u_id?", u.first_name AS "u_first?", u.middle_name AS "u_middle?",
                  u.last_name AS "u_last?", u.email AS "u_email?",
                  st.id AS "st_id?", st.product_id AS "st_product?",
                  st.warehouse_id AS "st_warehouse?", st.shelf_id AS "st_shelf?",
                  st.bin_id AS "st_bin?", st.stock_lvl AS "st_lvl?", st.reserved AS "st_reserved?",
                  st.maximum_stock_level AS "st_max?", st.minimal_stock_level AS "st_min?",
                  st.meta AS "st_meta?", st.created_at AS "st_created?",
                  st.updated_at AS "st_updated?"
           FROM (SELECT * FROM stock_movements
                 WHERE workspace_id = $1 AND product_id = $2
                   AND ($3::uuid IS NULL OR warehouse_id = $3)
                   AND ($4::uuid IS NULL OR shelf_id = $4)
                   AND ($5::uuid IS NULL OR bin_id = $5)
                   AND ($8::text[] IS NULL OR movement_type = ANY($8))
                 ORDER BY created_at DESC, id DESC
                 LIMIT $6 OFFSET $7) m
           LEFT JOIN warehouses w ON w.id = m.warehouse_id AND w.deleted_at IS NULL
           LEFT JOIN warehouse_section_row_shelves sh ON sh.id = m.shelf_id AND sh.deleted_at IS NULL
           LEFT JOIN warehouse_section_rows r ON r.id = sh.row_id AND r.deleted_at IS NULL
           LEFT JOIN warehouse_sections sec ON sec.id = r.section_id AND sec.deleted_at IS NULL
           LEFT JOIN warehouse_section_row_bins b ON b.id = m.bin_id AND b.deleted_at IS NULL
           LEFT JOIN users u ON u.id = m.user_id
           LEFT JOIN stocks st ON st.id = m.stock_id AND st.deleted_at IS NULL
           ORDER BY m.created_at DESC, m.id DESC"#,
        workspace_id,
        product_id,
        f.warehouse_id,
        f.shelf_id,
        f.bin_id,
        f.limit,
        f.offset,
        f.types.as_deref(),
    )
    .fetch_all(db)
    .await
}

/// Movements matching the filter.
pub async fn movements_count(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    product_id: Uuid,
    f: &MovementFilter,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM stock_movements m
           WHERE m.workspace_id = $1 AND m.product_id = $2
             AND ($3::uuid IS NULL OR m.warehouse_id = $3)
             AND ($4::uuid IS NULL OR m.shelf_id = $4)
             AND ($5::uuid IS NULL OR m.bin_id = $5)
             AND ($6::text[] IS NULL OR m.movement_type = ANY($6))"#,
        workspace_id,
        product_id,
        f.warehouse_id,
        f.shelf_id,
        f.bin_id,
        f.types.as_deref(),
    )
    .fetch_one(db)
    .await
}

/// A stock row of a list page, with the total in every row.
pub struct ListRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub stock_lvl: Decimal,
    pub minimal_stock_level: i32,
    pub maximum_stock_level: i32,
    pub meta: Option<Value>,
    pub lot_id: Option<Uuid>,
    pub lot_code: Option<String>,
    pub lot_expires_on: Option<NaiveDate>,
    pub total: i64,
}

/// Stock list filter: `negative` = store rows below 0 (Laravel ListNegativeStock), lowest
/// first, with `pattern` (escaped `%q%`) on product codes/name and warehouse name/alias.
/// Rows of deleted products or warehouses are left out.
pub struct ListFilter<'a> {
    pub negative: bool,
    pub pattern: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

/// One page of stock rows: insertion order, or lowest level first when `negative`. Two
/// statements, so each sorts on its own index.
pub async fn list(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<Vec<ListRow>, sqlx::Error> {
    if !f.negative {
        return sqlx::query_as!(
            ListRow,
            r#"SELECT s.id, s.product_id, s.warehouse_id, s.stock_lvl, s.minimal_stock_level,
                      s.maximum_stock_level, s.meta, l.id AS "lot_id?", l.code AS "lot_code?", l.expires_on AS "lot_expires_on?",
                      count(*) OVER () AS "total!"
               FROM stocks s
               JOIN products p ON p.id = s.product_id AND p.deleted_at IS NULL
               JOIN warehouses w ON w.id = s.warehouse_id AND w.deleted_at IS NULL
               LEFT JOIN lots l ON l.id = s.lot_id
               WHERE s.workspace_id = $1 AND s.deleted_at IS NULL
               ORDER BY s.id
               LIMIT $2 OFFSET $3"#,
            workspace_id,
            f.limit,
            f.offset,
        )
        .fetch_all(db)
        .await;
    }
    sqlx::query_as!(
        ListRow,
        r#"SELECT s.id, s.product_id, s.warehouse_id, s.stock_lvl, s.minimal_stock_level,
                  s.maximum_stock_level, s.meta, l.id AS "lot_id?", l.code AS "lot_code?", l.expires_on AS "lot_expires_on?",
                  count(*) OVER () AS "total!"
           FROM stocks s
           JOIN products p ON p.id = s.product_id AND p.deleted_at IS NULL
           JOIN warehouses w ON w.id = s.warehouse_id AND w.deleted_at IS NULL
           LEFT JOIN lots l ON l.id = s.lot_id
           WHERE s.workspace_id = $1 AND s.deleted_at IS NULL AND s.stock_lvl < 0 AND w.is_store
             AND ($2::text IS NULL OR p.name ILIKE $2 OR p.reference ILIKE $2
                  OR p.reference_vendor ILIKE $2 OR p.product_nr_formatted ILIKE $2
                  OR p.ean13 ILIKE $2 OR w.name ILIKE $2 OR w.alias ILIKE $2)
           ORDER BY s.stock_lvl, s.id
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        f.pattern,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await
}

/// Rows matching the filter (for a page past the end); one statement per list, as `list`.
pub async fn count(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<i64, sqlx::Error> {
    if !f.negative {
        return sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!"
               FROM stocks s
               JOIN products p ON p.id = s.product_id AND p.deleted_at IS NULL
               JOIN warehouses w ON w.id = s.warehouse_id AND w.deleted_at IS NULL
               WHERE s.workspace_id = $1 AND s.deleted_at IS NULL"#,
            workspace_id,
        )
        .fetch_one(db)
        .await;
    }
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!"
           FROM stocks s
           JOIN products p ON p.id = s.product_id AND p.deleted_at IS NULL
           JOIN warehouses w ON w.id = s.warehouse_id AND w.deleted_at IS NULL
           WHERE s.workspace_id = $1 AND s.deleted_at IS NULL AND s.stock_lvl < 0 AND w.is_store
             AND ($2::text IS NULL OR p.name ILIKE $2 OR p.reference ILIKE $2
                  OR p.reference_vendor ILIKE $2 OR p.product_nr_formatted ILIKE $2
                  OR p.ean13 ILIKE $2 OR w.name ILIKE $2 OR w.alias ILIKE $2)"#,
        workspace_id,
        f.pattern,
    )
    .fetch_one(db)
    .await
}

/// Occupied bins per warehouse: live bins holding stock with level − reserved > 0. Counted
/// from the bins side, so the cost follows the bins, not products × bins. (Laravel's
/// withCount lost its DISTINCT and counted stock rows.)
pub async fn occupied_by_warehouse(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    warehouse_ids: &[Uuid],
) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT b.warehouse_id, count(*) AS "n!"
           FROM warehouse_section_row_bins b
           WHERE b.workspace_id = $1 AND b.warehouse_id = ANY($2) AND b.deleted_at IS NULL
             AND EXISTS (SELECT 1 FROM stocks s
                         WHERE s.bin_id = b.id AND s.deleted_at IS NULL
                           AND s.stock_lvl - s.reserved > 0)
           GROUP BY b.warehouse_id"#,
        workspace_id,
        warehouse_ids,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.warehouse_id, r.n)).collect())
}

/// Occupied bins per section row, same rule.
pub async fn occupied_by_row(
    db: impl sqlx::PgExecutor<'_>,
    workspace_id: Uuid,
    row_ids: &[Uuid],
) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT b.row_id, count(*) AS "n!"
           FROM warehouse_section_row_bins b
           WHERE b.workspace_id = $1 AND b.row_id = ANY($2) AND b.deleted_at IS NULL
             AND EXISTS (SELECT 1 FROM stocks s
                         WHERE s.bin_id = b.id AND s.deleted_at IS NULL
                           AND s.stock_lvl - s.reserved > 0)
           GROUP BY b.row_id"#,
        workspace_id,
        row_ids,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.row_id, r.n)).collect())
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
            lot_id: None,
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
