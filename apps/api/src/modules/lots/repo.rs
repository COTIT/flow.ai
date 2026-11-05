//! SQL for `lots` and the append-only `lot_events` trail. Writes run in the caller's
//! transaction so a change and its event commit together.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::users::dto::full_name;

/// A lot row (passport columns excluded).
#[derive(Clone, Debug)]
pub struct LotRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub code: String,
    pub public_code: String,
    pub status: String,
    pub produced_on: Option<NaiveDate>,
    pub best_before: Option<NaiveDate>,
    pub expires_on: Option<NaiveDate>,
    pub origin: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A trail row.
pub struct EventRow {
    pub id: Uuid,
    pub kind: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub user_id: Option<Uuid>,
    /// The user's full name while they are a member of the workspace.
    pub user_name: Option<String>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
}

/// A trail row to append.
pub struct NewEvent<'a> {
    pub lot_id: Uuid,
    pub kind: &'static str,
    pub from_status: Option<&'a str>,
    pub to_status: Option<&'a str>,
    pub user_id: Uuid,
    pub meta: Option<Value>,
}

/// List filters; `q` is a code prefix (LIKE wildcards escaped by the caller).
pub struct ListFilter<'a> {
    pub product_id: Option<Uuid>,
    pub status: Option<&'a str>,
    pub q: Option<&'a str>,
}

/// Insert a lot; None when `public_code` collided (caller retries with a new one).
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    l: &LotRow,
    created_by: Uuid,
) -> Result<Option<LotRow>, sqlx::Error> {
    sqlx::query_as!(
        LotRow,
        "INSERT INTO lots (workspace_id, product_id, code, public_code, status, produced_on,
                           best_before, expires_on, origin, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (public_code) DO NOTHING
         RETURNING id, product_id, code, public_code, status, produced_on, best_before,
                   expires_on, origin, created_at, updated_at",
        workspace_id,
        l.product_id,
        l.code,
        l.public_code,
        l.status,
        l.produced_on,
        l.best_before,
        l.expires_on,
        l.origin,
        created_by,
    )
    .fetch_optional(conn)
    .await
}

/// One lot of the workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<LotRow>, sqlx::Error> {
    sqlx::query_as!(
        LotRow,
        "SELECT id, product_id, code, public_code, status, produced_on, best_before, expires_on,
                origin, created_at, updated_at
         FROM lots WHERE id = $1 AND workspace_id = $2",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// One lot of the workspace under `FOR NO KEY UPDATE`.
pub async fn locked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<LotRow>, sqlx::Error> {
    sqlx::query_as!(
        LotRow,
        "SELECT id, product_id, code, public_code, status, produced_on, best_before, expires_on,
                origin, created_at, updated_at
         FROM lots WHERE id = $1 AND workspace_id = $2
         FOR NO KEY UPDATE",
        id,
        workspace_id,
    )
    .fetch_optional(conn)
    .await
}

/// Write the editable fields of a lot the caller holds locked.
pub async fn update(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    l: &LotRow,
) -> Result<LotRow, sqlx::Error> {
    sqlx::query_as!(
        LotRow,
        "UPDATE lots SET code = $3, status = $4, produced_on = $5, best_before = $6,
                         expires_on = $7, origin = $8, updated_at = now()
         WHERE id = $1 AND workspace_id = $2
         RETURNING id, product_id, code, public_code, status, produced_on, best_before,
                   expires_on, origin, created_at, updated_at",
        l.id,
        workspace_id,
        l.code,
        l.status,
        l.produced_on,
        l.best_before,
        l.expires_on,
        l.origin,
    )
    .fetch_one(conn)
    .await
}

/// Lots packed from `parent` (one level), under a row lock, in id order.
pub async fn children_locked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    parent: Uuid,
) -> Result<Vec<LotRow>, sqlx::Error> {
    sqlx::query_as!(
        LotRow,
        "SELECT id, product_id, code, public_code, status, produced_on, best_before, expires_on,
                origin, created_at, updated_at
         FROM lots WHERE workspace_id = $1 AND parent_lot_id = $2
         ORDER BY id
         FOR NO KEY UPDATE",
        workspace_id,
        parent,
    )
    .fetch_all(conn)
    .await
}

/// A locked lot's new status.
pub async fn set_status(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE lots SET status = $3, updated_at = now() WHERE id = $1 AND workspace_id = $2",
        id,
        workspace_id,
        status,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// A transfer of a lot out of this workspace: who received (or will receive) it.
pub struct SentRow {
    pub id: Uuid,
    pub to_workspace_id: Uuid,
    pub to_lot_id: Option<Uuid>,
}

/// Transfers of `lot` to other workspaces, oldest first.
pub async fn sent(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    lot: Uuid,
) -> Result<Vec<SentRow>, sqlx::Error> {
    sqlx::query_as!(
        SentRow,
        "SELECT id, to_workspace_id, to_lot_id FROM supply_transfers
         WHERE from_workspace_id = $1 AND from_lot_id = $2
         ORDER BY dispatched_at, id",
        workspace_id,
        lot,
    )
    .fetch_all(db)
    .await
}

/// An order still holding stock of a lot.
pub struct HoldRow {
    pub order_id: Uuid,
    pub number: String,
    pub warehouse_id: Uuid,
    pub quantity: String,
}

/// Orders holding stock on `lot`, per warehouse, oldest hold first.
pub async fn holds(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    lot: Uuid,
) -> Result<Vec<HoldRow>, sqlx::Error> {
    sqlx::query_as!(
        HoldRow,
        r#"SELECT o.id AS order_id, o.number, s.warehouse_id,
                  sum(r.quantity)::numeric(19,2)::text AS "quantity!"
           FROM stock_reservations r
           JOIN stocks s ON s.workspace_id = r.workspace_id AND s.id = r.stock_id
           JOIN orders o ON o.workspace_id = r.workspace_id AND o.id = r.related_id
           WHERE r.workspace_id = $1 AND s.lot_id = $2 AND r.related_domain = 'order'
           GROUP BY o.id, o.number, s.warehouse_id
           ORDER BY min(r.created_at), o.id"#,
        workspace_id,
        lot,
    )
    .fetch_all(db)
    .await
}

/// One page, newest first, with the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LotRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, product_id, code, public_code, status, produced_on, best_before,
                  expires_on, origin, created_at, updated_at, count(*) OVER () AS "total!"
           FROM lots
           WHERE workspace_id = $1
             AND ($2::uuid IS NULL OR product_id = $2)
             AND ($3::text IS NULL OR status = $3)
             AND ($4::text IS NULL OR lower(code) LIKE lower($4) || '%')
           ORDER BY id DESC
           LIMIT $5 OFFSET $6"#,
        workspace_id,
        f.product_id,
        f.status,
        f.q,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let lots = rows
        .into_iter()
        .map(|r| LotRow {
            id: r.id,
            product_id: r.product_id,
            code: r.code,
            public_code: r.public_code,
            status: r.status,
            produced_on: r.produced_on,
            best_before: r.best_before,
            expires_on: r.expires_on,
            origin: r.origin,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((lots, total))
}

/// Append one trail row in the caller's transaction.
pub async fn add_event(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    e: &NewEvent<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO lot_events (workspace_id, lot_id, kind, from_status, to_status, user_id, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        workspace_id,
        e.lot_id,
        e.kind,
        e.from_status,
        e.to_status,
        e.user_id,
        e.meta,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// One page of a lot's trail, newest first, with the total.
pub async fn events(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    lot_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EventRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT e.id, e.kind, e.from_status, e.to_status, e.user_id, e.meta, e.created_at,
                  u.first_name AS "first_name?", u.middle_name, u.last_name AS "last_name?",
                  count(*) OVER () AS "total!"
           FROM lot_events e
           LEFT JOIN workspace_members m ON m.workspace_id = e.workspace_id AND m.user_id = e.user_id
           LEFT JOIN users u ON u.id = m.user_id AND u.deleted_at IS NULL
           WHERE e.lot_id = $1 AND e.workspace_id = $2
           ORDER BY e.created_at DESC, e.id DESC
           LIMIT $3 OFFSET $4"#,
        lot_id,
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let events = rows
        .into_iter()
        .map(|r| EventRow {
            id: r.id,
            kind: r.kind,
            from_status: r.from_status,
            to_status: r.to_status,
            user_id: r.user_id,
            user_name: r
                .first_name
                .zip(r.last_name)
                .map(|(f, l)| full_name(&f, r.middle_name.as_deref(), &l)),
            meta: r.meta,
            created_at: r.created_at,
        })
        .collect();
    Ok((events, total))
}

/// A lot's passport columns.
pub struct PassportRow {
    pub draft: Value,
    pub published: Option<Value>,
    pub version: i32,
    pub published_at: Option<DateTime<Utc>>,
}

/// The passport columns of one lot of the workspace.
pub async fn passport(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<PassportRow>, sqlx::Error> {
    sqlx::query_as!(
        PassportRow,
        "SELECT passport_draft AS draft, passport AS published, passport_version AS version,
                published_at
         FROM lots WHERE id = $1 AND workspace_id = $2",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Replace the draft; None when the lot is not this workspace's.
pub async fn set_draft(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    draft: &Value,
) -> Result<Option<PassportRow>, sqlx::Error> {
    sqlx::query_as!(
        PassportRow,
        "UPDATE lots SET passport_draft = $3, updated_at = now()
         WHERE id = $1 AND workspace_id = $2
         RETURNING passport_draft AS draft, passport AS published,
                   passport_version AS version, published_at",
        id,
        workspace_id,
        draft,
    )
    .fetch_optional(db)
    .await
}

/// The draft becomes the published copy with the next version (lot locked by the caller).
pub async fn publish(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<PassportRow, sqlx::Error> {
    sqlx::query_as!(
        PassportRow,
        "UPDATE lots SET passport = passport_draft, passport_version = passport_version + 1,
                         published_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2
         RETURNING passport_draft AS draft, passport AS published,
                   passport_version AS version, published_at",
        id,
        workspace_id,
    )
    .fetch_one(conn)
    .await
}

/// Drop the published copy; the draft and version stay (lot locked by the caller).
pub async fn unpublish(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<PassportRow, sqlx::Error> {
    sqlx::query_as!(
        PassportRow,
        "UPDATE lots SET passport = NULL, published_at = NULL, updated_at = now()
         WHERE id = $1 AND workspace_id = $2
         RETURNING passport_draft AS draft, passport AS published,
                   passport_version AS version, published_at",
        id,
        workspace_id,
    )
    .fetch_one(conn)
    .await
}

/// A published lot for the public page, with its product's public fields.
pub struct PublicRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub code: String,
    pub public_code: String,
    pub status: String,
    pub produced_on: Option<NaiveDate>,
    pub best_before: Option<NaiveDate>,
    pub expires_on: Option<NaiveDate>,
    pub origin: Option<String>,
    pub passport: Value,
    pub version: i32,
    pub published_at: DateTime<Utc>,
    pub product_name: String,
    pub brand: Option<String>,
    pub short_description: Option<String>,
}

/// The published lot with this public code (of `workspace_id` when given): one statement,
/// so unknown, unpublished and foreign codes cost the same.
pub async fn public(
    db: impl PgExecutor<'_>,
    public_code: &str,
    workspace_id: Option<Uuid>,
) -> Result<Option<PublicRow>, sqlx::Error> {
    sqlx::query_as!(
        PublicRow,
        r#"SELECT l.id, l.workspace_id, l.code, l.public_code, l.status, l.produced_on, l.best_before,
                  l.expires_on, l.origin, l.passport AS "passport!", l.passport_version AS version,
                  l.published_at AS "published_at!", p.name AS product_name, p.brand,
                  p.short_description
           FROM lots l
           JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
                AND p.deleted_at IS NULL
           WHERE l.public_code = $1 AND l.passport IS NOT NULL
             AND ($2::uuid IS NULL OR l.workspace_id = $2)"#,
        public_code,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// True once any stock row (live or deleted) was made for the lot.
pub async fn has_stock(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM stocks WHERE workspace_id = $1 AND lot_id = $2) AS "has!""#,
        workspace_id,
        id,
    )
    .fetch_one(db)
    .await
}

/// A lot's stock in one warehouse, summed over its rows there; 2-decimal text.
pub struct FreeRow {
    pub warehouse_id: Uuid,
    pub level: String,
    pub reserved: String,
    pub available: String,
}

/// The lot's live stock per warehouse, by warehouse name.
pub async fn free_stock(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    lot: Uuid,
) -> Result<Vec<FreeRow>, sqlx::Error> {
    sqlx::query_as!(
        FreeRow,
        r#"SELECT s.warehouse_id,
                  sum(s.stock_lvl)::numeric(19,2)::text AS "level!",
                  sum(s.reserved)::numeric(19,2)::text AS "reserved!",
                  (sum(s.stock_lvl) - sum(s.reserved))::numeric(19,2)::text AS "available!"
           FROM stocks s
           JOIN warehouses w ON w.id = s.warehouse_id
           WHERE s.workspace_id = $1 AND s.lot_id = $2 AND s.deleted_at IS NULL
           GROUP BY s.warehouse_id, w.name
           ORDER BY w.name, s.warehouse_id"#,
        workspace_id,
        lot,
    )
    .fetch_all(db)
    .await
}

/// A sale or return of this lot that credited or debited farmers, as the selling workspace may
/// see it: no farm-side ids, kg, prices or amounts.
pub struct CreditRow {
    pub order_id: Uuid,
    pub order_number: String,
    pub till_ref: Option<String>,
    pub kind: String,
    pub jars: String,
    pub at: DateTime<Utc>,
}

/// A page of the farmer credits written for `lot_id` sold by `workspace_id`, newest first, and
/// the total.
pub async fn farmer_credits(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    lot_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CreditRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT a.order_id, a.order_number, t.client_ref AS "till_ref?", a.kind,
                  a.jars::text AS "jars!", a.created_at AS at, count(*) OVER () AS "total!"
           FROM farm_sale_attributions a
           LEFT JOIN till_sales t ON t.workspace_id = a.sold_workspace_id
                                 AND t.order_id = a.order_id
           WHERE a.sold_workspace_id = $1 AND a.sold_lot_id = $2
           ORDER BY a.created_at DESC, a.id DESC
           LIMIT $3 OFFSET $4"#,
        workspace_id,
        lot_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let credits = rows
        .into_iter()
        .map(|r| CreditRow {
            order_id: r.order_id,
            order_number: r.order_number,
            till_ref: r.till_ref,
            kind: r.kind,
            jars: r.jars,
            at: r.at,
        })
        .collect();
    Ok((credits, total))
}

/// Where a lot came from: its packing parent and pack, or the supply transfer it was received
/// from with the sender's lot codes (the receiver sees those in `/supply-transfers` already).
pub struct LineageRow {
    pub lot_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub parent_code: Option<String>,
    pub pack_raw_kg: Option<String>,
    pub pack_units: Option<String>,
    pub transfer_id: Option<Uuid>,
    pub from_lot_code: Option<String>,
    pub from_lot_public_code: Option<String>,
    /// The sender's lot status now (read through the transfer, never written).
    pub from_lot_status: Option<String>,
}

/// [`LineageRow`] of each lot among `ids` of the workspace, in one statement.
pub async fn lineage(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<LineageRow>, sqlx::Error> {
    sqlx::query_as!(
        LineageRow,
        r#"SELECT l.id AS lot_id, p.id AS "parent_id?", p.code AS "parent_code?",
                  l.pack_raw_kg::text AS pack_raw_kg, l.pack_units::text AS pack_units,
                  t.id AS "transfer_id?", f.code AS "from_lot_code?",
                  f.public_code AS "from_lot_public_code?", f.status AS "from_lot_status?"
           FROM lots l
           LEFT JOIN lots p ON p.workspace_id = l.workspace_id AND p.id = l.parent_lot_id
           LEFT JOIN supply_transfers t ON t.to_workspace_id = l.workspace_id
                                       AND t.to_lot_id = l.id
           LEFT JOIN lots f ON f.workspace_id = t.from_workspace_id AND f.id = t.from_lot_id
           WHERE l.workspace_id = $1 AND l.id = ANY($2)"#,
        workspace_id,
        ids,
    )
    .fetch_all(db)
    .await
}
