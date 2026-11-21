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
    /// The register's currency: upper-case ISO 4217 code.
    pub currency: String,
    /// The currency's minor-unit exponent.
    pub decimals: i16,
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
                  s.closed_by, s.closed_at, s.counted, s.difference,
                  upper(c.asset_type) AS "currency!", c.decimals
           FROM drawer_shifts s
           JOIN till_registers r ON r.workspace_id = s.workspace_id AND r.id = s.register_id
           JOIN currencies c ON c.workspace_id = r.workspace_id AND c.id = r.currency_id
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
                  s.closed_by, s.closed_at, s.counted, s.difference,
                  upper(c.asset_type) AS "currency!", c.decimals
           FROM drawer_shifts s
           JOIN till_registers r ON r.workspace_id = s.workspace_id AND r.id = s.register_id
           JOIN currencies c ON c.workspace_id = r.workspace_id AND c.id = r.currency_id
           WHERE s.workspace_id = $1 AND s.id = $2"#,
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await
}

/// Which shifts the office list returns; `None` = any.
#[derive(Default)]
pub struct ShiftFilter {
    /// Still open (`true`) or closed.
    pub open: Option<bool>,
    pub register_id: Option<Uuid>,
    /// `opened_at` at or after.
    pub from: Option<DateTime<Utc>>,
    /// `opened_at` before.
    pub to: Option<DateTime<Utc>>,
}

/// One page of shifts matching `f`, newest first, and the total.
pub async fn list_shifts(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ShiftFilter,
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
                  upper(c.asset_type) AS "currency!", c.decimals,
                  count(*) OVER () AS "total!"
           FROM drawer_shifts s
           JOIN till_registers r ON r.workspace_id = s.workspace_id AND r.id = s.register_id
           JOIN currencies c ON c.workspace_id = r.workspace_id AND c.id = r.currency_id
           WHERE s.workspace_id = $1
             AND ($4::bool IS NULL OR (s.closed_at IS NULL) = $4)
             AND ($5::uuid IS NULL OR s.register_id = $5)
             AND ($6::timestamptz IS NULL OR s.opened_at >= $6)
             AND ($7::timestamptz IS NULL OR s.opened_at < $7)
           ORDER BY s.opened_at DESC, s.id DESC
           LIMIT $2 OFFSET $3"#,
        workspace_id,
        limit,
        offset,
        f.open,
        f.register_id,
        f.from,
        f.to,
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
            currency: s.currency,
            decimals: s.decimals,
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

/// A place's currency, locked for this transaction (register or vault row FOR NO KEY UPDATE).
pub struct PlaceRow {
    pub currency_id: Uuid,
    pub code: String,
    pub decimals: i16,
}

/// Lock the drawer's register or the vault; None when not in the workspace.
pub async fn lock_place(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    register_id: Option<Uuid>,
    vault_id: Option<Uuid>,
) -> Result<Option<PlaceRow>, sqlx::Error> {
    if let Some(id) = register_id {
        return sqlx::query_as!(
            PlaceRow,
            r#"SELECT r.currency_id, upper(c.asset_type) AS "code!", c.decimals
               FROM till_registers r
               JOIN currencies c ON c.workspace_id = r.workspace_id AND c.id = r.currency_id
               WHERE r.workspace_id = $1 AND r.id = $2 FOR NO KEY UPDATE OF r"#,
            workspace_id,
            id,
        )
        .fetch_optional(conn)
        .await;
    }
    sqlx::query_as!(
        PlaceRow,
        r#"SELECT v.currency_id, upper(c.asset_type) AS "code!", c.decimals
           FROM cash_vaults v
           JOIN currencies c ON c.workspace_id = v.workspace_id AND c.id = v.currency_id
           WHERE v.workspace_id = $1 AND v.id = $2 FOR NO KEY UPDATE OF v"#,
        workspace_id,
        vault_id,
    )
    .fetch_optional(conn)
    .await
}

/// A count against a place's active book cash, all in SQL. `counted`/`difference`/`amount`
/// are None when the major-unit text has more decimals than the currency or overflows.
pub struct CountFacts {
    pub expected: i64,
    pub counted: Option<i64>,
    pub difference: Option<i64>,
    /// |difference|: the journal amount.
    pub amount: Option<i64>,
}

/// Active balance (reviewed opening + place rows from the cutover, counts excluded; masterplan §8 ACTIVE CASH BALANCE) and the count in minor units: Office
/// `major` text × 10^decimals when whole, else Till `minor`.
pub async fn count_facts(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    register_id: Option<Uuid>,
    vault_id: Option<Uuid>,
    major: Option<rust_decimal::Decimal>,
    minor: Option<i64>,
    decimals: i16,
) -> Result<CountFacts, sqlx::Error> {
    sqlx::query_as!(
        CountFacts,
        r#"WITH b AS (
               SELECT (coalesce((SELECT sum(l.amount) FROM cash_ledger l
                                 JOIN accounting_settings s ON s.workspace_id = l.workspace_id
                                 WHERE l.workspace_id = $1 AND l.kind NOT IN ('open_count', 'close_count')
                                   AND (l.register_id = $2 OR l.vault_id = $3)
                                   AND (s.state <> 'active' OR s.cutover_at IS NULL
                                        OR l.created_at >= s.cutover_at)), 0)
                       + coalesce((SELECT sum(o.amount) FROM cash_place_openings o
                                   WHERE o.workspace_id = $1
                                     AND (o.register_id = $2 OR o.vault_id = $3)), 0))::bigint
                      AS expected),
           m AS (SELECT $4::numeric * 10::numeric ^ $6::int AS v),
           c AS (
               SELECT CASE WHEN $4::numeric IS NULL THEN $5::bigint
                           WHEN m.v = trunc(m.v) AND m.v < 1e18 THEN m.v::bigint END AS counted
               FROM m)
           SELECT b.expected AS "expected!", c.counted, c.counted - b.expected AS difference,
                  abs(c.counted - b.expected) AS amount
           FROM b, c"#,
        workspace_id,
        register_id,
        vault_id,
        major,
        minor,
        i32::from(decimals),
    )
    .fetch_one(conn)
    .await
}

/// Today in the book's timezone (UTC while unset).
pub async fn book_today(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<chrono::NaiveDate, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT (now() AT TIME ZONE coalesce(timezone, 'UTC'))::date AS "d!"
           FROM accounting_settings WHERE workspace_id = $1"#,
        workspace_id,
    )
    .fetch_one(conn)
    .await
}

/// The effect a posted journal entry carries.
pub async fn entry_effect(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    entry_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT effect_id AS "e!" FROM journal_entries WHERE workspace_id = $1 AND id = $2"#,
        workspace_id,
        entry_id,
    )
    .fetch_one(conn)
    .await
}

/// A cash adjustment to store with its ledger row.
pub struct NewAdjustment<'a> {
    pub id: Uuid,
    pub register_id: Option<Uuid>,
    pub vault_id: Option<Uuid>,
    pub currency_id: Uuid,
    pub expected: i64,
    pub counted: i64,
    pub difference: i64,
    pub evidence: &'a str,
    pub reason: &'a str,
    pub date: chrono::NaiveDate,
    pub member_user_id: Uuid,
    pub till_user_id: Option<Uuid>,
    pub effect_id: Uuid,
    pub journal_entry_id: Option<Uuid>,
}

/// Store the adjustment and its place movement (member actor, no shift); its creation time.
pub async fn insert_adjustment(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    a: &NewAdjustment<'_>,
) -> Result<DateTime<Utc>, sqlx::Error> {
    let created_at = sqlx::query_scalar!(
        "INSERT INTO cash_adjustments (id, workspace_id, register_id, vault_id, currency_id,
             expected, counted, difference, count_evidence_ref, reason, accounting_date,
             member_user_id, till_user_id, effect_id, journal_entry_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
         RETURNING created_at",
        a.id,
        workspace_id,
        a.register_id,
        a.vault_id,
        a.currency_id,
        a.expected,
        a.counted,
        a.difference,
        a.evidence,
        a.reason,
        a.date,
        a.member_user_id,
        a.till_user_id,
        a.effect_id,
        a.journal_entry_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query!(
        "INSERT INTO cash_ledger (workspace_id, register_id, vault_id, kind, amount, actor_kind,
             member_user_id, till_user_id, source_kind, source_id, effect_id)
         VALUES ($1, $2, $3, 'adjustment', $4, 'member', $5, $6, 'cash_adjustment', $7, $8)",
        workspace_id,
        a.register_id,
        a.vault_id,
        a.difference,
        a.member_user_id,
        a.till_user_id,
        a.id,
        a.effect_id,
    )
    .execute(conn)
    .await?;
    Ok(created_at)
}

/// A drawer or vault with its active balance.
pub struct PlaceListRow {
    pub kind: String,
    pub id: Uuid,
    pub name: String,
    pub currency_id: Uuid,
    pub code: String,
    pub decimals: i16,
    pub balance: i64,
    pub open_shift_id: Option<Uuid>,
}

/// Every drawer and vault of the workspace (a shop has a handful) with active balances.
pub async fn list_places(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<Vec<PlaceListRow>, sqlx::Error> {
    sqlx::query_as!(
        PlaceListRow,
        r#"SELECT p.kind AS "kind!", p.id AS "id!", p.name AS "name!",
                  p.currency_id AS "currency_id!", upper(c.asset_type) AS "code!",
                  c.decimals AS "decimals!", p.balance AS "balance!", p.open_shift_id
           FROM (
               SELECT 'drawer' AS kind, r.id, r.name, r.currency_id, r.workspace_id,
                      (coalesce((SELECT sum(l.amount) FROM cash_ledger l
                                 WHERE l.workspace_id = r.workspace_id AND l.register_id = r.id
                                   AND l.kind NOT IN ('open_count', 'close_count')
                                   AND (st.state <> 'active' OR st.cutover_at IS NULL
                                        OR l.created_at >= st.cutover_at)), 0)
                       + coalesce((SELECT o.amount FROM cash_place_openings o
                                   WHERE o.workspace_id = r.workspace_id AND o.register_id = r.id), 0)
                      )::bigint AS balance,
                      (SELECT s.id FROM drawer_shifts s
                       WHERE s.register_id = r.id AND s.closed_at IS NULL) AS open_shift_id
               FROM till_registers r
               JOIN accounting_settings st ON st.workspace_id = r.workspace_id
               WHERE r.workspace_id = $1
               UNION ALL
               SELECT 'vault', v.id, v.name, v.currency_id, v.workspace_id,
                      (coalesce((SELECT sum(l.amount) FROM cash_ledger l
                                 WHERE l.workspace_id = v.workspace_id AND l.vault_id = v.id
                                   AND l.kind NOT IN ('open_count', 'close_count')
                                   AND (st.state <> 'active' OR st.cutover_at IS NULL
                                        OR l.created_at >= st.cutover_at)), 0)
                       + coalesce((SELECT o.amount FROM cash_place_openings o
                                   WHERE o.workspace_id = v.workspace_id AND o.vault_id = v.id), 0)
                      )::bigint,
                      NULL::uuid
               FROM cash_vaults v
               JOIN accounting_settings st ON st.workspace_id = v.workspace_id
               WHERE v.workspace_id = $1
           ) p
           JOIN currencies c ON c.workspace_id = p.workspace_id AND c.id = p.currency_id
           ORDER BY p.kind, p.name, p.id
           LIMIT 2000"#,
        workspace_id,
    )
    .fetch_all(db)
    .await
}

/// Cash due for `total` (minor units) in steps of `step` minor units, half up: 1.02 → 1.00,
/// 1.03 → 1.05 at step 5. Step 1 or below = no rounding. Callers pass `total` ≥ 0.
pub async fn round_cash(
    db: impl PgExecutor<'_>,
    total: i64,
    step: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT LEAST(money_round_div($1::bigint::numeric, GREATEST($2::bigint, 1)::numeric) * GREATEST($2::bigint, 1),
                        9223372036854775807)::bigint AS "due!""#,
        total,
        step,
    )
    .fetch_one(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    /// Cash rounding runs in SQL (`money_round_div`), half up to the step; step < 1 means none.
    #[sqlx::test]
    async fn cash_rounds_half_up_to_the_step_in_sql(pool: PgPool) {
        for (total, step, due) in [
            (1002, 5, 1000),
            (1003, 5, 1005),
            (1005, 5, 1005),
            (1007, 5, 1005),
            (1008, 5, 1010),
            (1005, 10, 1010),
            (1004, 10, 1000),
            (12_345, 1, 12_345),
            (0, 5, 0),
            (2, 5, 0),
            (3, 5, 5),
            // exact half steps round up
            (1025, 50, 1050),
            (1024, 50, 1000),
            (1050, 100, 1100),
            (1049, 100, 1000),
            (1, 2, 2),
            // step 0 or below: no rounding
            (1003, 0, 1003),
            (1003, -5, 1003),
            // largest refund amount
            (1_000_000_000_000_000_000, 5, 1_000_000_000_000_000_000),
            (999_999_999_999_999_998, 5, 1_000_000_000_000_000_000),
            // clamps like the old i64::MAX fallback
            (i64::MAX, i64::MAX / 2, i64::MAX - 1),
            (i64::MAX, 6_148_914_691_236_517_204, i64::MAX),
        ] {
            let got = round_cash(&pool, total, step).await.unwrap();
            assert_eq!(got, due, "{total} step {step}");
        }
    }
}
