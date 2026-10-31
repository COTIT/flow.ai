//! Payout, void and cash ledger SQL. All three tables are insert-only (a trigger refuses the
//! rest).

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A payout with its currency and void, if any.
pub struct PayoutRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub person_id: Uuid,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub amount: i64,
    pub id_type: String,
    pub client_ref: Uuid,
    pub request_hash: Vec<u8>,
    pub paid_by: Uuid,
    pub paid_at: DateTime<Utc>,
    pub void_reason: Option<String>,
    pub voided_by: Option<Uuid>,
    pub voided_at: Option<DateTime<Utc>>,
}

/// A person's household facts for the four-eyes check, for one staff member.
pub struct HouseholdFacts {
    pub registered: bool,
    pub verified: bool,
    pub own: bool,
}

/// A new payout.
pub struct NewPayout<'a> {
    pub village_id: Uuid,
    pub person_id: Uuid,
    pub currency_id: Uuid,
    pub amount: i64,
    pub id_type: &'a str,
    pub client_ref: Uuid,
    pub request_hash: &'a [u8],
    pub paid_by: Uuid,
}

/// A cash ledger row.
pub struct Movement {
    pub village_id: Uuid,
    pub staff_user_id: Uuid,
    pub currency_id: Uuid,
    pub day: NaiveDate,
    pub kind: &'static str,
    pub amount: i64,
    pub payout_id: Option<Uuid>,
    pub created_by: Uuid,
}

/// One officer's day in one currency.
pub struct DayRow {
    pub staff_user_id: Uuid,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub opening: i64,
    pub floats: i64,
    pub payouts: i64,
    pub paid: i64,
    pub voided: i64,
    pub returned: i64,
    pub counted: Option<i64>,
}

/// The payout `id` (or with `client_ref`) of the workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Option<Uuid>,
    client_ref: Option<Uuid>,
) -> Result<Option<PayoutRow>, sqlx::Error> {
    sqlx::query_as!(
        PayoutRow,
        r#"SELECT p.id, p.village_id, p.person_id, p.currency_id, c.asset_type AS currency,
                  c.decimals, p.amount, p.id_type, p.client_ref, p.request_hash, p.paid_by,
                  p.paid_at, v.reason AS "void_reason?", v.voided_by AS "voided_by?",
                  v.voided_at AS "voided_at?"
           FROM farm_payouts p
           JOIN currencies c ON c.workspace_id = p.workspace_id AND c.id = p.currency_id
           LEFT JOIN farm_payout_voids v ON v.payout_id = p.id
           WHERE p.workspace_id = $1 AND (p.id = $2 OR p.client_ref = $3)"#,
        workspace_id,
        id,
        client_ref,
    )
    .fetch_optional(db)
    .await
}

/// Lock a live person of the workspace (serialises payouts per farmer); their village.
pub async fn lock_person(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    person_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT village_id FROM farm_persons
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL FOR UPDATE",
        workspace_id,
        person_id,
    )
    .fetch_optional(conn)
    .await
}

/// Whether `me` (or `my_person`) registered, verified or belongs to a household of `person_id`.
pub async fn household_facts(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    person_id: Uuid,
    me: Uuid,
    my_person: Option<Uuid>,
) -> Result<HouseholdFacts, sqlx::Error> {
    sqlx::query_as!(
        HouseholdFacts,
        r#"SELECT coalesce(bool_or(h.captured_by = $3), false) AS "registered!",
                  coalesce(bool_or(h.verified_by = $3), false) AS "verified!",
                  coalesce(bool_or(EXISTS (SELECT 1 FROM farm_household_members o
                                           WHERE o.household_id = h.id AND o.person_id = $4)),
                           false) AS "own!"
           FROM farm_household_members m
           JOIN farm_households h ON h.workspace_id = m.workspace_id AND h.id = m.household_id
           WHERE m.workspace_id = $1 AND m.person_id = $2"#,
        workspace_id,
        person_id,
        me,
        my_person,
    )
    .fetch_one(db)
    .await
}

/// A person's passbook balance in one currency. Its own statement, after the person lock.
pub async fn balance(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    person_id: Uuid,
    currency_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT coalesce(sum(amount), 0)::bigint AS "balance!" FROM farm_passbook_entries
           WHERE workspace_id = $1 AND person_id = $2 AND currency_id = $3"#,
        workspace_id,
        person_id,
        currency_id,
    )
    .fetch_one(conn)
    .await
}

/// Insert a payout; its id.
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    p: &NewPayout<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_payouts (workspace_id, village_id, person_id, currency_id, amount,
                                   id_type, client_ref, request_hash, paid_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
        workspace_id,
        p.village_id,
        p.person_id,
        p.currency_id,
        p.amount,
        p.id_type,
        p.client_ref,
        p.request_hash,
        p.paid_by,
    )
    .fetch_one(conn)
    .await
}

/// A passbook line for a payout (`kind` payout or payout_void, signed `amount`).
pub async fn passbook_line(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    p: &PayoutRow,
    kind: &str,
    amount: i64,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO farm_passbook_entries (workspace_id, village_id, person_id, currency_id,
                                            kind, amount, payout_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        workspace_id,
        p.village_id,
        p.person_id,
        p.currency_id,
        kind,
        amount,
        p.id,
        by,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Record the void; false when the payout was already void.
pub async fn insert_void(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    payout_id: Uuid,
    reason: &str,
    by: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "INSERT INTO farm_payout_voids (payout_id, workspace_id, reason, voided_by)
         VALUES ($1, $2, $3, $4) ON CONFLICT (payout_id) DO NOTHING",
        payout_id,
        workspace_id,
        reason,
        by,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Append a cash ledger row; its id.
pub async fn move_cash(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    m: &Movement,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_cash_ledger (workspace_id, village_id, staff_user_id, currency_id, day,
                                       kind, amount, payout_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
        workspace_id,
        m.village_id,
        m.staff_user_id,
        m.currency_id,
        m.day,
        m.kind,
        m.amount,
        m.payout_id,
        m.created_by,
    )
    .fetch_one(db)
    .await
}

/// True when `user_id` holds a live staff role (officer of `village_id`, ops or admin).
pub async fn is_staff_of(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    village_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_memberships
                          WHERE workspace_id = $1 AND user_id = $2 AND revoked_at IS NULL
                            AND (role IN ('ops', 'admin')
                                 OR (role = 'field_officer' AND village_id = $3))) AS "ok!""#,
        workspace_id,
        user_id,
        village_id,
    )
    .fetch_one(db)
    .await
}

/// Each officer's `day` per currency (of `village_id` when given; only `staff` when given):
/// the balance carried in, the day's movements, and the last count of the day.
pub async fn day(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    day: NaiveDate,
    village_id: Option<Uuid>,
    staff: Option<Uuid>,
) -> Result<Vec<DayRow>, sqlx::Error> {
    sqlx::query_as!(
        DayRow,
        r#"SELECT l.staff_user_id, l.currency_id, c.asset_type AS currency, c.decimals,
                  coalesce(sum(l.amount) FILTER (WHERE l.day < $2 AND l.kind <> 'count'), 0)::bigint
                      AS "opening!",
                  coalesce(sum(l.amount) FILTER (WHERE l.day = $2 AND l.kind = 'float_out'), 0)::bigint
                      AS "floats!",
                  count(*) FILTER (WHERE l.day = $2 AND l.kind = 'payout') AS "payouts!",
                  coalesce(-sum(l.amount) FILTER (WHERE l.day = $2 AND l.kind = 'payout'), 0)::bigint
                      AS "paid!",
                  coalesce(sum(l.amount) FILTER (WHERE l.day = $2 AND l.kind = 'payout_void'), 0)::bigint
                      AS "voided!",
                  coalesce(-sum(l.amount) FILTER (WHERE l.day = $2 AND l.kind = 'float_back'), 0)::bigint
                      AS "returned!",
                  (array_agg(l.amount ORDER BY l.created_at DESC, l.id DESC)
                       FILTER (WHERE l.day = $2 AND l.kind = 'count'))[1] AS counted
           FROM farm_cash_ledger l
           JOIN currencies c ON c.workspace_id = l.workspace_id AND c.id = l.currency_id
           WHERE l.workspace_id = $1 AND l.day <= $2
             AND ($3::uuid IS NULL OR l.village_id = $3)
             AND ($4::uuid IS NULL OR l.staff_user_id = $4)
           GROUP BY l.staff_user_id, l.currency_id, c.asset_type, c.decimals
           ORDER BY l.staff_user_id, l.currency_id"#,
        workspace_id,
        day,
        village_id,
        staff,
    )
    .fetch_all(db)
    .await
}
