//! farm_villages and farm_village_capacities SQL. Every statement filters `workspace_id`.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::PgExecutor;
use uuid::Uuid;

/// A stored village with its current capacity (all None when it has none or the caller may
/// not see it).
pub struct VillageRow {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub latitude: Option<Decimal>,
    pub longitude: Option<Decimal>,
    pub primary_language: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub capacity_kw: Option<Decimal>,
    pub basis: Option<String>,
    pub simultaneity_factor: Option<Decimal>,
    pub source_note: Option<String>,
    pub effective_from: Option<NaiveDate>,
}

impl VillageRow {
    /// Same village with its capacity left out.
    pub fn without_capacity(self) -> Self {
        Self {
            capacity_kw: None,
            basis: None,
            simultaneity_factor: None,
            source_note: None,
            effective_from: None,
            ..self
        }
    }
}

/// Fields of a new village; `id` None = server picks.
pub struct NewVillage<'a> {
    pub id: Option<Uuid>,
    pub code: &'a str,
    pub name: &'a str,
    pub latitude: Option<Decimal>,
    pub longitude: Option<Decimal>,
    pub primary_language: &'a str,
}

/// Changed fields; None = keep. Coordinates: Some(None) = clear.
#[derive(Default)]
pub struct VillagePatch {
    pub code: Option<String>,
    pub name: Option<String>,
    pub latitude: Option<Option<Decimal>>,
    pub longitude: Option<Option<Decimal>>,
    pub primary_language: Option<String>,
}

/// A capacity figure; None = column default (planned, 1.000, today).
pub struct NewCapacity {
    pub capacity_kw: Decimal,
    pub basis: Option<String>,
    pub simultaneity_factor: Option<Decimal>,
    pub source_note: Option<String>,
    pub effective_from: Option<NaiveDate>,
}

/// A stored capacity row.
pub struct CapacityRow {
    pub capacity_kw: Decimal,
    pub basis: String,
    pub simultaneity_factor: Decimal,
    pub source_note: Option<String>,
    pub effective_from: NaiveDate,
}

/// One page of villages by code, and the count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<VillageRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        VillageRow,
        r#"SELECT v.id, v.code, v.name, v.latitude, v.longitude, v.primary_language,
                  v.created_at, v.updated_at, c.capacity_kw AS "capacity_kw?", c.basis AS "basis?",
                  c.simultaneity_factor AS "simultaneity_factor?", c.source_note,
                  c.effective_from AS "effective_from?"
           FROM farm_villages v
           LEFT JOIN farm_village_capacities c ON c.village_id = v.id AND c.is_current
           WHERE v.workspace_id = $1
           ORDER BY v.code, v.id LIMIT $2 OFFSET $3"#,
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_villages WHERE workspace_id = $1"#,
        workspace_id
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// One village of this workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<VillageRow>, sqlx::Error> {
    sqlx::query_as!(
        VillageRow,
        r#"SELECT v.id, v.code, v.name, v.latitude, v.longitude, v.primary_language,
                  v.created_at, v.updated_at, c.capacity_kw AS "capacity_kw?", c.basis AS "basis?",
                  c.simultaneity_factor AS "simultaneity_factor?", c.source_note,
                  c.effective_from AS "effective_from?"
           FROM farm_villages v
           LEFT JOIN farm_village_capacities c ON c.village_id = v.id AND c.is_current
           WHERE v.workspace_id = $1 AND v.id = $2"#,
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Lock a village of this workspace for a capacity change; false when it isn't there.
pub async fn lock(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar!(
        "SELECT id FROM farm_villages WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await?
    .is_some())
}

/// Insert a village; its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    v: &NewVillage<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_villages (id, workspace_id, code, name, latitude, longitude, primary_language)
         VALUES (coalesce($1, uuidv7()), $2, $3, $4, $5, $6, $7)
         RETURNING id",
        v.id,
        workspace_id,
        v.code,
        v.name,
        v.latitude,
        v.longitude,
        v.primary_language,
    )
    .fetch_one(db)
    .await
}

/// Apply a patch; None when the village isn't in this workspace.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    p: &VillagePatch,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "UPDATE farm_villages SET
             code = coalesce($3, code),
             name = coalesce($4, name),
             latitude = CASE WHEN $5 THEN $6 ELSE latitude END,
             longitude = CASE WHEN $7 THEN $8 ELSE longitude END,
             primary_language = coalesce($9, primary_language),
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2
         RETURNING id",
        workspace_id,
        id,
        p.code,
        p.name,
        p.latitude.is_some(),
        p.latitude.flatten(),
        p.longitude.is_some(),
        p.longitude.flatten(),
        p.primary_language,
    )
    .fetch_optional(db)
    .await
}

/// Retire the village's current capacity and record `c` as current. Caller holds the village
/// lock, so two writers never race for the one current row.
pub async fn replace_capacity(
    db: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    village_id: Uuid,
    created_by: Uuid,
    c: &NewCapacity,
) -> Result<CapacityRow, sqlx::Error> {
    sqlx::query!(
        "UPDATE farm_village_capacities SET is_current = false
         WHERE workspace_id = $1 AND village_id = $2 AND is_current",
        workspace_id,
        village_id,
    )
    .execute(&mut *db)
    .await?;
    sqlx::query_as!(
        CapacityRow,
        "INSERT INTO farm_village_capacities (workspace_id, village_id, capacity_kw, basis,
             simultaneity_factor, source_note, effective_from, created_by)
         VALUES ($1, $2, $3, coalesce($4, 'planned'), coalesce($5, 1.000), $6,
                 coalesce($7, current_date), $8)
         RETURNING capacity_kw, basis, simultaneity_factor, source_note, effective_from",
        workspace_id,
        village_id,
        c.capacity_kw,
        c.basis,
        c.simultaneity_factor,
        c.source_note,
        c.effective_from,
        created_by,
    )
    .fetch_one(&mut *db)
    .await
}
