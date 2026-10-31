//! Cash SQL: vaults, drawer shifts and `cash_ledger` inserts (the table refuses updates and
//! deletes). Every query filters `workspace_id`. Drawer writes run under the register's row
//! lock, vault writes under the vault's, register first.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A shift; `expected` is live for an open one.
pub struct ShiftRow {
    pub id: Uuid,
    pub register_id: Uuid,
    pub opened_by: Uuid,
    pub opened_at: DateTime<Utc>,
    pub opening: i64,
    pub expected: i64,
    pub closed_by: Option<Uuid>,
    pub closed_at: Option<DateTime<Utc>>,
    pub counted: Option<i64>,
    pub difference: Option<i64>,
}

/// A vault with its balance.
pub struct VaultRow {
    pub id: Uuid,
    pub name: String,
    pub currency_id: Uuid,
    pub balance: i64,
    pub created_at: DateTime<Utc>,
}

/// What a ledger insert books: the place, the kind and its links.
pub struct Entry {
    pub register_id: Option<Uuid>,
    pub shift_id: Option<Uuid>,
    pub vault_id: Option<Uuid>,
    pub kind: &'static str,
    pub transfer_id: Option<Uuid>,
    pub till_user_id: Uuid,
}

/// Lock the register row: one drawer write at a time per register.
pub async fn lock_register(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "SELECT id FROM till_registers WHERE workspace_id = $1 AND id = $2 FOR NO KEY UPDATE",
        workspace_id,
        id,
    )
    .fetch_one(conn)
    .await
    .map(|_| ())
}

/// The register's open shift, expected cash included.
pub async fn open_shift(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    register_id: Uuid,
) -> Result<Option<ShiftRow>, sqlx::Error> {
    sqlx::query_as!(
        ShiftRow,
        r#"SELECT s.id, s.register_id, s.opened_by, s.opened_at, s.opening,
                  (SELECT coalesce(sum(l.amount), 0)::bigint FROM cash_ledger l
                   WHERE l.workspace_id = s.workspace_id AND l.shift_id = s.id
                     AND l.kind <> 'close_count') AS "expected!",
                  s.closed_by, s.closed_at, s.counted, s.difference
           FROM drawer_shifts s
           WHERE s.workspace_id = $1 AND s.register_id = $2 AND s.closed_at IS NULL"#,
        workspace_id,
        register_id,
    )
    .fetch_optional(db)
    .await
}

/// Start a shift; its id.
pub async fn insert_shift(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    register_id: Uuid,
    opened_by: Uuid,
    opening: i64,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO drawer_shifts (workspace_id, register_id, opened_by, opening)
         VALUES ($1, $2, $3, $4) RETURNING id",
        workspace_id,
        register_id,
        opened_by,
        opening,
    )
    .fetch_one(db)
    .await
}

/// Book count lines at `entry`'s place; `sign` -1 takes them out, 1 puts them in.
pub async fn book(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    entry: &Entry,
    lines: &[(i64, i64)],
    sign: i64,
) -> Result<(), sqlx::Error> {
    let (denominations, counts): (Vec<i64>, Vec<i32>) = lines
        .iter()
        .map(|(d, c)| (*d, i32::try_from(*c).unwrap_or(i32::MAX)))
        .unzip();
    sqlx::query!(
        "INSERT INTO cash_ledger (workspace_id, register_id, shift_id, vault_id, kind,
                                  transfer_id, denomination, count, amount, till_user_id)
         SELECT $1, $2, $3, $4, $5, $6, d, c, $9 * d * c, $7
         FROM unnest($8::bigint[], $10::int[]) AS t (d, c)",
        workspace_id,
        entry.register_id,
        entry.shift_id,
        entry.vault_id,
        entry.kind,
        entry.transfer_id,
        entry.till_user_id,
        &denominations,
        sign,
        &counts,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Close a shift with its count.
pub async fn close_shift(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    closed_by: Uuid,
    expected: i64,
    counted: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE drawer_shifts SET closed_by = $3, closed_at = now(), expected = $4, counted = $5,
                                  difference = $5::bigint - $4::bigint
         WHERE workspace_id = $1 AND id = $2 AND closed_at IS NULL",
        workspace_id,
        id,
        closed_by,
        expected,
        counted,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// A shift by id (expected live while open).
pub async fn shift(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<ShiftRow, sqlx::Error> {
    sqlx::query_as!(
        ShiftRow,
        r#"SELECT s.id, s.register_id, s.opened_by, s.opened_at, s.opening,
                  coalesce(s.expected,
                           (SELECT coalesce(sum(l.amount), 0)::bigint FROM cash_ledger l
                            WHERE l.workspace_id = s.workspace_id AND l.shift_id = s.id
                              AND l.kind <> 'close_count')) AS "expected!",
                  s.closed_by, s.closed_at, s.counted, s.difference
           FROM drawer_shifts s WHERE s.workspace_id = $1 AND s.id = $2"#,
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await
}

/// One page of shifts, newest first, and the total.
pub async fn list_shifts(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ShiftRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT s.id, s.register_id, s.opened_by, s.opened_at, s.opening,
                  coalesce(s.expected,
                           (SELECT coalesce(sum(l.amount), 0)::bigint FROM cash_ledger l
                            WHERE l.workspace_id = s.workspace_id AND l.shift_id = s.id
                              AND l.kind <> 'close_count')) AS "expected!",
                  s.closed_by, s.closed_at, s.counted, s.difference,
                  count(*) OVER () AS "total!"
           FROM drawer_shifts s WHERE s.workspace_id = $1
           ORDER BY s.opened_at DESC, s.id DESC
           LIMIT $2 OFFSET $3"#,
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let rows = rows
        .into_iter()
        .map(|s| ShiftRow {
            id: s.id,
            register_id: s.register_id,
            opened_by: s.opened_by,
            opened_at: s.opened_at,
            opening: s.opening,
            expected: s.expected,
            closed_by: s.closed_by,
            closed_at: s.closed_at,
            counted: s.counted,
            difference: s.difference,
        })
        .collect();
    Ok((rows, total))
}

/// Store a vault; its id and creation time.
pub async fn insert_vault(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    name: &str,
    currency_id: Uuid,
    created_by: Uuid,
) -> Result<(Uuid, DateTime<Utc>), sqlx::Error> {
    let r = sqlx::query!(
        "INSERT INTO cash_vaults (workspace_id, name, currency_id, created_by)
         VALUES ($1, $2, $3, $4) RETURNING id, created_at",
        workspace_id,
        name,
        currency_id,
        created_by,
    )
    .fetch_one(db)
    .await?;
    Ok((r.id, r.created_at))
}

/// Every vault of the workspace with its balance (a shop has a handful).
pub async fn list_vaults(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Vec<VaultRow>, sqlx::Error> {
    sqlx::query_as!(
        VaultRow,
        r#"SELECT v.id, v.name, v.currency_id, v.created_at,
                  (SELECT coalesce(sum(l.amount), 0)::bigint FROM cash_ledger l
                   WHERE l.workspace_id = v.workspace_id AND l.vault_id = v.id) AS "balance!"
           FROM cash_vaults v WHERE v.workspace_id = $1 ORDER BY v.name, v.id LIMIT 1000"#,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// A vault of the workspace under a row lock: its currency and balance.
pub async fn lock_vault(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<(Uuid, i64)>, sqlx::Error> {
    let found = sqlx::query_scalar!(
        "SELECT currency_id FROM cash_vaults WHERE workspace_id = $1 AND id = $2 FOR NO KEY UPDATE",
        workspace_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(currency_id) = found else {
        return Ok(None);
    };
    let balance = sqlx::query_scalar!(
        r#"SELECT coalesce(sum(amount), 0)::bigint AS "b!" FROM cash_ledger
           WHERE workspace_id = $1 AND vault_id = $2"#,
        workspace_id,
        id,
    )
    .fetch_one(conn)
    .await?;
    Ok(Some((currency_id, balance)))
}
