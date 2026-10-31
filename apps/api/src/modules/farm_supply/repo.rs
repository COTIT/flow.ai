//! Supply batch SQL. Batches and shares are insert-only (a trigger refuses the rest).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::http::farm::Scope;

/// A stored batch with its lot's codes.
pub struct BatchRow {
    pub id: Uuid,
    pub client_ref: Uuid,
    pub village_id: Uuid,
    pub crop_id: Uuid,
    pub lot_id: Uuid,
    pub lot_code: String,
    pub public_code: String,
    pub price_per_kg: i64,
    pub currency_id: Uuid,
    pub kg_total: Decimal,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

/// One share line.
pub struct LineRow {
    pub batch_id: Uuid,
    pub harvest_report_id: Uuid,
    pub person_id: Uuid,
    pub kg: Decimal,
}

/// Who used a client_ref, and with which body.
pub struct RefRow {
    pub id: Uuid,
    pub created_by: Uuid,
    pub request_hash: Vec<u8>,
}

/// A harvest report as a batch line needs it, locked.
pub struct HarvestRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub kind: String,
    pub is_current: bool,
    pub verification: String,
    pub verified_by: Option<Uuid>,
    pub deleted: bool,
    pub crop_id: Uuid,
    /// Primary manager of the harvest's farm, if the farm has one.
    pub person_id: Option<Uuid>,
    pub quantity_kg: Decimal,
}

/// A new batch row.
pub struct NewBatchRow<'a> {
    pub client_ref: Uuid,
    pub request_hash: &'a [u8],
    pub village_id: Uuid,
    pub crop_id: Uuid,
    pub lot_id: Uuid,
    pub price_per_kg: i64,
    pub currency_id: Uuid,
    pub kg_total: Decimal,
    pub created_by: Uuid,
}

/// True when `crop_id` is a crop of the workspace.
pub async fn crop_exists(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    crop_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM farm_crops WHERE workspace_id = $1 AND id = $2) AS "ok!""#,
        workspace_id,
        crop_id,
    )
    .fetch_one(db)
    .await
}

/// The batch that used `client_ref` in this workspace.
pub async fn find_ref(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    client_ref: Uuid,
) -> Result<Option<RefRow>, sqlx::Error> {
    sqlx::query_as!(
        RefRow,
        "SELECT id, created_by, request_hash FROM farm_supply_batches
         WHERE workspace_id = $1 AND client_ref = $2",
        workspace_id,
        client_ref,
    )
    .fetch_optional(db)
    .await
}

/// Lock the workspace's harvest reports among `ids` (id order, so two batches never deadlock)
/// and read what a line checks. Unknown and foreign ids are simply missing.
pub async fn lock_harvests(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<HarvestRow>, sqlx::Error> {
    sqlx::query_as!(
        HarvestRow,
        r#"SELECT h.id, h.village_id, h.kind, h.is_current, h.verification, h.verified_by,
                  h.deleted_at IS NOT NULL AS "deleted!", c.crop_id,
                  m.person_id AS "person_id?", h.quantity_kg
           FROM farm_harvest_reports h
           JOIN farm_crop_cycles c ON c.workspace_id = h.workspace_id AND c.id = h.crop_cycle_id
           JOIN farm_plots p ON p.workspace_id = c.workspace_id AND p.id = c.plot_id
           LEFT JOIN farm_managers m ON m.farm_id = p.farm_id AND m.is_primary
           WHERE h.workspace_id = $1 AND h.id = ANY($2)
           ORDER BY h.id
           FOR UPDATE OF h"#,
        workspace_id,
        ids,
    )
    .fetch_all(conn)
    .await
}

/// Kg already committed per harvest. Its own statement, after the lock: it must see what
/// transactions that held the lock before us committed.
pub async fn committed(
    conn: &mut PgConnection,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, Decimal)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", farm_harvest_committed_kg(id) AS "kg!"
           FROM unnest($1::uuid[]) AS id"#,
        ids,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.kg)).collect())
}

/// Insert the batch row; its id.
pub async fn insert_batch(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    b: &NewBatchRow<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_supply_batches (workspace_id, client_ref, request_hash, village_id,
                                          crop_id, lot_id, price_per_kg, currency_id, kg_total,
                                          created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
        workspace_id,
        b.client_ref,
        b.request_hash,
        b.village_id,
        b.crop_id,
        b.lot_id,
        b.price_per_kg,
        b.currency_id,
        b.kg_total,
        b.created_by,
    )
    .fetch_one(conn)
    .await
}

/// Insert the share lines `(harvest, person, kg)` of a batch, one statement.
pub async fn insert_lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    village_id: Uuid,
    batch_id: Uuid,
    lines: &[(Uuid, Uuid, Decimal)],
) -> Result<(), sqlx::Error> {
    let harvests: Vec<Uuid> = lines.iter().map(|l| l.0).collect();
    let persons: Vec<Uuid> = lines.iter().map(|l| l.1).collect();
    let kgs: Vec<Decimal> = lines.iter().map(|l| l.2).collect();
    sqlx::query!(
        "INSERT INTO farm_supply_shares (workspace_id, village_id, batch_id, harvest_report_id,
                                         person_id, kg)
         SELECT $1, $2, $3, h, p, kg FROM unnest($4::uuid[], $5::uuid[], $6::numeric[]) AS t(h, p, kg)",
        workspace_id,
        village_id,
        batch_id,
        &harvests,
        &persons,
        &kgs,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// One batch of the workspace inside `scope`.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    scope: &Scope,
) -> Result<Option<BatchRow>, sqlx::Error> {
    sqlx::query_as!(
        BatchRow,
        "SELECT b.id, b.client_ref, b.village_id, b.crop_id, b.lot_id, l.code AS lot_code,
                l.public_code, b.price_per_kg, b.currency_id, b.kg_total, b.created_by,
                b.created_at
         FROM farm_supply_batches b
         JOIN lots l ON l.workspace_id = b.workspace_id AND l.id = b.lot_id
         WHERE b.workspace_id = $1 AND b.id = $2 AND ($3::bool OR b.village_id = ANY($4))",
        workspace_id,
        id,
        scope.all,
        &scope.villages,
    )
    .fetch_optional(db)
    .await
}

/// One page of batches inside `scope`, newest first, and the count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    scope: &Scope,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BatchRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        BatchRow,
        "SELECT b.id, b.client_ref, b.village_id, b.crop_id, b.lot_id, l.code AS lot_code,
                l.public_code, b.price_per_kg, b.currency_id, b.kg_total, b.created_by,
                b.created_at
         FROM farm_supply_batches b
         JOIN lots l ON l.workspace_id = b.workspace_id AND l.id = b.lot_id
         WHERE b.workspace_id = $1 AND ($2::bool OR b.village_id = ANY($3))
         ORDER BY b.created_at DESC, b.id DESC LIMIT $4 OFFSET $5",
        workspace_id,
        scope.all,
        &scope.villages,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_supply_batches
           WHERE workspace_id = $1 AND ($2::bool OR village_id = ANY($3))"#,
        workspace_id,
        scope.all,
        &scope.villages,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// Lines of the given batches, by batch then harvest id.
pub async fn lines(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    batch_ids: &[Uuid],
) -> Result<Vec<LineRow>, sqlx::Error> {
    sqlx::query_as!(
        LineRow,
        "SELECT batch_id, harvest_report_id, person_id, kg FROM farm_supply_shares
         WHERE workspace_id = $1 AND batch_id = ANY($2)
         ORDER BY batch_id, harvest_report_id",
        workspace_id,
        batch_ids,
    )
    .fetch_all(db)
    .await
}

/// Farmers of a supply lot per village (for the public page): count and harvest window.
pub struct GroupRow {
    pub village: String,
    pub farmers: i64,
    pub harvest_start: Option<chrono::NaiveDate>,
    pub harvest_end: Option<chrono::NaiveDate>,
}

/// The farmer groups of `lot_id`'s batch, by village name; empty for a lot without a batch.
pub async fn public_groups(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    lot_id: Uuid,
) -> Result<Vec<GroupRow>, sqlx::Error> {
    sqlx::query_as!(
        GroupRow,
        r#"SELECT v.name AS village, count(DISTINCT s.person_id) AS "farmers!",
                  min(c.harvest_start) AS harvest_start, max(c.harvest_end) AS harvest_end
           FROM farm_supply_batches b
           JOIN farm_supply_shares s ON s.batch_id = b.id
           JOIN farm_villages v ON v.workspace_id = s.workspace_id AND v.id = s.village_id
           JOIN farm_harvest_reports h ON h.id = s.harvest_report_id
           JOIN farm_crop_cycles c ON c.id = h.crop_cycle_id
           WHERE b.workspace_id = $1 AND b.lot_id = $2
           GROUP BY v.id, v.name
           ORDER BY v.name"#,
        workspace_id,
        lot_id,
    )
    .fetch_all(db)
    .await
}
