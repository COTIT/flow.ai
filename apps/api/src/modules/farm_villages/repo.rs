//! farm_villages SQL. Every statement filters `workspace_id`.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgExecutor;
use uuid::Uuid;

/// A stored village.
pub struct VillageRow {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub latitude: Option<Decimal>,
    pub longitude: Option<Decimal>,
    pub primary_language: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

/// One page of villages by code, and the count.
pub async fn list(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<VillageRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        VillageRow,
        "SELECT id, code, name, latitude, longitude, primary_language, created_at, updated_at
         FROM farm_villages WHERE workspace_id = $1
         ORDER BY code, id LIMIT $2 OFFSET $3",
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
        "SELECT id, code, name, latitude, longitude, primary_language, created_at, updated_at
         FROM farm_villages WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Insert a village.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    v: &NewVillage<'_>,
) -> Result<VillageRow, sqlx::Error> {
    sqlx::query_as!(
        VillageRow,
        "INSERT INTO farm_villages (id, workspace_id, code, name, latitude, longitude, primary_language)
         VALUES (coalesce($1, uuidv7()), $2, $3, $4, $5, $6, $7)
         RETURNING id, code, name, latitude, longitude, primary_language, created_at, updated_at",
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
) -> Result<Option<VillageRow>, sqlx::Error> {
    sqlx::query_as!(
        VillageRow,
        "UPDATE farm_villages SET
             code = coalesce($3, code),
             name = coalesce($4, name),
             latitude = CASE WHEN $5 THEN $6 ELSE latitude END,
             longitude = CASE WHEN $7 THEN $8 ELSE longitude END,
             primary_language = coalesce($9, primary_language),
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2
         RETURNING id, code, name, latitude, longitude, primary_language, created_at, updated_at",
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
