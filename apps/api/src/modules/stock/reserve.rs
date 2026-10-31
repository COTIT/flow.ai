//! Order stock seam: reserve, release, commit (take) and restock what an order holds. Holds
//! live in `stock_reservations`, one row per stock row picked; `stocks.reserved` is their sum
//! and only this file writes either. Every call locks its stock rows in product order, so
//! callers holding their order row lock never deadlock each other. Lots (phase 3) replace
//! `pick` only: callers name products and quantities.

use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    error::AppError,
    modules::{
        stock::repo::{self, Cause, Floor, Movement, Slot, StockRow},
        webhooks,
    },
};

/// A quantity of a product an order holds.
#[derive(Clone, Copy, Debug)]
pub struct Want {
    pub product_id: Uuid,
    pub quantity: Decimal,
}

/// Result of [`reserve`].
#[derive(Debug, PartialEq, Eq)]
pub enum Reserved {
    Done,
    /// `wants[index]` asked more than `available` (level − reserved); nothing written.
    Short {
        index: usize,
        available: Decimal,
    },
}

/// Why a hold change was refused.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    /// Reserve, release and commit need `cause.related`.
    #[error("stock hold without the record it belongs to")]
    NoRelated,
    /// Release or commit of more than the record holds; never clamped.
    #[error("more than held: {0}")]
    OverRelease(Uuid),
}

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        match e {
            Error::Db(e) => Self::Database(e),
            other => Self::Internal(other.to_string()),
        }
    }
}

fn related<'a>(cause: &Cause<'a>) -> Result<(&'a str, Uuid), Error> {
    cause.related.ok_or(Error::NoRelated)
}

/// The warehouse-level row (no bin, no lot) of a product.
fn slot(warehouse_id: Uuid, product_id: Uuid) -> Slot {
    Slot {
        product_id,
        warehouse_id,
        bin: None,
        lot_id: None,
    }
}

/// Per product: the summed quantity and the first index asking for it, sorted by product
/// (the slot-key order every caller locks in).
fn totals(wants: &[Want]) -> Vec<(usize, Want)> {
    let mut out: Vec<(usize, Want)> = Vec::new();
    for (i, w) in wants.iter().enumerate() {
        match out.iter_mut().find(|(_, t)| t.product_id == w.product_id) {
            Some((_, t)) => t.quantity += w.quantity,
            None => out.push((i, *w)),
        }
    }
    out.sort_by_key(|(_, w)| w.product_id);
    out
}

/// Locked rows a product's hold may come from, in pick order. Reserves come from the
/// warehouse-level row (bin NULL) only: phase 3 relies on that.
async fn pick(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    product_id: Uuid,
) -> Result<Vec<StockRow>, sqlx::Error> {
    let row = repo::locked(conn, workspace_id, slot(warehouse_id, product_id)).await?;
    Ok(row.into_iter().collect())
}

/// Hold `wants` at the warehouse for `cause.related`: all or nothing. Short when a product's
/// level − reserved is below what is asked (no overselling, store warehouses included); then
/// nothing is written, not even a missing stock row.
pub async fn reserve(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    wants: &[Want],
    cause: &Cause<'_>,
) -> Result<Reserved, Error> {
    let (domain, id) = related(cause)?;
    let mut picked = Vec::new();
    for (index, w) in totals(wants) {
        let rows = pick(conn, workspace_id, warehouse_id, w.product_id).await?;
        let available: Decimal = rows.iter().map(|r| r.stock_lvl - r.reserved).sum();
        if available < w.quantity {
            return Ok(Reserved::Short { index, available });
        }
        picked.push((w, rows));
    }
    for (w, rows) in picked {
        let mut left = w.quantity;
        for row in rows {
            let take = left.min(row.stock_lvl - row.reserved);
            if take <= Decimal::ZERO {
                continue;
            }
            sqlx::query!(
                "INSERT INTO stock_reservations (workspace_id, stock_id, product_id,
                                                 related_domain, related_id, quantity)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                workspace_id,
                row.id,
                w.product_id,
                domain,
                id,
                take,
            )
            .execute(&mut *conn)
            .await?;
            shift(conn, workspace_id, &row, take, "reserved").await?;
            left -= take;
            if left.is_zero() {
                break;
            }
        }
    }
    Ok(Reserved::Done)
}

/// Give back part or all of `cause.related`'s hold, oldest reservation rows first.
pub async fn release(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    wants: &[Want],
    cause: &Cause<'_>,
) -> Result<(), Error> {
    let related = related(cause)?;
    for (_, w) in totals(wants) {
        for (row, take) in consume(conn, workspace_id, warehouse_id, related, w).await? {
            shift(conn, workspace_id, &row, -take, "released").await?;
        }
    }
    Ok(())
}

/// Take held stock: the level drops (ledger `decrease`, related to `cause`) and the hold goes,
/// from exactly the rows the hold was taken from.
pub async fn commit(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    wants: &[Want],
    cause: &Cause<'_>,
) -> Result<(), Error> {
    let related = related(cause)?;
    for (_, w) in totals(wants) {
        for (row, take) in consume(conn, workspace_id, warehouse_id, related, w).await? {
            let after = row.stock_lvl - take;
            set_reserved(conn, workspace_id, row.id, -take).await?;
            repo::set_quantities(
                conn,
                workspace_id,
                row.id,
                after,
                row.minimal_stock_level,
                row.maximum_stock_level,
            )
            .await?;
            let movement = Movement {
                kind: "decrease",
                change: Some(-take),
                before: row.stock_lvl,
                after,
                min: (None, None),
                max: (None, None),
                reason: cause.reason,
                cause,
            };
            repo::record(conn, workspace_id, &row, &movement).await?;
        }
    }
    Ok(())
}

/// Put returned stock back on the warehouse-level row (ledger `increase`).
pub async fn restock(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    wants: &[Want],
    cause: &Cause<'_>,
) -> Result<(), Error> {
    let wants = totals(wants);
    let slots: Vec<Slot> = wants
        .iter()
        .map(|(_, w)| slot(warehouse_id, w.product_id))
        .collect();
    repo::lock_all(conn, workspace_id, &slots).await?;
    for (_, w) in wants {
        let s = slot(warehouse_id, w.product_id);
        repo::mutate(conn, workspace_id, s, w.quantity, Floor::None, cause).await?;
    }
    Ok(())
}

/// Shrink `related`'s reservation rows of `want.product_id` at the warehouse by
/// `want.quantity`, oldest first; the locked stock rows and how much came off each. Errs when
/// they hold less.
async fn consume(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    (domain, id): (&str, Uuid),
    want: Want,
) -> Result<Vec<(StockRow, Decimal)>, Error> {
    let held = sqlx::query!(
        "SELECT r.id, r.stock_id, r.quantity FROM stock_reservations r
         JOIN stocks s ON s.workspace_id = r.workspace_id AND s.id = r.stock_id
         WHERE r.workspace_id = $1 AND r.related_domain = $2 AND r.related_id = $3
           AND r.product_id = $4 AND s.warehouse_id = $5
         ORDER BY r.created_at, r.id
         FOR UPDATE OF r",
        workspace_id,
        domain,
        id,
        want.product_id,
        warehouse_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total: Decimal = held.iter().map(|h| h.quantity).sum();
    if total < want.quantity {
        return Err(Error::OverRelease(want.product_id));
    }
    let mut left = want.quantity;
    let mut out = Vec::new();
    for h in held {
        if left.is_zero() {
            break;
        }
        let take = left.min(h.quantity);
        let row = lock_row(conn, workspace_id, h.stock_id).await?;
        if take == h.quantity {
            sqlx::query!("DELETE FROM stock_reservations WHERE id = $1", h.id)
                .execute(&mut *conn)
                .await?;
        } else {
            sqlx::query!(
                "UPDATE stock_reservations SET quantity = quantity - $2 WHERE id = $1",
                h.id,
                take,
            )
            .execute(&mut *conn)
            .await?;
        }
        out.push((row, take));
        left -= take;
    }
    Ok(out)
}

/// A stock row by id under a row lock (live or soft-deleted: a hold keeps its row).
async fn lock_row(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<StockRow, sqlx::Error> {
    sqlx::query_as!(
        StockRow,
        "SELECT id, product_id, warehouse_id, shelf_id, bin_id, lot_id, stock_lvl, reserved,
                minimal_stock_level, maximum_stock_level
         FROM stocks WHERE workspace_id = $1 AND id = $2
         FOR NO KEY UPDATE",
        workspace_id,
        id,
    )
    .fetch_one(conn)
    .await
}

async fn set_reserved(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    change: Decimal,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE stocks SET reserved = reserved + $3, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
        change,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// Move a locked row's hold by `change` and tell subscribers availability moved.
async fn shift(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    row: &StockRow,
    change: Decimal,
    reason: &'static str,
) -> Result<(), sqlx::Error> {
    set_reserved(conn, workspace_id, row.id, change).await?;
    let payload = serde_json::json!({
        "stock_id": row.id,
        "product_id": row.product_id,
        "warehouse_id": row.warehouse_id,
        "lot_id": row.lot_id,
        "reason": reason,
        "quantity_change": "0.00",
        "quantity_after": format!("{:.2}", row.stock_lvl),
    });
    webhooks::events::emit(conn, workspace_id, webhooks::events::STOCK_CHANGED, payload).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sqlx::PgPool;

    use super::*;

    /// A workspace, two products with `level` each in one warehouse.
    async fn fixture(pool: &PgPool, level: i64) -> (Uuid, Uuid, Uuid, Uuid) {
        let (ws, a, b, wh): (Uuid, Uuid, Uuid, Uuid) = sqlx::query_as(
            "WITH u AS (INSERT INTO users (email, first_name, last_name, password_hash)
                        VALUES ('ada@example.com', 'Ada', 'L', 'x') RETURNING id),
                  w AS (INSERT INTO workspaces (name, created_by_user_id)
                        SELECT 'acme', id FROM u RETURNING id),
                  c AS (INSERT INTO currencies (workspace_id, name, asset_type)
                        SELECT id, 'Euro', 'EUR' FROM w RETURNING id, workspace_id),
                  p AS (INSERT INTO products (workspace_id, product_number, product_appendix,
                                              product_nr_formatted, name, currency_id)
                        SELECT workspace_id, n, '{}', 'PRO-' || n, 'P' || n, c.id
                        FROM c, generate_series(1, 2) n RETURNING id, workspace_id, name),
                  m AS (INSERT INTO warehouses (workspace_id, name, slug)
                        SELECT id, 'Main', 'main' FROM w RETURNING id)
             SELECT a.workspace_id, a.id, b.id, m.id
             FROM p a, p b, m WHERE a.name = 'P1' AND b.name = 'P2'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        for p in [a, b] {
            repo::mutate(
                &mut tx,
                ws,
                slot(wh, p),
                Decimal::from(level),
                Floor::None,
                &Cause::default(),
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        (ws, a, b, wh)
    }

    async fn levels(pool: &PgPool, product: Uuid) -> (String, String) {
        sqlx::query_as(
            "SELECT stock_lvl::text, reserved::text FROM stocks
             WHERE product_id = $1 AND bin_id IS NULL AND deleted_at IS NULL",
        )
        .bind(product)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Σ reservation rows of `order` for `product`.
    async fn held(pool: &PgPool, order: Uuid, product: Uuid) -> String {
        sqlx::query_scalar(
            "SELECT coalesce(sum(quantity), 0)::numeric(19,2)::text FROM stock_reservations
             WHERE related_domain = 'order' AND related_id = $1 AND product_id = $2",
        )
        .bind(order)
        .bind(product)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn want(product_id: Uuid, q: i64) -> Want {
        Want {
            product_id,
            quantity: Decimal::from(q),
        }
    }

    fn order(id: u128) -> Cause<'static> {
        Cause {
            related: Some(("order", Uuid::from_u128(id))),
            ..Cause::default()
        }
    }

    #[test]
    fn totals_sum_per_product_keep_first_index_and_sort() {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let t = totals(&[want(b, 2), want(a, 1), want(b, 3)]);
        let t: Vec<(usize, Uuid, Decimal)> = t
            .iter()
            .map(|(i, w)| (*i, w.product_id, w.quantity))
            .collect();
        assert_eq!(t, [(1, a, Decimal::ONE), (0, b, Decimal::from(5))]);
    }

    #[sqlx::test]
    async fn reserve_release_commit_and_restock_move_level_hold_and_rows(pool: PgPool) {
        let (ws, a, b, wh) = fixture(&pool, 5).await;
        let o = order(9);
        let mut tx = pool.begin().await.unwrap();
        let r = reserve(&mut tx, ws, wh, &[want(a, 2), want(a, 1)], &o)
            .await
            .unwrap();
        assert_eq!(r, Reserved::Done);
        tx.commit().await.unwrap();
        assert_eq!(levels(&pool, a).await, ("5.00".into(), "3.00".into()));
        assert_eq!(held(&pool, Uuid::from_u128(9), a).await, "3.00");

        let mut tx = pool.begin().await.unwrap();
        let r = reserve(&mut tx, ws, wh, &[want(a, 3)], &order(10))
            .await
            .unwrap();
        assert!(matches!(r, Reserved::Short { index: 0, .. }), "{r:?}");
        release(&mut tx, ws, wh, &[want(a, 1)], &o).await.unwrap();
        commit(&mut tx, ws, wh, &[want(a, 2)], &o).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(levels(&pool, a).await, ("3.00".into(), "0.00".into()));
        assert_eq!(held(&pool, Uuid::from_u128(9), a).await, "0.00");
        assert_eq!(levels(&pool, b).await, ("5.00".into(), "0.00".into()));

        let mut tx = pool.begin().await.unwrap();
        restock(&mut tx, ws, wh, &[want(a, 1)], &o).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(levels(&pool, a).await, ("4.00".into(), "0.00".into()));
        let related: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM stock_movements WHERE related_domain = 'order'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            related, 2,
            "commit and restock write the ledger; holds don't"
        );
    }

    #[sqlx::test]
    async fn short_names_the_first_short_line_and_writes_nothing(pool: PgPool) {
        let (ws, a, b, wh) = fixture(&pool, 5).await;
        let none = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO products (workspace_id, product_number, product_appendix,
                                   product_nr_formatted, name, currency_id)
             SELECT workspace_id, 3, '{}', 'PRO-3', 'P3', currency_id FROM products LIMIT 1
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let r = reserve(&mut tx, ws, wh, &[want(a, 1), want(b, 6)], &order(1))
            .await
            .unwrap();
        assert_eq!(
            r,
            Reserved::Short {
                index: 1,
                available: Decimal::from(5)
            }
        );
        let r = reserve(&mut tx, ws, wh, &[want(none, 1)], &order(1))
            .await
            .unwrap();
        assert_eq!(
            r,
            Reserved::Short {
                index: 0,
                available: Decimal::ZERO
            }
        );
        tx.commit().await.unwrap();
        let written: (i64, i64, String) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM stock_reservations),
                    (SELECT count(*) FROM stocks WHERE product_id = $1),
                    (SELECT sum(reserved)::text FROM stocks)",
        )
        .bind(none)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(written, (0, 0, "0.00".into()));
    }

    #[sqlx::test]
    async fn holds_need_their_order_and_never_clamp(pool: PgPool) {
        let (ws, a, _, wh) = fixture(&pool, 5).await;
        let mut tx = pool.begin().await.unwrap();
        let bare = Cause::default();
        let w = [want(a, 1)];
        assert!(matches!(
            reserve(&mut tx, ws, wh, &w, &bare).await,
            Err(Error::NoRelated)
        ));
        assert!(matches!(
            release(&mut tx, ws, wh, &w, &bare).await,
            Err(Error::NoRelated)
        ));
        assert!(matches!(
            commit(&mut tx, ws, wh, &w, &bare).await,
            Err(Error::NoRelated)
        ));
        let o = order(1);
        reserve(&mut tx, ws, wh, &[want(a, 2)], &o).await.unwrap();
        assert!(matches!(
            release(&mut tx, ws, wh, &[want(a, 3)], &o).await,
            Err(Error::OverRelease(p)) if p == a
        ));
        assert!(matches!(
            commit(&mut tx, ws, wh, &[want(a, 1)], &order(2)).await,
            Err(Error::OverRelease(_))
        ));
    }

    /// Only this file writes `stocks.reserved` or `stock_reservations`.
    #[test]
    fn nothing_else_writes_holds() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut stack = vec![std::path::PathBuf::from(root)];
        let mut writers = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
                let writes = [
                    "reserved = reserved",
                    "SET reserved",
                    "INTO stock_reservations",
                    "FROM stock_reservations WHERE id",
                    "UPDATE stock_reservations",
                ];
                if writes.iter().any(|w| compact.contains(w)) {
                    writers.push(path.file_name().unwrap().to_string_lossy().into_owned());
                }
            }
        }
        assert_eq!(writers, ["reserve.rs"]);
    }

    /// Two orders with the same products in opposite line order, 50 of each at once: sorted
    /// locks mean every one finishes (no 40P01); each order's rows sum to its lines, then 0.
    #[sqlx::test]
    async fn opposite_line_orders_never_deadlock(pool: PgPool) {
        let (ws, a, b, wh) = fixture(&pool, 1000).await;
        let pool = Arc::new(pool);
        let mut tasks = Vec::new();
        for i in 0..100u128 {
            let pool = pool.clone();
            let lines = if i % 2 == 0 {
                [want(a, 1), want(b, 2)]
            } else {
                [want(b, 2), want(a, 1)]
            };
            tasks.push(tokio::spawn(async move {
                let o = order(i);
                let mut tx = pool.begin().await?;
                let r = reserve(&mut tx, ws, wh, &lines, &o).await?;
                assert_eq!(r, Reserved::Done);
                tx.commit().await?;
                assert_eq!(held(&pool, Uuid::from_u128(i), a).await, "1.00");
                assert_eq!(held(&pool, Uuid::from_u128(i), b).await, "2.00");
                let mut tx = pool.begin().await?;
                let mut back = lines;
                back.reverse();
                release(&mut tx, ws, wh, &back, &o).await?;
                tx.commit().await?;
                Ok::<_, Error>(())
            }));
        }
        for t in tasks {
            t.await.unwrap().unwrap();
        }
        assert_eq!(levels(&pool, a).await, ("1000.00".into(), "0.00".into()));
        assert_eq!(levels(&pool, b).await, ("1000.00".into(), "0.00".into()));
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM stock_reservations")
            .fetch_one(&*pool)
            .await
            .unwrap();
        assert_eq!(rows, 0);
    }

    /// Ten buyers at once for five units: exactly five holds, one row each.
    #[sqlx::test]
    async fn parallel_reserves_never_oversell(pool: PgPool) {
        let (ws, a, _, wh) = fixture(&pool, 5).await;
        let pool = Arc::new(pool);
        let tasks: Vec<_> = (0..10u128)
            .map(|i| {
                let pool = pool.clone();
                tokio::spawn(async move {
                    let mut tx = pool.begin().await.unwrap();
                    let r = reserve(&mut tx, ws, wh, &[want(a, 1)], &order(i))
                        .await
                        .unwrap();
                    tx.commit().await.unwrap();
                    r == Reserved::Done
                })
            })
            .collect();
        let mut done = 0;
        for t in tasks {
            done += usize::from(t.await.unwrap());
        }
        assert_eq!(done, 5);
        assert_eq!(levels(&pool, a).await, ("5.00".into(), "5.00".into()));
        let rows: (i64, String) =
            sqlx::query_as("SELECT count(*), sum(quantity)::text FROM stock_reservations")
                .fetch_one(&*pool)
                .await
                .unwrap();
        assert_eq!(rows, (5, "5.00".into()));
    }
}
